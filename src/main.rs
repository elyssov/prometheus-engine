// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Cat in the Apartment (playable demo)
//
// Spawn a ChibiCat inside a П-44 apartment. WASD drives the cat
// (A/D turn, W/S forward/back, Shift = run).  Mouse-drag orbits
// the camera around the cat.  SPACE = paw swipe with a flat
// billboarded crosshair 1.5 body-lengths ahead.
//
// Tab — fly-through manual camera (legacy god-mode for debugging).
//
// 1 voxel = 1 cm.  CAT_SCALE = 4  →  cat ~31 cm at the ears.
// ═══════════════════════════════════════════════════════════════

mod core;

use glam::{Mat4, Quat, Vec3};
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use core::brick::BrickModel;
use core::apartment::build_apartment;
use core::cat::build_chibi_cat;
use core::skeleton::Skeleton;
use core::damage::{Damage, Durability};
use core::meshing::{ChunkMesh, MeshVertex};
use core::render_mesh::{self, GpuMesh, MeshUniforms};

// ─── Tuning constants ────────────────────────────────────────

const CAT_SCALE: f32 = 4.0;
const CAT_WALK_SPEED: f32 = 120.0; // cm/s (so 3 body-lengths per second, cat-appropriate)
const CAT_RUN_SPEED:  f32 = 260.0;
const CAT_TURN_SPEED: f32 = 8.0;   // rad/s — how fast cat yaw chases movement direction
const CAT_COLLIDER_RADIUS: f32 = 10.0; // XZ collision cylinder (cm)
const CAT_COLLIDER_Y_LO:   f32 = 4.0;  // body lower bound for world-brick cross-check
const CAT_COLLIDER_Y_HI:   f32 = 28.0; // body upper bound

const SWIPE_DURATION:    f32 = 0.55;
const SWIPE_REACH_CM:    f32 = 4.5 * CAT_SCALE;   // ≈ 1.5 body-lengths in front of the muzzle
const CROSSHAIR_SIZE_CM: f32 = 7.0;
const CAT_FEET_Y:        f32 = 0.0;               // floor level
const CAT_PELVIS_Y:      f32 = 2.3 * CAT_SCALE;   // thigh+shin+paw height so feet rest on floor

// Apartment living-room centre (see apartment.rs layout).
// Living room spans x=520..900, z=450..800; pelvis sits at the centre on the floor.
const SPAWN_POS: Vec3 = Vec3::new(710.0, CAT_PELVIS_Y, 625.0);

// Camera orbit (3rd-person)
const CAM_DIST_DEFAULT:  f32 = 110.0;
const CAM_PITCH_DEFAULT: f32 = 0.35;  // slight downward tilt
const CAM_HEIGHT_OFFSET: f32 = 25.0;  // look slightly above cat's pelvis

// ─── Cinematic waypoints (kept for Tab=fly mode scene setup) ──
#[derive(Clone, Copy)]
struct Waypoint { pos: Vec3, look: Vec3 }

// ─── Modes ────────────────────────────────────────────────────
#[derive(PartialEq, Eq, Clone, Copy)]
enum Mode {
    Cat,        // default gameplay
    ManualFly,  // fly-through for debug
}

// ─── Input ────────────────────────────────────────────────────
#[derive(Default)]
struct KeysHeld {
    w: bool, a: bool, s: bool, d: bool, q: bool, e: bool,
    shift: bool, space_edge: bool,
}

// ─── App ─────────────────────────────────────────────────────
struct App {
    // GPU / window
    win: Option<Arc<Window>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    pipeline: Option<wgpu::RenderPipeline>,
    uniform_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    depth_view: Option<wgpu::TextureView>,

    // Scene
    apartment: BrickModel,
    apartment_gpu: Option<GpuMesh>,

    cat_sk: Skeleton,
    cat_model: BrickModel,
    cat_gpu: Option<GpuMesh>,
    cat_pos: Vec3,
    cat_yaw: f32,
    cat_moving: bool,

    // Animation
    anim_time: f32,
    swipe_t: Option<f32>,
    swipe_already_hit: bool,

    // Crosshair
    crosshair_gpu: Option<GpuMesh>,

    // Camera
    mode: Mode,
    cam_yaw: f32,        // orbit yaw around cat
    cam_pitch: f32,
    cam_dist: f32,
    fly_pos: Vec3,
    fly_yaw: f32,
    fly_pitch: f32,

    // Input
    keys: KeysHeld,
    mouse_dragging: bool,
    last_mouse: (f64, f64),

    // Timing
    last_frame: Instant,
    fov: f32,
}

impl App {
    fn new() -> Self {
        // Apartment (static)
        let mut apartment = build_apartment();
        apartment.update_static();
        mark_apartment_breakables(&mut apartment);
        println!("  Apartment assembled: {} bricks", apartment.brick_count());

        // Cat rig
        let mut cat_sk = Skeleton::chibi_cat(CAT_SCALE);
        cat_sk.root_position = SPAWN_POS;
        cat_sk.root_rotation = Quat::IDENTITY;
        cat_sk.solve_forward();
        let mut cat_model = build_chibi_cat(&cat_sk, CAT_SCALE);
        cat_model.update(&cat_sk);
        println!("  ChibiCat assembled: {} bricks", cat_model.brick_count());

        Self {
            win: None, device: None, queue: None, surface: None, config: None,
            pipeline: None, uniform_buffer: None, bind_group: None, depth_view: None,

            apartment, apartment_gpu: None,
            cat_sk, cat_model, cat_gpu: None,
            cat_pos: SPAWN_POS, cat_yaw: 0.0, cat_moving: false,

            anim_time: 0.0,
            swipe_t: None,
            swipe_already_hit: false,

            crosshair_gpu: None,

            mode: Mode::Cat,
            cam_yaw: 0.0,
            cam_pitch: CAM_PITCH_DEFAULT,
            cam_dist: CAM_DIST_DEFAULT,
            fly_pos: Vec3::new(450.0, 300.0, -50.0),
            fly_yaw: 0.0,
            fly_pitch: -0.3,

            keys: KeysHeld::default(),
            mouse_dragging: false,
            last_mouse: (0.0, 0.0),

            last_frame: Instant::now(),
            fov: 55.0,
        }
    }

    // ── Cat update (Mario-style, camera-relative) ────────
    fn update_cat(&mut self, dt: f32) {
        if self.mode != Mode::Cat { return; }

        // Direction the camera is looking (horizontal) — WASD relative to it.
        let orbit_yaw = self.cat_yaw + self.cam_yaw;
        let cam_fwd   = Vec3::new(orbit_yaw.sin(), 0.0, orbit_yaw.cos());
        let cam_right = Vec3::new(orbit_yaw.cos(), 0.0, -orbit_yaw.sin());

        let mut mv = Vec3::ZERO;
        if self.keys.w { mv += cam_fwd; }
        if self.keys.s { mv -= cam_fwd; }
        if self.keys.d { mv += cam_right; }
        if self.keys.a { mv -= cam_right; }
        let moving = mv.length_squared() > 0.01;
        if moving { mv = mv.normalize(); }
        self.cat_moving = moving;

        let speed = if self.keys.shift { CAT_RUN_SPEED } else { CAT_WALK_SPEED };
        let desired = self.cat_pos + mv * speed * dt;

        // Outer-wall clamp, then brick-vs-cat collision push-out
        let mut next = desired;
        next.x = next.x.clamp(25.0, 875.0);
        next.z = next.z.clamp(25.0, 775.0);
        next.y = CAT_PELVIS_Y;
        next = resolve_cat_collision(next, &self.apartment, CAT_COLLIDER_RADIUS);
        self.cat_pos = next;

        // Smooth rotate cat yaw toward movement direction
        if moving {
            let target_yaw = mv.x.atan2(mv.z);
            let two_pi = std::f32::consts::TAU;
            let mut delta = target_yaw - self.cat_yaw;
            while delta >  std::f32::consts::PI { delta -= two_pi; }
            while delta < -std::f32::consts::PI { delta += two_pi; }
            let step = (CAT_TURN_SPEED * dt).min(delta.abs());
            self.cat_yaw += step * delta.signum();
            // Normalize yaw
            self.cat_yaw = self.cat_yaw.rem_euclid(two_pi);
        }

        // Advance animation clock
        self.anim_time += dt * if self.keys.shift && self.cat_moving { 2.0 } else { 1.0 };

        // Space starts a swipe — and snaps cat yaw to camera direction so
        // the strike goes where the player is looking.
        if self.keys.space_edge {
            self.keys.space_edge = false;
            if self.swipe_t.is_none() {
                self.cat_yaw = self.cat_yaw + self.cam_yaw;
                self.cam_yaw = 0.0;
                self.swipe_t = Some(0.0);
                self.swipe_already_hit = false;
            }
        }

        // Swipe progress
        if let Some(t) = self.swipe_t {
            let nt = t + dt / SWIPE_DURATION;
            self.swipe_t = if nt >= 1.0 { None } else { Some(nt) };
        }

        // Hit detection during strike window (t≈0.28..0.55)
        self.try_swipe_hit();

        // Flash decay for all hit bricks
        self.apartment.tick_flash(dt);

        // Apply pose — write rotations into skeleton, then solve.
        self.pose_cat_skeleton();
        self.cat_sk.root_position = self.cat_pos;
        self.cat_sk.root_rotation = Quat::from_rotation_y(self.cat_yaw);
        self.cat_sk.solve_forward();

        self.cat_model.update(&self.cat_sk);

        // Per-brick overrides: pump the right front leg during swipe.
        self.apply_brick_overrides();
    }

    fn pose_cat_skeleton(&mut self) {
        // Reset key bones each frame; we author rotations from scratch.
        let reset = [
            "spine1", "spine2", "neck", "head",
            "upper_arm_l", "forearm_l", "upper_arm_r", "forearm_r",
            "thigh_l", "shin_l", "thigh_r", "shin_r",
            "tail1", "tail2", "tail3", "tail4",
        ];
        for name in reset {
            self.cat_sk.bone_mut(name).local_rotation = Quat::IDENTITY;
        }

        let t = self.anim_time;
        let moving = self.cat_moving;

        // Idle: gentle breathing + tail sway
        let breath = (t * 2.0).sin() * 0.03;
        self.cat_sk.bone_mut("spine1").local_rotation =
            Quat::from_rotation_x(breath);
        let tail_sway = (t * 2.2).sin() * 0.5;
        self.cat_sk.bone_mut("tail1").local_rotation = Quat::from_rotation_x(tail_sway * 0.4);
        self.cat_sk.bone_mut("tail2").local_rotation = Quat::from_rotation_x(tail_sway * 0.3);
        self.cat_sk.bone_mut("tail3").local_rotation = Quat::from_rotation_x(tail_sway * 0.2);

        // Walk: alternating leg swing (trot — diagonal pairs FL+BR, FR+BL)
        if moving {
            let freq = if self.keys.shift { 9.0 } else { 5.5 };
            let phase = t * freq;
            let swing = phase.sin() * 0.45;
            let swing_b = (phase + std::f32::consts::PI).sin() * 0.45;

            self.cat_sk.bone_mut("upper_arm_l").local_rotation = Quat::from_rotation_x(swing);
            self.cat_sk.bone_mut("forearm_l").local_rotation  = Quat::from_rotation_x((swing.abs() - 0.1).max(0.0) * 0.9);
            self.cat_sk.bone_mut("upper_arm_r").local_rotation = Quat::from_rotation_x(swing_b);
            self.cat_sk.bone_mut("forearm_r").local_rotation  = Quat::from_rotation_x((swing_b.abs() - 0.1).max(0.0) * 0.9);

            self.cat_sk.bone_mut("thigh_l").local_rotation = Quat::from_rotation_x(-swing_b);
            self.cat_sk.bone_mut("shin_l").local_rotation  = Quat::from_rotation_x((swing_b.abs() - 0.1).max(0.0) * 0.9);
            self.cat_sk.bone_mut("thigh_r").local_rotation = Quat::from_rotation_x(-swing);
            self.cat_sk.bone_mut("shin_r").local_rotation  = Quat::from_rotation_x((swing.abs() - 0.1).max(0.0) * 0.9);
        }

        // Swipe: REARING — pelvis tilts back, pivots on hind legs, right front strikes.
        if let Some(tn) = self.swipe_t {
            // Envelope: rise 0..0.25, strike 0.25..0.5, hold 0.5..0.7, recover 0.7..1
            let rise   = smoothstep((tn / 0.25).clamp(0.0, 1.0))
                       * (1.0 - smoothstep(((tn - 0.7) / 0.3).clamp(0.0, 1.0)));
            let strike = {
                let s = ((tn - 0.25) / 0.25).clamp(0.0, 1.0);
                smoothstep(s) * (1.0 - smoothstep(((tn - 0.6) / 0.3).clamp(0.0, 1.0)))
            };

            // Spine rears back (pitch).  Negative X = tail down, chest up.
            let rear_amt = rise * 1.15;   // ~66°
            let sp = Quat::from_rotation_x(-rear_amt * 0.5);
            self.cat_sk.bone_mut("spine1").local_rotation =
                self.cat_sk.bone_mut("spine1").local_rotation * sp;
            self.cat_sk.bone_mut("spine2").local_rotation = Quat::from_rotation_x(-rear_amt * 0.6);

            // Hind legs brace — thighs compress forward, shins straighten.
            let hind = -rear_amt * 0.4;
            self.cat_sk.bone_mut("thigh_l").local_rotation =
                self.cat_sk.bone_mut("thigh_l").local_rotation * Quat::from_rotation_x(hind);
            self.cat_sk.bone_mut("thigh_r").local_rotation =
                self.cat_sk.bone_mut("thigh_r").local_rotation * Quat::from_rotation_x(hind);

            // Left front: tucked up to chest.
            self.cat_sk.bone_mut("upper_arm_l").local_rotation = Quat::from_rotation_x(-1.1 * rise);
            self.cat_sk.bone_mut("forearm_l").local_rotation  = Quat::from_rotation_x(1.3 * rise);

            // Right front: wind-up (up), then STRIKE (forward).
            let wind = rise * 0.9;                 // shoulder draws back / up during rise
            let pop  = strike * 1.7;               // shoulder slams forward during strike
            self.cat_sk.bone_mut("upper_arm_r").local_rotation =
                Quat::from_rotation_x(-wind + pop);
            self.cat_sk.bone_mut("forearm_r").local_rotation =
                Quat::from_rotation_x(1.3 * wind - 0.9 * strike);

            // Tail whips up for balance.
            self.cat_sk.bone_mut("tail1").local_rotation =
                self.cat_sk.bone_mut("tail1").local_rotation * Quat::from_rotation_x(-0.9 * rise);
            self.cat_sk.bone_mut("tail2").local_rotation =
                Quat::from_rotation_x(-0.7 * rise);
        }
    }

    fn try_swipe_hit(&mut self) {
        if self.swipe_already_hit { return; }
        let tn = match self.swipe_t { Some(t) => t, None => return };
        // Strike window — when the paw is actually out
        if !(0.28..=0.55).contains(&tn) { return; }

        let cat_fwd = Vec3::new(self.cat_yaw.sin(), 0.0, self.cat_yaw.cos());
        // Ray from cat's face forward.  Origin sits at the muzzle height.
        let origin = self.cat_pos
            + cat_fwd * (2.0 * CAT_SCALE)
            + Vec3::Y * CAM_HEIGHT_OFFSET;
        let dir = cat_fwd;
        let max_dist = SWIPE_REACH_CM * 1.4;

        if let Some((idx, dist)) = self.apartment.raycast_breakable(origin, dir, max_dist) {
            let name = self.apartment.bricks[idx].name.clone();
            // cat_paw is a bit anemic (power 0.3) — the cat here is a chunky mascot,
            // buff it for playability.  Equivalent to a solid open-palm smack.
            let smack = Damage::new(1.0, 3.0, core::damage::DamageKind::Blunt);
            if let Some(h) = self.apartment.hit_brick(idx, &smack) {
                println!("  💥 SWIPE! {} @ {:.0}cm → severity={:?} broken={} r={:.1}",
                    name, dist, h.severity, h.broken, h.effective_radius);
            }
        }
        self.swipe_already_hit = true;
    }

    fn apply_brick_overrides(&mut self) {
        let upper_r = self.cat_sk.bone("upper_arm_r").id;
        let forearm_r = self.cat_sk.bone("forearm_r").id;
        let paw_fr = self.cat_sk.bone("paw_fr").id;

        // During swipe, scale up right-front bricks — the "paw vytyagivaetsya pri udare" look.
        let paw_pump = if let Some(tn) = self.swipe_t {
            let s = ((tn - 0.2) / 0.35).clamp(0.0, 1.0);
            let envelope = s * (1.0 - ((tn - 0.55) / 0.25).clamp(0.0, 1.0));
            1.0 + envelope * 1.6
        } else { 1.0 };

        for b in self.cat_model.bricks.iter_mut() {
            let is_strike_limb = match b.parent {
                Some(p) => p == upper_r || p == forearm_r || p == paw_fr,
                None => false,
            };
            b.scale = if is_strike_limb { Vec3::splat(paw_pump) } else { Vec3::ONE };
        }
        // Re-apply world transforms after scale change (positions don't depend on scale,
        // but world_transform does — it's read in append_brick via world_transform()).
        // Our Brick::world_transform uses self.scale directly, so no re-compute needed.
    }

    // ── Manual fly camera ─────────────────────────────────
    fn update_fly(&mut self, dt: f32) {
        if self.mode != Mode::ManualFly { return; }
        let speed = if self.keys.shift { 700.0 } else { 220.0 } * dt;
        let f = Vec3::new(
            self.fly_yaw.sin() * self.fly_pitch.cos(),
            self.fly_pitch.sin(),
            self.fly_yaw.cos() * self.fly_pitch.cos(),
        );
        let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        let right = flat.cross(Vec3::Y).normalize_or_zero();
        if self.keys.w { self.fly_pos += flat * speed; }
        if self.keys.s { self.fly_pos -= flat * speed; }
        if self.keys.a { self.fly_pos -= right * speed; }
        if self.keys.d { self.fly_pos += right * speed; }
        if self.keys.q { self.fly_pos.y -= speed; }
        if self.keys.e { self.fly_pos.y += speed; }
    }

    // ── Camera eye/center from mode ───────────────────────
    fn compute_camera(&self) -> (Vec3, Vec3) {
        match self.mode {
            Mode::Cat => {
                // Orbit around cat: camera sits behind-and-above cat_yaw + cam_yaw offset.
                let orbit_yaw = self.cat_yaw + self.cam_yaw;
                let hor = self.cam_pitch.cos();
                let dir = Vec3::new(
                    -orbit_yaw.sin() * hor,
                     self.cam_pitch.sin(),
                    -orbit_yaw.cos() * hor,
                );
                let target = self.cat_pos + Vec3::Y * CAM_HEIGHT_OFFSET;
                let eye = target + dir * self.cam_dist;
                (eye, target)
            }
            Mode::ManualFly => {
                let f = Vec3::new(
                    self.fly_yaw.sin() * self.fly_pitch.cos(),
                    self.fly_pitch.sin(),
                    self.fly_yaw.cos() * self.fly_pitch.cos(),
                );
                (self.fly_pos, self.fly_pos + f * 100.0)
            }
        }
    }

    // ── Crosshair mesh (flat billboard at reach) ─────────
    fn build_crosshair_mesh(&self, eye: Vec3) -> ChunkMesh {
        let mut mesh = ChunkMesh::new();
        let cat_fwd = Vec3::new(self.cat_yaw.sin(), 0.0, self.cat_yaw.cos());
        let crosshair_center = self.cat_pos
            + cat_fwd * SWIPE_REACH_CM
            + Vec3::Y * (CAM_HEIGHT_OFFSET - 4.0); // approximate muzzle height

        // Billboard basis: the quad faces the camera.
        let view_dir = (crosshair_center - eye).normalize_or_zero();
        let right = view_dir.cross(Vec3::Y).normalize_or_zero();
        let up    = right.cross(view_dir).normalize_or_zero();

        // Pulse with swipe for feedback
        let size_mul = match self.swipe_t {
            Some(tn) => {
                let s = ((tn - 0.25) / 0.3).clamp(0.0, 1.0);
                1.0 + s * (1.0 - s) * 4.0 * 0.7  // peak ~1.7x mid-strike
            }
            None => 1.0,
        };
        let half = CROSSHAIR_SIZE_CM * 0.5 * size_mul;

        let color = if self.swipe_t.is_some() { [1.0, 0.35, 0.25, 1.0] } else { [1.0, 0.82, 0.15, 1.0] };
        let normal = [-view_dir.x, -view_dir.y, -view_dir.z];

        // Diamond corners (ромб)
        let c0 = crosshair_center + up * half;
        let c1 = crosshair_center + right * half;
        let c2 = crosshair_center - up * half;
        let c3 = crosshair_center - right * half;

        let push = |mesh: &mut ChunkMesh, p: Vec3| {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z],
                normal,
                color,
                material: 0,
            });
        };
        let base = mesh.vertices.len() as u32;
        push(&mut mesh, c0);
        push(&mut mesh, c1);
        push(&mut mesh, c2);
        push(&mut mesh, c3);
        mesh.indices.push(base);     mesh.indices.push(base + 1); mesh.indices.push(base + 2);
        mesh.indices.push(base);     mesh.indices.push(base + 2); mesh.indices.push(base + 3);
        mesh.triangle_count += 2;

        // Second side (backface) so it's visible from both directions — since we cull back-faces.
        let n2 = [view_dir.x, view_dir.y, view_dir.z];
        let base2 = mesh.vertices.len() as u32;
        for p in [c0, c3, c2, c1] {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z], normal: n2, color, material: 0,
            });
        }
        mesh.indices.push(base2);     mesh.indices.push(base2 + 1); mesh.indices.push(base2 + 2);
        mesh.indices.push(base2);     mesh.indices.push(base2 + 2); mesh.indices.push(base2 + 3);
        mesh.triangle_count += 2;

        mesh
    }

    // ── Init ──────────────────────────────────────────────
    fn init_gpu(&mut self, window: Arc<Window>) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(), ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface), force_fallback_adapter: false,
        })).expect("No GPU");
        println!("  GPU: {}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Prometheus"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            }, None,
        )).unwrap();

        let size = window.inner_size();
        let config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).unwrap();
        surface.configure(&device, &config);

        let (pipeline, bgl) = render_mesh::create_mesh_pipeline(&device, config.format);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniforms"),
            contents: bytemuck::bytes_of(&MeshUniforms::new(
                Mat4::IDENTITY, Mat4::IDENTITY, Vec3::ZERO,
                Vec3::new(0.4, -0.75, 0.3).normalize(),
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BG"), layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });
        let (_, depth_view) = render_mesh::create_depth_texture(&device, config.width, config.height);

        // Build apartment GPU mesh once — static.
        let apt_mesh = self.apartment.to_mesh();
        println!("  Apartment mesh: {} tris, {} verts",
            apt_mesh.triangle_count, apt_mesh.vertices.len());
        self.apartment_gpu = GpuMesh::from_chunk_mesh(&device, &apt_mesh);

        self.win = Some(window);
        self.pipeline = Some(pipeline);
        self.uniform_buffer = Some(uniform_buffer);
        self.bind_group = Some(bind_group);
        self.depth_view = Some(depth_view);
        self.device = Some(device);
        self.queue = Some(queue);
        self.surface = Some(surface);
        self.config = Some(config);
    }

    // ── Render ────────────────────────────────────────────
    fn render(&mut self) {
        // dt — real time.
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(1.0 / 20.0);
        self.last_frame = now;

        self.update_cat(dt);
        self.update_fly(dt);

        // Rebuild dynamic GPU meshes every frame.
        // Cat  — animated.  Apartment — flash tint + broken-brick pruning.
        // Crosshair — billboard needs camera basis.
        if let Some(device) = &self.device {
            let cat_mesh = self.cat_model.to_mesh();
            self.cat_gpu = GpuMesh::from_chunk_mesh(device, &cat_mesh);

            let apt_mesh = self.apartment.to_mesh();
            self.apartment_gpu = GpuMesh::from_chunk_mesh(device, &apt_mesh);

            let (eye_tmp, _) = self.compute_camera();
            let crosshair_mesh = self.build_crosshair_mesh(eye_tmp);
            self.crosshair_gpu = GpuMesh::from_chunk_mesh(device, &crosshair_mesh);
        }

        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let surface = self.surface.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        let (eye, center) = self.compute_camera();
        let aspect = config.width as f32 / config.height as f32;
        let view = Mat4::look_at_rh(eye, center, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov.to_radians(), aspect, 1.0, 3000.0);

        let uniforms = MeshUniforms::new(view, proj, eye,
            Vec3::new(0.4, -0.75, 0.3).normalize());
        queue.write_buffer(self.uniform_buffer.as_ref().unwrap(), 0, bytemuck::bytes_of(&uniforms));

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => { surface.configure(device, config); return; }
        };
        let view_tex = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.72, g: 0.82, b: 0.92, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.depth_view.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(self.pipeline.as_ref().unwrap());
            pass.set_bind_group(0, self.bind_group.as_ref().unwrap(), &[]);
            for mesh in [&self.apartment_gpu, &self.cat_gpu, &self.crosshair_gpu] {
                if let Some(m) = mesh {
                    pass.set_vertex_buffer(0, m.vertex_buffer.slice(..));
                    pass.set_index_buffer(m.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        self.win.as_ref().unwrap().request_redraw();
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// XZ-plane cylinder-vs-axis-aligned-box collision resolution.
/// Cat is a vertical cylinder at `pos` with the given radius, from
/// CAT_COLLIDER_Y_LO to CAT_COLLIDER_Y_HI.  Bricks whose world AABBs
/// live outside that vertical slab (floors, high lintels) are skipped.
fn resolve_cat_collision(pos_in: Vec3, apt: &BrickModel, radius: f32) -> Vec3 {
    let mut pos = pos_in;
    // Three passes handle corner pockets where two walls push you into a third.
    for _ in 0..3 {
        let mut any = false;
        for b in &apt.bricks {
            if !b.visible { continue; }
            let he = Vec3::new(
                b.half_extents.x * b.scale.x,
                b.half_extents.y * b.scale.y,
                b.half_extents.z * b.scale.z,
            );
            let min_y = b.world_position.y - he.y;
            let max_y = b.world_position.y + he.y;
            if max_y < CAT_COLLIDER_Y_LO || min_y > CAT_COLLIDER_Y_HI { continue; }

            let min_x = b.world_position.x - he.x;
            let max_x = b.world_position.x + he.x;
            let min_z = b.world_position.z - he.z;
            let max_z = b.world_position.z + he.z;

            let cx = pos.x.clamp(min_x, max_x);
            let cz = pos.z.clamp(min_z, max_z);
            let dx = pos.x - cx;
            let dz = pos.z - cz;
            let d2 = dx * dx + dz * dz;
            let r2 = radius * radius;

            if d2 < r2 - 1e-4 {
                any = true;
                if d2 > 1e-4 {
                    let d = d2.sqrt();
                    let push = radius - d;
                    pos.x += dx / d * push;
                    pos.z += dz / d * push;
                } else {
                    // Cat centre is inside the AABB — pick the shortest exit axis.
                    let pen_xp = (max_x + radius) - pos.x;
                    let pen_xn = pos.x - (min_x - radius);
                    let pen_zp = (max_z + radius) - pos.z;
                    let pen_zn = pos.z - (min_z - radius);
                    let m = pen_xp.min(pen_xn).min(pen_zp).min(pen_zn);
                    if m == pen_xp      { pos.x = max_x + radius; }
                    else if m == pen_xn { pos.x = min_x - radius; }
                    else if m == pen_zp { pos.z = max_z + radius; }
                    else                { pos.z = min_z - radius; }
                }
            }
        }
        if !any { break; }
    }
    pos
}

/// Walk the apartment's bricks and tag small props as breakable.
/// Large bricks (walls, floors, beds, sofas, the TV stand, etc.) remain
/// indestructible.  Material choice by name hints.
fn mark_apartment_breakables(apt: &mut BrickModel) {
    let mut by_mat = std::collections::HashMap::<&'static str, usize>::new();
    for b in apt.bricks.iter_mut() {
        let max_he = b.half_extents.max_element();
        if max_he >= 25.0 { continue; } // structural or heavy furniture
        let lname = b.name.to_lowercase();
        if lname.starts_with("wall") || lname.starts_with("floor")
           || lname.starts_with("ceiling") || lname.contains("divider")
           || lname.contains("corridor_wall") || lname.contains("door_frame") {
            continue;
        }
        let (dur, tag) = if lname.contains("book") || lname.contains("pillow")
                          || lname.contains("blanket") || lname.contains("linen")
                          || lname.contains("rug") || lname.contains("towel")
                          || lname.contains("curtain") || lname.contains("jacket") {
            (Durability::fabric(), "fabric")
        } else if lname.contains("bulb") || lname.contains("mirror")
                  || lname.contains("tv_screen") || lname.contains("window")
                  || lname.contains("vase") || lname.contains("glass") {
            (Durability::glass(), "glass")
        } else if lname.contains("plate") || lname.contains("fruit")
                  || lname.contains("bowl") || lname.contains("pot")
                  || lname.contains("porcelain") {
            (Durability::ceramic(), "ceramic")
        } else {
            // Generic small prop — wood default
            (Durability::wood(), "wood")
        };
        b.durability = Some(dur);
        *by_mat.entry(tag).or_insert(0) += 1;
    }
    let total: usize = by_mat.values().sum();
    println!("  Breakable props marked: {} total", total);
    for (m, c) in &by_mat {
        println!("      {}: {}", m, c);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if self.win.is_some() { return; }
        let build = env!("CARGO_PKG_VERSION");
        let build_tag = env!("BUILD_TAG");
        let w = Arc::new(el.create_window(
            Window::default_attributes()
                .with_title(format!("PROMETHEUS — Cat in the Apartment — v{build} build {build_tag}"))
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
        ).unwrap());
        self.init_gpu(w);
    }

    fn window_event(&mut self, el: &winit::event_loop::ActiveEventLoop, _: winit::window::WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(size) => {
                if let (Some(device), Some(surface), Some(config)) =
                    (self.device.as_ref(), self.surface.as_ref(), self.config.as_mut()) {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(device, config);
                    let (_, dv) = render_mesh::create_depth_texture(device, config.width, config.height);
                    self.depth_view = Some(dv);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state.is_pressed();
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) if pressed => el.exit(),
                    PhysicalKey::Code(KeyCode::Tab) if pressed => {
                        self.mode = match self.mode {
                            Mode::Cat => {
                                let (e, _) = self.compute_camera();
                                self.fly_pos = e;
                                Mode::ManualFly
                            }
                            Mode::ManualFly => Mode::Cat,
                        };
                        println!("  Mode: {}", match self.mode {
                            Mode::Cat => "CAT (WASD=move, Space=swipe, drag mouse=orbit)",
                            Mode::ManualFly => "FLY (WASD/QE, drag mouse, Shift=fast)",
                        });
                    }
                    PhysicalKey::Code(KeyCode::Space) if pressed => {
                        self.keys.space_edge = true;
                    }
                    PhysicalKey::Code(KeyCode::KeyW) => self.keys.w = pressed,
                    PhysicalKey::Code(KeyCode::KeyA) => self.keys.a = pressed,
                    PhysicalKey::Code(KeyCode::KeyS) => self.keys.s = pressed,
                    PhysicalKey::Code(KeyCode::KeyD) => self.keys.d = pressed,
                    PhysicalKey::Code(KeyCode::KeyQ) => self.keys.q = pressed,
                    PhysicalKey::Code(KeyCode::KeyE) => self.keys.e = pressed,
                    PhysicalKey::Code(KeyCode::ShiftLeft)
                    | PhysicalKey::Code(KeyCode::ShiftRight) => self.keys.shift = pressed,
                    PhysicalKey::Code(KeyCode::Equal) | PhysicalKey::Code(KeyCode::NumpadAdd)
                        if pressed => self.fov = (self.fov + 5.0).min(120.0),
                    PhysicalKey::Code(KeyCode::Minus) | PhysicalKey::Code(KeyCode::NumpadSubtract)
                        if pressed => self.fov = (self.fov - 5.0).max(20.0),
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    self.mouse_dragging = state.is_pressed();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let dx = (position.x - self.last_mouse.0) as f32;
                let dy = (position.y - self.last_mouse.1) as f32;
                if self.mouse_dragging {
                    match self.mode {
                        Mode::Cat => {
                            self.cam_yaw -= dx * 0.005;
                            self.cam_pitch = (self.cam_pitch - dy * 0.003).clamp(-0.35, 1.1);
                        }
                        Mode::ManualFly => {
                            self.fly_yaw -= dx * 0.004;
                            self.fly_pitch = (self.fly_pitch - dy * 0.003).clamp(-1.3, 1.3);
                        }
                    }
                }
                self.last_mouse = (position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.mode == Mode::Cat {
                    let scroll = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 30.0,
                    };
                    self.cam_dist = (self.cam_dist - scroll * 8.0).clamp(40.0, 300.0);
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    println!();
    println!("  ═══════════════════════════════════════════");
    println!("  🐱  PROMETHEUS — Cat in the Apartment");
    println!("      Chibi tabby in a 72 m² П-44.  1 vox = 1 cm.");
    println!("  ═══════════════════════════════════════════");
    println!();
    println!("  Controls (Mario-style, camera-relative):");
    println!("    W / A / S / D — cat moves relative to the camera");
    println!("    Shift         — run");
    println!("    Space         — paw SWIPE (cat snaps to camera direction, rears, strikes)");
    println!("    Mouse drag    — orbit camera around cat");
    println!("    Wheel         — zoom camera in / out");
    println!("    + / -         — FOV");
    println!("    Tab           — toggle fly-through debug camera");
    println!("    Esc           — quit");
    println!();
    println!("  Three corridor doors are boarded shut: bath, kitchen, bedroom-1.");
    println!("  Bust through — about five swipes each.  Other three are open.");
    println!();

    let el = EventLoop::new().unwrap();
    let mut app = App::new();
    el.run_app(&mut app).unwrap();
}
