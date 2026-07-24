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
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window},
};

use core::apartment::build_apartment;
use core::brick::{Brick, BrickModel};
use core::cat::build_chibi_cat;
use core::damage::{self, Damage, Durability};
use core::meshing::{ChunkMesh, MeshVertex};
use core::render_mesh::{self, GpuMesh, MeshUniforms};
use core::skeleton::Skeleton;
use core::voxobj::VoxObject;
use std::collections::HashMap;

// ─── Tuning constants ────────────────────────────────────────

const CAT_SCALE: f32 = 4.0;
const CAT_WALK_SPEED: f32 = 120.0; // cm/s (so 3 body-lengths per second, cat-appropriate)
const CAT_RUN_SPEED: f32 = 260.0;
const CAT_TURN_SPEED: f32 = 8.0; // rad/s — how fast cat yaw chases movement direction
const CAT_COLLIDER_RADIUS: f32 = 17.0; // XZ collision cylinder (cm) — covers head + body breadth so the muzzle doesn't poke into walls
const CAT_COLLIDER_Y_LO: f32 = 0.0; // body lower bound for world-brick cross-check
const CAT_COLLIDER_Y_HI: f32 = 30.0; // body upper bound (covers full chibi cat ~30 cm tall)

const SWIPE_DURATION: f32 = 0.55;
const SWIPE_REACH_CM: f32 = 12.0 * CAT_SCALE; // ≈ 48 cm in front of the muzzle — enough to reach furniture from collider distance
const CROSSHAIR_SIZE_CM: f32 = 9.0;
const CAT_BASE_POWER: f32 = 1.5; // raw paw power; lethal in 2 hits to wood, 1 hit to fabric/glass
const CAT_FEET_Y: f32 = 0.0; // floor level
const CAT_PELVIS_Y: f32 = 2.3 * CAT_SCALE; // thigh+shin+paw height so feet rest on floor

// Apartment living-room centre (see apartment.rs layout).
// Living room spans x=520..900, z=450..800; pelvis sits at the centre on the floor.
const SPAWN_POS: Vec3 = Vec3::new(710.0, CAT_PELVIS_Y, 625.0);

// Camera orbit (3rd-person)
const CAM_DIST_DEFAULT: f32 = 110.0;
const CAM_PITCH_DEFAULT: f32 = 0.35; // slight downward tilt
const CAM_HEIGHT_OFFSET: f32 = 25.0; // look slightly above cat's pelvis

// ─── Cinematic waypoints (kept for Tab=fly mode scene setup) ──
#[derive(Clone, Copy)]
struct Waypoint {
    pos: Vec3,
    look: Vec3,
}

// ─── Modes ────────────────────────────────────────────────────
#[derive(PartialEq, Eq, Clone, Copy)]
enum Mode {
    Cat,       // default gameplay
    ManualFly, // fly-through for debug
}

// ─── Input ────────────────────────────────────────────────────
#[derive(Default)]
struct KeysHeld {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    q: bool,
    e: bool,
    shift: bool,
    space_edge: bool,
}

const CAT_MOUSE_SENS: f32 = 0.0045; // rad per pixel — yaw responsiveness
const CAT_GRAVITY: f32 = 800.0; // cm/s² — Earth-ish, feels right for chibi cat
const CAT_JUMP_SPEED: f32 = 400.0; // cm/s initial vertical velocity (peak ~100 cm — sofa, table, bed)

// ─── Gibs (debris particles) ──────────────────────────────────
const GIB_GRAVITY: f32 = 900.0; // cm/s²
const GIB_BOUNCE: f32 = 0.35; // Y velocity multiplier on floor hit
const GIB_FRICTION: f32 = 0.6; // XZ velocity multiplier on floor hit
const GIB_LIFE: f32 = 3.0; // seconds before despawn

#[derive(Clone)]
struct Gib {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    ang_vel: Vec3, // axis * angular speed (rad/s)
    half_extents: Vec3,
    color: [u8; 3],
    life: f32,
}

// ─── Tiny bitmap font ────────────────────────────────────────
// 5×7 ASCII subset for HUD/overlay text.  Each row is a 5-bit pattern
// where bit 4 (0b10000) is the leftmost pixel.  Only the chars we
// actually print live here; others render as blanks.
const FONT_W: usize = 5;
const FONT_H: usize = 7;

fn glyph(c: char) -> [u8; FONT_H] {
    match c {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10101, 0b10101, 0b10011, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        ' ' => [0; 7],
        _ => [0; 7],
    }
}

/// Draw a string `s` into `mesh` using a bitmap font, billboarded around
/// `centre` with basis vectors `right` and `up`.  `cell` is the pixel size
/// in world units; characters are spaced FONT_W+1 cells apart.
fn draw_text(
    mesh: &mut ChunkMesh,
    s: &str,
    centre: Vec3,
    right: Vec3,
    up: Vec3,
    cell: f32,
    color: [f32; 4],
    normal_front: [f32; 3],
    normal_back: [f32; 3],
) {
    let total_chars = s.chars().count() as f32;
    let glyph_w = (FONT_W as f32 + 1.0) * cell;
    // Centre the string horizontally on `centre`.
    let start_x = -(total_chars * glyph_w - cell) * 0.5;
    let half = cell * 0.5;
    let mut push_quad = |c: [Vec3; 4]| {
        let base = mesh.vertices.len() as u32;
        for p in c {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z],
                normal: normal_front,
                color,
                material: 0,
            });
        }
        mesh.indices.push(base);
        mesh.indices.push(base + 1);
        mesh.indices.push(base + 2);
        mesh.indices.push(base);
        mesh.indices.push(base + 2);
        mesh.indices.push(base + 3);
        let base2 = mesh.vertices.len() as u32;
        for p in [c[0], c[3], c[2], c[1]] {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z],
                normal: normal_back,
                color,
                material: 0,
            });
        }
        mesh.indices.push(base2);
        mesh.indices.push(base2 + 1);
        mesh.indices.push(base2 + 2);
        mesh.indices.push(base2);
        mesh.indices.push(base2 + 2);
        mesh.indices.push(base2 + 3);
        mesh.triangle_count += 4;
    };
    for (ci, ch) in s.chars().enumerate() {
        let g = glyph(ch.to_ascii_uppercase());
        let char_x = start_x + ci as f32 * glyph_w;
        for (row, bits) in g.iter().enumerate() {
            // Top row should sit highest; row 0 maps to +up direction.
            let py = (FONT_H as f32 / 2.0 - row as f32 - 0.5) * cell;
            for col in 0..FONT_W {
                if bits & (1 << (FONT_W - 1 - col)) == 0 {
                    continue;
                }
                let px = char_x + col as f32 * cell;
                let cc = centre + right * px + up * py;
                let c0 = cc + up * half - right * half;
                let c1 = cc + up * half + right * half;
                let c2 = cc - up * half + right * half;
                let c3 = cc - up * half - right * half;
                push_quad([c0, c1, c2, c3]);
            }
        }
    }
}

/// Pseudo-random helper (xorshift32, seeded from a struct address).
/// Avoids pulling rand crate for one feature.
fn rand_unit(seed: &mut u32) -> f32 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0 // [-1, +1]
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
    beauty_mode: bool,
    beauty_capture_countdown: u32,
    beauty_capture_complete: bool,
    apartment: BrickModel,
    apartment_gpu: Option<GpuMesh>,

    cat_sk: Skeleton,
    cat_model: BrickModel,
    cat_gpu: Option<GpuMesh>,
    cat_pos: Vec3,
    cat_yaw: f32,
    cat_moving: bool,
    cat_vy: f32,           // vertical velocity (cm/s)
    cat_grounded: bool,    // touching the floor (or top of furniture)
    debug_collision: bool, // F1 toggles per-frame collision diagnostics
    paused: bool,          // Esc menu open / world frozen
    cursor_locked: bool,   // tracks current cursor grab state
    screenshot_pending: bool,
    screenshot_index: u32,
    screenshot_flash_t: f32, // visible flash badge after a save

    // Animation
    anim_time: f32,
    swipe_t: Option<f32>,
    swipe_already_hit: bool,

    // Crosshair
    crosshair_gpu: Option<GpuMesh>,
    pause_overlay_gpu: Option<GpuMesh>,
    shot_flash_gpu: Option<GpuMesh>,

    // Gibs / debris
    gibs: Vec<Gib>,
    gibs_model: BrickModel,
    gibs_gpu: Option<GpuMesh>,
    rng_seed: u32,

    // Voxel objects (bricks promoted to voxel chunks after first hit)
    vox_objects: Vec<VoxObject>,
    brick_to_vox: HashMap<usize, usize>,
    vox_gpu: Option<GpuMesh>,

    // Cat HP — backlash damage from hitting tough things
    cat_hp: f32,
    cat_max_hp: f32,

    // Camera
    mode: Mode,
    cam_yaw: f32, // orbit yaw around cat
    cam_pitch: f32,
    cam_dist: f32,
    fly_pos: Vec3,
    fly_yaw: f32,
    fly_pitch: f32,

    // Input
    keys: KeysHeld,
    mouse_dragging: bool, // legacy — used in fly mode
    rmb_held: bool,       // right mouse — free orbit override
    last_mouse: (f64, f64),

    // Timing
    last_frame: Instant,
    fov: f32,
}

impl App {
    fn new(beauty_mode: bool) -> Self {
        // Apartment (static)
        let mut apartment = if beauty_mode {
            build_beauty_stage()
        } else {
            build_apartment()
        };
        apartment.update_static();
        if !beauty_mode {
            mark_apartment_breakables(&mut apartment);
        }
        println!("  Scene assembled: {} bricks", apartment.brick_count());

        // Cat rig
        let mut cat_sk = Skeleton::chibi_cat(CAT_SCALE);
        let cat_pos = if beauty_mode {
            Vec3::new(0.0, CAT_PELVIS_Y + 3.5, 0.0)
        } else {
            SPAWN_POS
        };
        cat_sk.root_position = cat_pos;
        cat_sk.root_rotation = Quat::IDENTITY;
        cat_sk.solve_forward();
        let mut cat_model = build_chibi_cat(&cat_sk, CAT_SCALE);
        cat_model.update(&cat_sk);
        println!("  ChibiCat assembled: {} bricks", cat_model.brick_count());

        Self {
            win: None,
            device: None,
            queue: None,
            surface: None,
            config: None,
            pipeline: None,
            uniform_buffer: None,
            bind_group: None,
            depth_view: None,

            beauty_mode,
            beauty_capture_countdown: if beauty_mode { 2 } else { 0 },
            beauty_capture_complete: false,
            apartment,
            apartment_gpu: None,
            cat_sk,
            cat_model,
            cat_gpu: None,
            cat_pos,
            cat_yaw: 0.0,
            cat_moving: false,
            cat_vy: 0.0,
            cat_grounded: true,
            debug_collision: false,
            paused: false,
            cursor_locked: false,
            screenshot_pending: false,
            screenshot_index: 0,
            screenshot_flash_t: 0.0,

            anim_time: 0.0,
            swipe_t: None,
            swipe_already_hit: false,

            crosshair_gpu: None,
            pause_overlay_gpu: None,
            shot_flash_gpu: None,

            gibs: Vec::new(),
            gibs_model: BrickModel::new("gibs"),
            gibs_gpu: None,
            rng_seed: 0xC0FFEE,

            vox_objects: Vec::new(),
            brick_to_vox: HashMap::new(),
            vox_gpu: None,

            cat_hp: 10.0,
            cat_max_hp: 10.0,

            mode: Mode::Cat,
            cam_yaw: 0.0,
            cam_pitch: CAM_PITCH_DEFAULT,
            cam_dist: CAM_DIST_DEFAULT,
            fly_pos: Vec3::new(450.0, 300.0, -50.0),
            fly_yaw: 0.0,
            fly_pitch: -0.3,

            keys: KeysHeld::default(),
            mouse_dragging: false,
            rmb_held: false,
            last_mouse: (0.0, 0.0),

            last_frame: Instant::now(),
            fov: if beauty_mode { 42.5 } else { 55.0 },
        }
    }

    // ── Cat update (FPS-style: mouse turns the cat, WASD is body-relative) ────
    fn update_cat(&mut self, dt: f32) {
        if self.beauty_mode {
            self.update_beauty_cat(dt);
            return;
        }
        if self.mode != Mode::Cat {
            return;
        }

        // Move relative to the cat's own facing.  W/S = forward/back,
        // A/D = strafe left/right.  Mouse turns the cat; A/D never rotate.
        let cat_fwd = Vec3::new(self.cat_yaw.sin(), 0.0, self.cat_yaw.cos());
        let cat_right = Vec3::new(self.cat_yaw.cos(), 0.0, -self.cat_yaw.sin());

        let mut mv = Vec3::ZERO;
        if self.keys.w {
            mv += cat_fwd;
        }
        if self.keys.s {
            mv -= cat_fwd;
        }
        if self.keys.d {
            mv -= cat_right;
        } // strafe right
        if self.keys.a {
            mv += cat_right;
        } // strafe left
        let moving = mv.length_squared() > 0.01;
        if moving {
            mv = mv.normalize();
        }
        self.cat_moving = moving;

        let speed = if self.keys.shift {
            CAT_RUN_SPEED
        } else {
            CAT_WALK_SPEED
        };
        let desired = self.cat_pos + mv * speed * dt;

        // Outer-wall clamp, then brick-vs-cat collision push-out
        let mut next = desired;
        next.x = next.x.clamp(25.0, 875.0);
        next.z = next.z.clamp(25.0, 775.0);
        // Vertical physics: gravity + jump.  Floor at CAT_PELVIS_Y, but
        // bricks (and vox chunks) within XZ overlap can become a higher floor —
        // cat lands on top of a sofa / bed / table.
        self.cat_vy -= CAT_GRAVITY * dt;
        let prev_y = self.cat_pos.y;
        let mut new_y = prev_y + self.cat_vy * dt;
        // Find the highest support surface under the cat at (next.x, next.z).
        let mut floor_y = CAT_PELVIS_Y;
        for b in &self.apartment.bricks {
            if !b.visible {
                continue;
            }
            let he = Vec3::new(
                b.half_extents.x * b.scale.x,
                b.half_extents.y * b.scale.y,
                b.half_extents.z * b.scale.z,
            );
            let mn_x = b.world_position.x - he.x;
            let mx_x = b.world_position.x + he.x;
            let mn_z = b.world_position.z - he.z;
            let mx_z = b.world_position.z + he.z;
            // Tight XZ overlap test — only the slimmest inflate so cat doesn't
            // hover-stick to objects beside it.
            if next.x < mn_x - CAT_COLLIDER_RADIUS * 0.5
                || next.x > mx_x + CAT_COLLIDER_RADIUS * 0.5
                || next.z < mn_z - CAT_COLLIDER_RADIUS * 0.5
                || next.z > mx_z + CAT_COLLIDER_RADIUS * 0.5
            {
                continue;
            }
            let top = b.world_position.y + he.y;
            // Skip rugs/mats — anything thinner than a paw shouldn't be a perch.
            if top < 8.0 {
                continue;
            }
            if top > floor_y && top <= prev_y + 5.0 {
                floor_y = top;
            }
        }
        for v in &self.vox_objects {
            if v.is_empty() {
                continue;
            }
            let (mn, mx) = v.world_aabb();
            if next.x < mn.x - CAT_COLLIDER_RADIUS * 0.5
                || next.x > mx.x + CAT_COLLIDER_RADIUS * 0.5
                || next.z < mn.z - CAT_COLLIDER_RADIUS * 0.5
                || next.z > mx.z + CAT_COLLIDER_RADIUS * 0.5
            {
                continue;
            }
            let top = mx.y;
            if top < 8.0 {
                continue;
            }
            if top > floor_y && top <= prev_y + 5.0 {
                floor_y = top;
            }
        }
        if new_y <= floor_y {
            new_y = floor_y;
            self.cat_vy = 0.0;
            self.cat_grounded = true;
        } else {
            self.cat_grounded = false;
        }
        next.y = new_y;
        next = resolve_cat_collision(
            next,
            &self.apartment,
            &self.vox_objects,
            CAT_COLLIDER_RADIUS,
            self.debug_collision,
        );
        self.cat_pos = next;

        // Advance animation clock
        self.anim_time += dt
            * if self.keys.shift && self.cat_moving {
                2.0
            } else {
                1.0
            };

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

    fn update_beauty_cat(&mut self, dt: f32) {
        self.anim_time += dt;
        let t = self.anim_time;

        for name in [
            "spine1",
            "spine2",
            "neck",
            "head",
            "upper_arm_l",
            "forearm_l",
            "upper_arm_r",
            "forearm_r",
            "thigh_l",
            "shin_l",
            "thigh_r",
            "shin_r",
            "tail1",
            "tail2",
            "tail3",
            "tail4",
        ] {
            self.cat_sk.bone_mut(name).local_rotation = Quat::IDENTITY;
        }

        let breath = (t * 1.8).sin();
        self.cat_sk.bone_mut("spine1").local_rotation = Quat::from_rotation_x(breath * 0.025);
        self.cat_sk.bone_mut("neck").local_rotation = Quat::from_rotation_x(-0.10 + breath * 0.018);
        self.cat_sk.bone_mut("head").local_rotation =
            Quat::from_rotation_y((t * 0.55).sin() * 0.045);

        let tail = (t * 1.35).sin();
        self.cat_sk.bone_mut("tail1").local_rotation = Quat::from_rotation_y(tail * 0.18);
        self.cat_sk.bone_mut("tail2").local_rotation = Quat::from_rotation_y(tail * 0.28);
        self.cat_sk.bone_mut("tail3").local_rotation = Quat::from_rotation_y(tail * 0.34);
        self.cat_sk.bone_mut("tail4").local_rotation = Quat::from_rotation_y(tail * 0.40);

        self.cat_sk.root_position = self.cat_pos;
        self.cat_sk.root_rotation = Quat::from_rotation_y(self.cat_yaw);
        self.cat_sk.solve_forward();
        self.cat_model.update(&self.cat_sk);
    }

    fn pose_cat_skeleton(&mut self) {
        // Reset key bones each frame; we author rotations from scratch.
        let reset = [
            "spine1",
            "spine2",
            "neck",
            "head",
            "upper_arm_l",
            "forearm_l",
            "upper_arm_r",
            "forearm_r",
            "thigh_l",
            "shin_l",
            "thigh_r",
            "shin_r",
            "tail1",
            "tail2",
            "tail3",
            "tail4",
        ];
        for name in reset {
            self.cat_sk.bone_mut(name).local_rotation = Quat::IDENTITY;
        }

        let t = self.anim_time;
        let moving = self.cat_moving;

        // Idle: gentle breathing + tail sway
        let breath = (t * 2.0).sin() * 0.03;
        self.cat_sk.bone_mut("spine1").local_rotation = Quat::from_rotation_x(breath);

        // Head follows the camera pitch — the cat watches the crosshair.
        // Negative X-rotation tips muzzle up; cam_pitch>0 means camera looks
        // down on the cat → head should also tilt down (positive X-rotation
        // in skeleton convention).  Clamp to a believable range.
        let head_pitch = self.cam_pitch.clamp(-0.5, 0.9);
        self.cat_sk.bone_mut("neck").local_rotation = Quat::from_rotation_x(head_pitch * 0.35);
        self.cat_sk.bone_mut("head").local_rotation = Quat::from_rotation_x(head_pitch * 0.65);
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
            self.cat_sk.bone_mut("forearm_l").local_rotation =
                Quat::from_rotation_x((swing.abs() - 0.1).max(0.0) * 0.9);
            self.cat_sk.bone_mut("upper_arm_r").local_rotation = Quat::from_rotation_x(swing_b);
            self.cat_sk.bone_mut("forearm_r").local_rotation =
                Quat::from_rotation_x((swing_b.abs() - 0.1).max(0.0) * 0.9);

            self.cat_sk.bone_mut("thigh_l").local_rotation = Quat::from_rotation_x(-swing_b);
            self.cat_sk.bone_mut("shin_l").local_rotation =
                Quat::from_rotation_x((swing_b.abs() - 0.1).max(0.0) * 0.9);
            self.cat_sk.bone_mut("thigh_r").local_rotation = Quat::from_rotation_x(-swing);
            self.cat_sk.bone_mut("shin_r").local_rotation =
                Quat::from_rotation_x((swing.abs() - 0.1).max(0.0) * 0.9);
        }

        // Swipe: REARING — pelvis tilts back, pivots on hind legs, right front strikes.
        if let Some(tn) = self.swipe_t {
            // Envelope: rise 0..0.25, strike 0.25..0.5, hold 0.5..0.7, recover 0.7..1
            let rise = smoothstep((tn / 0.25).clamp(0.0, 1.0))
                * (1.0 - smoothstep(((tn - 0.7) / 0.3).clamp(0.0, 1.0)));
            let strike = {
                let s = ((tn - 0.25) / 0.25).clamp(0.0, 1.0);
                smoothstep(s) * (1.0 - smoothstep(((tn - 0.6) / 0.3).clamp(0.0, 1.0)))
            };

            // Spine rears back (pitch).  Negative X = tail down, chest up.
            let rear_amt = rise * 1.15; // ~66°
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
            self.cat_sk.bone_mut("forearm_l").local_rotation = Quat::from_rotation_x(1.3 * rise);

            // Right front: wind-up (up), then STRIKE (forward).
            let wind = rise * 0.9; // shoulder draws back / up during rise
            let pop = strike * 1.7; // shoulder slams forward during strike
            self.cat_sk.bone_mut("upper_arm_r").local_rotation = Quat::from_rotation_x(-wind + pop);
            self.cat_sk.bone_mut("forearm_r").local_rotation =
                Quat::from_rotation_x(1.3 * wind - 0.9 * strike);

            // Tail whips up for balance.
            self.cat_sk.bone_mut("tail1").local_rotation =
                self.cat_sk.bone_mut("tail1").local_rotation * Quat::from_rotation_x(-0.9 * rise);
            self.cat_sk.bone_mut("tail2").local_rotation = Quat::from_rotation_x(-0.7 * rise);
        }
    }

    fn try_swipe_hit(&mut self) {
        if self.swipe_already_hit {
            return;
        }
        let tn = match self.swipe_t {
            Some(t) => t,
            None => return,
        };
        // Strike window — when the paw is actually out
        if !(0.28..=0.55).contains(&tn) {
            return;
        }

        // 3D aim — yaw + pitch from camera.
        let aim_yaw = self.cat_yaw + self.cam_yaw;
        let pitch = self.cam_pitch;
        let dir = Vec3::new(
            aim_yaw.sin() * pitch.cos(),
            -pitch.sin(),
            aim_yaw.cos() * pitch.cos(),
        )
        .normalize();
        // Origin: cat's nose centre.  Crosshair sits at origin + dir * REACH,
        // so the impact point is exactly where the diamond floats.
        let origin = self.cat_pos + Vec3::Y * (CAM_HEIGHT_OFFSET - 4.0);
        let max_dist = SWIPE_REACH_CM * 1.4;

        let smack = Damage::new(CAT_BASE_POWER, 3.0, core::damage::DamageKind::Blunt);

        // Raycast against existing voxel objects first (for repeated carving
        // of the same brick).  Pick whichever is closer — vox or brick.
        let vox_hit = {
            let mut best: Option<(usize, f32)> = None;
            for (vi, v) in self.vox_objects.iter().enumerate() {
                if v.is_empty() {
                    continue;
                }
                let (mn, mx) = v.world_aabb();
                if let Some(t) = ray_aabb_box(origin, dir, mn, mx, max_dist) {
                    if best.map(|(_, bt)| t < bt).unwrap_or(true) {
                        best = Some((vi, t));
                    }
                }
            }
            best
        };
        let brick_hit = self.apartment.raycast_breakable(origin, dir, max_dist);
        let pick = match (brick_hit, vox_hit) {
            (None, None) => None,
            (Some(bh), None) => Some(("brick", bh.0, bh.1)),
            (None, Some(vh)) => Some(("vox", vh.0, vh.1)),
            (Some(bh), Some(vh)) => {
                if bh.1 <= vh.1 {
                    Some(("brick", bh.0, bh.1))
                } else {
                    Some(("vox", vh.0, vh.1))
                }
            }
        };

        if pick.is_none() {
            println!("  -- swipe missed (no breakable in {:.0}cm)", max_dist);
            self.swipe_already_hit = true;
            return;
        }
        let (kind, idx_or_vox, dist) = pick.unwrap();

        if kind == "vox" {
            // Carve existing voxel chunk.
            let impact = origin + dir * dist;
            let mut removed: Vec<Vec3> = Vec::new();
            let (color, gib_he, total_destroyed, annihilate_now, sev, applied, backlash) = {
                let vox = &mut self.vox_objects[idx_or_vox];
                let durability = vox.durability;
                let h = damage::compute_hit(&durability, &smack);
                if !h.applied {
                    let bl = (durability.toughness - smack.power).max(0.0) * 0.4 + 0.2;
                    (
                        vox.color,
                        Vec3::splat(vox.vox_size * 0.5),
                        false,
                        false,
                        h.severity,
                        false,
                        bl,
                    )
                } else {
                    let frac_left = vox.solid_count as f32 / vox.initial_count.max(1) as f32;
                    let hp_left = durability.hp * frac_left;
                    let annihilate = CAT_BASE_POWER >= hp_left * 3.0
                        || h.severity == core::damage::Severity::Shatter;
                    if annihilate {
                        vox.annihilate(&mut removed);
                    } else {
                        let world_r =
                            (h.effective_radius * vox.vox_size * 0.5).max(vox.vox_size * 1.2);
                        vox.carve_sphere(impact, world_r, &mut removed);
                    }
                    (
                        vox.color,
                        Vec3::splat(vox.vox_size * 0.5),
                        vox.is_empty(),
                        annihilate,
                        h.severity,
                        true,
                        0.0,
                    )
                }
            };
            if !applied {
                self.cat_hp = (self.cat_hp - backlash).max(0.0);
                println!(
                    "  🐾 OW! vox too tough — backlash {:.2}, cat hp {:.1}",
                    backlash, self.cat_hp
                );
                self.swipe_already_hit = true;
                return;
            }
            for p in &removed {
                self.spawn_gib_at(*p, dir, color, gib_he);
            }
            println!(
                "  💥 carve vox @ {:.0}cm → sev={:?} dust={} {}{}",
                dist,
                sev,
                removed.len(),
                if total_destroyed { "EMPTY " } else { "" },
                if annihilate_now { "(annihilation)" } else { "" }
            );
            self.swipe_already_hit = true;
            return;
        }

        // kind == "brick" — promote-then-carve path
        let idx = idx_or_vox;
        if let Some((idx, dist)) = Some((idx, dist)) {
            let impact = origin + dir * dist;
            let brick_color = self.apartment.bricks[idx].color;
            let brick_he = self.apartment.bricks[idx].half_extents;
            let brick_pos = self.apartment.bricks[idx].world_position;
            let brick_rot = self.apartment.bricks[idx].world_rotation;
            let brick_mat = self.apartment.bricks[idx].material;
            let brick_dur = self.apartment.bricks[idx]
                .durability
                .unwrap_or(Durability::wood());
            let name = self.apartment.bricks[idx].name.clone();

            // Pure compute first — we need to react to the result before
            // committing damage to anything.
            let h = damage::compute_hit(&brick_dur, &smack);

            // Backlash: cat power was below toughness, the strike rebounded.
            if !h.applied {
                let backlash = (brick_dur.toughness - smack.power).max(0.0) * 0.4 + 0.2;
                self.cat_hp = (self.cat_hp - backlash).max(0.0);
                println!(
                    "  🐾 OW! {} too tough — paw backlash {:.2}, cat hp {:.1}/{:.1}",
                    name, backlash, self.cat_hp, self.cat_max_hp
                );
                self.swipe_already_hit = true;
                return;
            }

            // Promote the brick to a voxel chunk on first hit (or fetch the
            // existing one).  Subsequent hits carve a sphere out of the SVO.
            let vox_idx = if let Some(&v) = self.brick_to_vox.get(&idx) {
                v
            } else {
                let vox = VoxObject::from_brick(
                    brick_pos,
                    brick_rot,
                    brick_he,
                    brick_color,
                    brick_mat,
                    brick_dur,
                    32,
                );
                let i = self.vox_objects.len();
                self.vox_objects.push(vox);
                self.brick_to_vox.insert(idx, i);
                // Hide the polygonal source — voxel chunk takes over rendering.
                self.apartment.bricks[idx].visible = false;
                self.apartment.bricks[idx].durability = None; // no double raycast hits
                i
            };

            // Carve.  Annihilate if effective_power is overwhelmingly larger
            // than what's left of the object's hp.
            let mut removed_positions: Vec<Vec3> = Vec::new();
            let (gib_he, totally_destroyed, annihilation) = {
                let vox = &mut self.vox_objects[vox_idx];
                let frac_left = vox.solid_count as f32 / vox.initial_count.max(1) as f32;
                let hp_left = brick_dur.hp * frac_left;
                let annihilation = CAT_BASE_POWER >= hp_left * 3.0
                    || h.severity == core::damage::Severity::Shatter;
                if annihilation {
                    vox.annihilate(&mut removed_positions);
                } else {
                    let world_radius =
                        (h.effective_radius * vox.vox_size * 0.5).max(vox.vox_size * 1.2);
                    vox.carve_sphere(impact, world_radius, &mut removed_positions);
                }
                (
                    Vec3::splat(vox.vox_size * 0.5),
                    vox.is_empty(),
                    annihilation,
                )
            };

            for p in &removed_positions {
                self.spawn_gib_at(*p, dir, brick_color, gib_he);
            }

            println!(
                "  💥 SWIPE! {} @ {:.0}cm → sev={:?} dust={} {}{}",
                name,
                dist,
                h.severity,
                removed_positions.len(),
                if totally_destroyed { "DESTROYED " } else { "" },
                if annihilation { "(annihilation)" } else { "" }
            );
        }
        self.swipe_already_hit = true;
    }

    /// Lock the cursor inside the window while playing; free it in menu.
    fn apply_cursor_grab(&mut self) {
        let want_lock = !self.paused && self.mode == Mode::Cat;
        if want_lock == self.cursor_locked {
            return;
        }
        if let Some(w) = &self.win {
            if want_lock {
                let r = w
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined));
                if r.is_ok() {
                    w.set_cursor_visible(false);
                    self.cursor_locked = true;
                }
            } else {
                let _ = w.set_cursor_grab(CursorGrabMode::None);
                w.set_cursor_visible(true);
                self.cursor_locked = false;
            }
        }
    }

    /// Build the pause-menu billboard.  Draws a dark plate close to the
    /// camera (4cm) so it always covers the scene regardless of cat's position,
    /// then prints "PAUSED" and "ESC TO RESUME" with the bitmap font.
    fn build_pause_overlay(&self, eye: Vec3, center: Vec3) -> ChunkMesh {
        let mut mesh = ChunkMesh::new();
        let fwd = (center - eye).normalize_or_zero();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(fwd).normalize_or_zero();
        // Sit the panel a bit further from the camera so the bitmap font has
        // room to breathe.  At 12cm with FOV 55, viewport is ~12.5cm tall.
        let billboard_centre = eye + fwd * 12.0;
        let half_w = 10.5;
        let half_h = 6.0;
        let n_front = [-fwd.x, -fwd.y, -fwd.z];
        let n_back = [fwd.x, fwd.y, fwd.z];
        let outer_color = [0.04, 0.04, 0.08, 1.0];
        let title_color = [1.0, 0.82, 0.15, 1.0];
        let hint_color = [0.85, 0.85, 0.90, 1.0];

        // Outer dark plate (double-sided).
        let c = [
            billboard_centre + up * half_h - right * half_w,
            billboard_centre + up * half_h + right * half_w,
            billboard_centre - up * half_h + right * half_w,
            billboard_centre - up * half_h - right * half_w,
        ];
        let base = mesh.vertices.len() as u32;
        for p in c {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z],
                normal: n_front,
                color: outer_color,
                material: 0,
            });
        }
        mesh.indices.push(base);
        mesh.indices.push(base + 1);
        mesh.indices.push(base + 2);
        mesh.indices.push(base);
        mesh.indices.push(base + 2);
        mesh.indices.push(base + 3);
        let base2 = mesh.vertices.len() as u32;
        for p in [c[0], c[3], c[2], c[1]] {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z],
                normal: n_back,
                color: outer_color,
                material: 0,
            });
        }
        mesh.indices.push(base2);
        mesh.indices.push(base2 + 1);
        mesh.indices.push(base2 + 2);
        mesh.indices.push(base2);
        mesh.indices.push(base2 + 2);
        mesh.indices.push(base2 + 3);
        mesh.triangle_count += 4;

        // Title "PAUSED" centred slightly above middle.
        let title_centre = billboard_centre + up * 1.8;
        draw_text(
            &mut mesh,
            "PAUSED",
            title_centre,
            right,
            up,
            0.40,
            title_color,
            n_front,
            n_back,
        );

        // Hint "ESC TO RESUME" smaller, below.
        let hint_centre = billboard_centre - up * 1.8;
        draw_text(
            &mut mesh,
            "ESC TO RESUME",
            hint_centre,
            right,
            up,
            0.20,
            hint_color,
            n_front,
            n_back,
        );

        mesh
    }

    /// Tiny golden "SHOT" badge in the top-right corner for ~0.6s after a
    /// screenshot is saved.  Built only when `screenshot_flash_t > 0`.
    fn build_shot_flash(&self, eye: Vec3, center: Vec3) -> ChunkMesh {
        let mut mesh = ChunkMesh::new();
        let fwd = (center - eye).normalize_or_zero();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(fwd).normalize_or_zero();
        // Anchor in the top-right of the near plane.
        let badge_centre = eye + fwd * 12.0 + right * 7.5 + up * 4.5;
        let n_front = [-fwd.x, -fwd.y, -fwd.z];
        let n_back = [fwd.x, fwd.y, fwd.z];
        let alpha = self.screenshot_flash_t.clamp(0.0, 1.0);
        let title_color = [1.0, 0.82, 0.15, alpha];
        draw_text(
            &mut mesh,
            "SHOT",
            badge_centre,
            right,
            up,
            0.18,
            title_color,
            n_front,
            n_back,
        );
        mesh
    }

    /// Spawn a single gib at exact position with given size and outward bias.
    fn spawn_gib_at(&mut self, pos: Vec3, dir_bias: Vec3, color: [u8; 3], half: Vec3) {
        let bias = dir_bias.normalize_or_zero() + Vec3::Y * 0.4;
        let rx = rand_unit(&mut self.rng_seed);
        let ry = rand_unit(&mut self.rng_seed);
        let rz = rand_unit(&mut self.rng_seed);
        let v = (bias + Vec3::new(rx, ry, rz) * 0.7).normalize_or_zero()
            * (40.0 + 60.0 * rand_unit(&mut self.rng_seed).abs());
        let ax = rand_unit(&mut self.rng_seed);
        let ay = rand_unit(&mut self.rng_seed);
        let az = rand_unit(&mut self.rng_seed);
        self.gibs.push(Gib {
            pos,
            rot: Quat::IDENTITY,
            vel: v,
            ang_vel: Vec3::new(ax, ay, az) * 6.0,
            half_extents: half,
            color,
            life: GIB_LIFE,
        });
    }

    /// Spawn `n` gibs around `pos`, mostly flying along `dir_bias` plus randomness.
    fn spawn_gibs(
        &mut self,
        pos: Vec3,
        dir_bias: Vec3,
        color: [u8; 3],
        half: Vec3,
        n: usize,
        base_speed: f32,
    ) {
        let bias = dir_bias.normalize_or_zero() + Vec3::Y * 0.6;
        for _ in 0..n {
            let rx = rand_unit(&mut self.rng_seed);
            let ry = rand_unit(&mut self.rng_seed);
            let rz = rand_unit(&mut self.rng_seed);
            let jitter = Vec3::new(rx, ry, rz);
            let v = (bias + jitter * 0.9).normalize_or_zero()
                * (base_speed * (0.7 + 0.5 * rand_unit(&mut self.rng_seed).abs()));
            let ax = rand_unit(&mut self.rng_seed);
            let ay = rand_unit(&mut self.rng_seed);
            let az = rand_unit(&mut self.rng_seed);
            // Mild size jitter for a more organic look.
            let sx = 0.7 + 0.4 * rand_unit(&mut self.rng_seed).abs();
            let sy = 0.7 + 0.4 * rand_unit(&mut self.rng_seed).abs();
            let sz = 0.7 + 0.4 * rand_unit(&mut self.rng_seed).abs();
            self.gibs.push(Gib {
                pos: pos + Vec3::new(rx, ry, rz) * 1.5,
                rot: Quat::IDENTITY,
                vel: v,
                ang_vel: Vec3::new(ax, ay, az) * 8.0,
                half_extents: Vec3::new(half.x * sx, half.y * sy, half.z * sz),
                color,
                life: GIB_LIFE,
            });
        }
    }

    /// Simple gravity for unsupported breakable bricks.  Each frame:
    ///   • For every visible brick that has durability (i.e. is breakable)
    ///     and is not on the floor, scan all other visible bricks for one
    ///     whose top is within ~2 cm of my bottom AND whose XZ AABB overlaps
    ///     mine.  If no support found → apply gravity tick.
    ///   • When a falling brick hits the floor (y_min ≤ 0) it stops.
    ///
    /// Cost: O(N²) per frame.  With ~290 apartment bricks that's ~84k checks
    /// — fine on RTX 2060.  vox-objects don't fall in V1.
    fn update_apartment_physics(&mut self, dt: f32) {
        const GRAVITY: f32 = 800.0;
        let n = self.apartment.bricks.len();
        // Build a snapshot of static-ish (non-falling) supporters.  We
        // re-read from the live array because a brick that just landed
        // becomes a supporter for the brick falling onto it.
        for i in 0..n {
            let b = &self.apartment.bricks[i];
            if !b.visible || b.durability.is_none() {
                continue;
            }
            let my_min_y = b.world_position.y - b.half_extents.y * b.scale.y;
            if my_min_y <= 0.5 {
                continue;
            } // resting on floor
            let my_he = Vec3::new(
                b.half_extents.x * b.scale.x,
                b.half_extents.y * b.scale.y,
                b.half_extents.z * b.scale.z,
            );
            let my_min_x = b.world_position.x - my_he.x;
            let my_max_x = b.world_position.x + my_he.x;
            let my_min_z = b.world_position.z - my_he.z;
            let my_max_z = b.world_position.z + my_he.z;
            // Look for any visible brick whose top sits within 2 cm of my
            // bottom and whose XZ AABB overlaps mine.
            let mut supported = false;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let s = &self.apartment.bricks[j];
                if !s.visible {
                    continue;
                }
                let s_he = Vec3::new(
                    s.half_extents.x * s.scale.x,
                    s.half_extents.y * s.scale.y,
                    s.half_extents.z * s.scale.z,
                );
                let s_top = s.world_position.y + s_he.y;
                if (s_top - my_min_y).abs() > 2.5 {
                    continue;
                }
                if s.world_position.x + s_he.x < my_min_x {
                    continue;
                }
                if s.world_position.x - s_he.x > my_max_x {
                    continue;
                }
                if s.world_position.z + s_he.z < my_min_z {
                    continue;
                }
                if s.world_position.z - s_he.z > my_max_z {
                    continue;
                }
                supported = true;
                break;
            }
            if supported {
                continue;
            }
            // Vox-objects can also support — treat their AABB top similarly.
            for v in &self.vox_objects {
                if v.is_empty() {
                    continue;
                }
                let (mn, mx) = v.world_aabb();
                if (mx.y - my_min_y).abs() > 2.5 {
                    continue;
                }
                if mx.x < my_min_x || mn.x > my_max_x {
                    continue;
                }
                if mx.z < my_min_z || mn.z > my_max_z {
                    continue;
                }
                supported = true;
                break;
            }
            if supported {
                continue;
            }
            // Falling — integrate gravity.
            let bm = &mut self.apartment.bricks[i];
            bm.vy -= GRAVITY * dt;
            bm.world_position.y += bm.vy * dt;
            // Floor clamp.
            let floor_y = my_he.y; // brick's centre when its bottom touches y=0
            if bm.world_position.y <= floor_y {
                bm.world_position.y = floor_y;
                bm.vy = 0.0;
            }
        }
    }

    fn update_gibs(&mut self, dt: f32) {
        for g in self.gibs.iter_mut() {
            g.vel.y -= GIB_GRAVITY * dt;
            g.pos += g.vel * dt;
            // Floor bounce
            let floor_y = g.half_extents.y;
            if g.pos.y < floor_y {
                g.pos.y = floor_y;
                if g.vel.y < 0.0 {
                    g.vel.y = -g.vel.y * GIB_BOUNCE;
                }
                g.vel.x *= GIB_FRICTION;
                g.vel.z *= GIB_FRICTION;
                g.ang_vel *= 0.85;
            }
            let av_len = g.ang_vel.length();
            if av_len > 1e-4 {
                let axis = g.ang_vel / av_len;
                g.rot = (Quat::from_axis_angle(axis, av_len * dt) * g.rot).normalize();
            }
            g.life -= dt;
        }
        self.gibs.retain(|g| g.life > 0.0);

        // Mirror gibs into a BrickModel so we can reuse to_mesh().
        self.gibs_model.bricks.clear();
        for g in &self.gibs {
            let mut b = Brick::new("gib", g.half_extents, g.color);
            b.world_position = g.pos;
            b.world_rotation = g.rot;
            self.gibs_model.bricks.push(b);
        }
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
        } else {
            1.0
        };

        for b in self.cat_model.bricks.iter_mut() {
            let is_strike_limb = match b.parent {
                Some(p) => p == upper_r || p == forearm_r || p == paw_fr,
                None => false,
            };
            b.scale = if is_strike_limb {
                Vec3::splat(paw_pump)
            } else {
                Vec3::ONE
            };
        }
        // Re-apply world transforms after scale change (positions don't depend on scale,
        // but world_transform does — it's read in append_brick via world_transform()).
        // Our Brick::world_transform uses self.scale directly, so no re-compute needed.
    }

    // ── Manual fly camera ─────────────────────────────────
    fn update_fly(&mut self, dt: f32) {
        if self.mode != Mode::ManualFly {
            return;
        }
        let speed = if self.keys.shift { 700.0 } else { 220.0 } * dt;
        let f = Vec3::new(
            self.fly_yaw.sin() * self.fly_pitch.cos(),
            self.fly_pitch.sin(),
            self.fly_yaw.cos() * self.fly_pitch.cos(),
        );
        let flat = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
        let right = flat.cross(Vec3::Y).normalize_or_zero();
        if self.keys.w {
            self.fly_pos += flat * speed;
        }
        if self.keys.s {
            self.fly_pos -= flat * speed;
        }
        if self.keys.a {
            self.fly_pos -= right * speed;
        }
        if self.keys.d {
            self.fly_pos += right * speed;
        }
        if self.keys.q {
            self.fly_pos.y -= speed;
        }
        if self.keys.e {
            self.fly_pos.y += speed;
        }
    }

    // ── Camera eye/center from mode ───────────────────────
    fn compute_camera(&self) -> (Vec3, Vec3) {
        if self.beauty_mode {
            let target = self.cat_pos + Vec3::new(0.0, 12.5, 3.5);
            let eye = target + Vec3::new(24.0, 12.0, 58.0);
            return (eye, target);
        }
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
        // Crosshair and probe share the exact ray of the swipe (origin + dir).
        let aim_yaw = self.cat_yaw + self.cam_yaw;
        let pitch = self.cam_pitch;
        let aim_fwd_3d = Vec3::new(
            aim_yaw.sin() * pitch.cos(),
            -pitch.sin(),
            aim_yaw.cos() * pitch.cos(),
        )
        .normalize();
        let probe_origin = self.cat_pos + Vec3::Y * (CAM_HEIGHT_OFFSET - 4.0);
        let crosshair_center = probe_origin + aim_fwd_3d * SWIPE_REACH_CM;
        let probe_dir = aim_fwd_3d;
        let probe_dist = SWIPE_REACH_CM * 1.4;
        let on_target = self
            .apartment
            .raycast_breakable(probe_origin, probe_dir, probe_dist)
            .is_some()
            || self.vox_objects.iter().any(|v| {
                if v.is_empty() {
                    return false;
                }
                let (mn, mx) = v.world_aabb();
                ray_aabb_box(probe_origin, probe_dir, mn, mx, probe_dist).is_some()
            });

        // Billboard basis: the quad faces the camera.
        let view_dir = (crosshair_center - eye).normalize_or_zero();
        let right = view_dir.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(view_dir).normalize_or_zero();

        // Pulse with swipe for feedback
        let size_mul = match self.swipe_t {
            Some(tn) => {
                let s = ((tn - 0.25) / 0.3).clamp(0.0, 1.0);
                1.0 + s * (1.0 - s) * 4.0 * 0.7 // peak ~1.7x mid-strike
            }
            None => 1.0,
        };
        let half = CROSSHAIR_SIZE_CM * 0.5 * size_mul;

        let color = if self.swipe_t.is_some() {
            [1.0, 0.35, 0.25, 1.0] // red mid-strike
        } else if on_target {
            [0.35, 1.0, 0.45, 1.0] // GREEN — target in range
        } else {
            [1.0, 0.82, 0.15, 1.0] // yellow — idle
        };
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
        mesh.indices.push(base);
        mesh.indices.push(base + 1);
        mesh.indices.push(base + 2);
        mesh.indices.push(base);
        mesh.indices.push(base + 2);
        mesh.indices.push(base + 3);
        mesh.triangle_count += 2;

        // Second side (backface) so it's visible from both directions — since we cull back-faces.
        let n2 = [view_dir.x, view_dir.y, view_dir.z];
        let base2 = mesh.vertices.len() as u32;
        for p in [c0, c3, c2, c1] {
            mesh.vertices.push(MeshVertex {
                position: [p.x, p.y, p.z],
                normal: n2,
                color,
                material: 0,
            });
        }
        mesh.indices.push(base2);
        mesh.indices.push(base2 + 1);
        mesh.indices.push(base2 + 2);
        mesh.indices.push(base2);
        mesh.indices.push(base2 + 2);
        mesh.indices.push(base2 + 3);
        mesh.triangle_count += 2;

        mesh
    }

    // ── Init ──────────────────────────────────────────────
    fn init_gpu(&mut self, window: Arc<Window>) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("No GPU");
        println!("  GPU: {}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Prometheus"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .unwrap();

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .unwrap();
        // Allow texture-to-buffer copies so we can grab screenshots.
        config.usage |= wgpu::TextureUsages::COPY_SRC;
        surface.configure(&device, &config);

        let (pipeline, bgl) = render_mesh::create_mesh_pipeline(&device, config.format);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniforms"),
            contents: bytemuck::bytes_of(&MeshUniforms::new(
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                Vec3::ZERO,
                Vec3::new(0.4, -0.75, 0.3).normalize(),
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BG"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let (_, depth_view) =
            render_mesh::create_depth_texture(&device, config.width, config.height);

        // Build apartment GPU mesh once — static.
        let apt_mesh = self.apartment.to_mesh();
        println!(
            "  Apartment mesh: {} tris, {} verts",
            apt_mesh.triangle_count,
            apt_mesh.vertices.len()
        );
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

        // World only ticks when not paused.  We keep rendering so the menu
        // overlay can sit on top of a still scene.
        if !self.paused {
            self.update_cat(dt);
            self.update_fly(dt);
            self.update_apartment_physics(dt);
            self.update_gibs(dt);
        } else {
            // Even paused, decay flash on bricks slowly so visual state stays
            // crisp (optional — no-op for now).
        }
        // Apply cursor grab state — locked while playing, free in menu.
        self.apply_cursor_grab();

        // Rebuild dynamic GPU meshes every frame.
        // Cat  — animated.  Apartment — flash tint + broken-brick pruning.
        // Gibs — physics-driven debris.  Crosshair — billboard.
        if let Some(device) = &self.device {
            let cat_mesh = self.cat_model.to_mesh();
            self.cat_gpu = GpuMesh::from_chunk_mesh(device, &cat_mesh);

            let apt_mesh = self.apartment.to_mesh();
            self.apartment_gpu = GpuMesh::from_chunk_mesh(device, &apt_mesh);

            let gibs_mesh = self.gibs_model.to_mesh();
            self.gibs_gpu = GpuMesh::from_chunk_mesh(device, &gibs_mesh);

            // Voxel chunks — rebuild combined mesh only when something
            // changed (carve / annihilate flips dirty).
            let any_dirty = self.vox_objects.iter().any(|v| v.dirty);
            if any_dirty || self.vox_gpu.is_none() {
                let mut combined = ChunkMesh::new();
                for v in self.vox_objects.iter_mut() {
                    if v.is_empty() {
                        v.dirty = false;
                        continue;
                    }
                    let m = v.build_mesh();
                    let base = combined.vertices.len() as u32;
                    combined.vertices.extend_from_slice(&m.vertices);
                    for ix in &m.indices {
                        combined.indices.push(base + ix);
                    }
                    combined.triangle_count += m.triangle_count;
                    v.dirty = false;
                }
                self.vox_gpu = GpuMesh::from_chunk_mesh(device, &combined);
            }

            let (eye_tmp, center_tmp) = self.compute_camera();
            if self.beauty_mode {
                self.crosshair_gpu = None;
            } else {
                let crosshair_mesh = self.build_crosshair_mesh(eye_tmp);
                self.crosshair_gpu = GpuMesh::from_chunk_mesh(device, &crosshair_mesh);
            }
            // Pause overlay — only when paused.
            if self.paused {
                let overlay = self.build_pause_overlay(eye_tmp, center_tmp);
                self.pause_overlay_gpu = GpuMesh::from_chunk_mesh(device, &overlay);
            } else {
                self.pause_overlay_gpu = None;
            }
            // Screenshot flash badge.
            if self.screenshot_flash_t > 0.0 {
                self.screenshot_flash_t = (self.screenshot_flash_t - dt).max(0.0);
                let flash = self.build_shot_flash(eye_tmp, center_tmp);
                self.shot_flash_gpu = GpuMesh::from_chunk_mesh(device, &flash);
            } else {
                self.shot_flash_gpu = None;
            }
        }

        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let surface = self.surface.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        let (eye, center) = self.compute_camera();
        let aspect = config.width as f32 / config.height as f32;
        let view = Mat4::look_at_rh(eye, center, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov.to_radians(), aspect, 1.0, 3000.0);

        let mut uniforms = MeshUniforms::new(
            view,
            proj,
            eye,
            if self.beauty_mode {
                Vec3::new(-0.35, 0.82, 0.45).normalize()
            } else {
                Vec3::new(0.4, -0.75, 0.3).normalize()
            },
        );
        if self.beauty_mode {
            uniforms.light_color = [1.0, 0.93, 0.82, 1.0];
            uniforms.ambient = [0.36, 0.42, 0.50, 0.62];
        }
        queue.write_buffer(
            self.uniform_buffer.as_ref().unwrap(),
            0,
            bytemuck::bytes_of(&uniforms),
        );

        if self.beauty_capture_countdown > 0 {
            self.beauty_capture_countdown -= 1;
            if self.beauty_capture_countdown == 0 {
                self.screenshot_pending = true;
            }
        }

        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                surface.configure(device, config);
                return;
            }
        };
        let view_tex = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.beauty_mode {
                            wgpu::Color {
                                r: 0.91,
                                g: 0.90,
                                b: 0.86,
                                a: 1.0,
                            }
                        } else {
                            wgpu::Color {
                                r: 0.72,
                                g: 0.82,
                                b: 0.92,
                                a: 1.0,
                            }
                        }),
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
            for mesh in [
                &self.apartment_gpu,
                &self.vox_gpu,
                &self.cat_gpu,
                &self.gibs_gpu,
                &self.crosshair_gpu,
                &self.pause_overlay_gpu,
                &self.shot_flash_gpu,
            ] {
                if let Some(m) = mesh {
                    pass.set_vertex_buffer(0, m.vertex_buffer.slice(..));
                    pass.set_index_buffer(m.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));

        if self.screenshot_pending {
            self.save_screenshot(&frame);
            self.screenshot_pending = false;
        }

        frame.present();
        self.win.as_ref().unwrap().request_redraw();
    }

    /// Read back the just-rendered surface texture and dump it as a PPM
    /// (binary P6) into `debug/screenshot_NNNN.ppm`.  PPM avoids needing an
    /// extra image-encoding crate; any common viewer (IrfanView, GIMP, ffmpeg)
    /// will open it.
    fn save_screenshot(&mut self, frame: &wgpu::SurfaceTexture) {
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();
        let w = config.width;
        let h = config.height;
        let bpp = 4u32;
        let unpadded_bpr = w * bpp;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bpr = (unpadded_bpr + align - 1) / align * align;
        let total_size = (padded_bpr as u64) * (h as u64);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screenshot-readback"),
            size: total_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("screenshot-copy"),
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bpr),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(enc.finish()));

        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).ok();
        });
        device.poll(wgpu::Maintain::Wait);
        if rx.recv().is_err() {
            return;
        }

        let data = buffer.slice(..).get_mapped_range();
        let format = config.format;
        let is_bgra = matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );

        std::fs::create_dir_all("debug").ok();
        // Find next free filename so screenshots don't overwrite each other.
        let mut idx = self.screenshot_index;
        let path = loop {
            let p = format!("debug/screenshot_{:04}.bmp", idx);
            if !std::path::Path::new(&p).exists() {
                break p;
            }
            idx += 1;
        };
        self.screenshot_index = idx + 1;

        // Build a 24-bit uncompressed BMP.  BMP rows are bottom-up and
        // padded to 4 bytes; pixel order is BGR.
        let row_bytes = ((w * 3 + 3) / 4) * 4;
        let pixel_size = row_bytes * h;
        let file_size = 14 + 40 + pixel_size;
        let mut out: Vec<u8> = Vec::with_capacity(file_size as usize);
        // BITMAPFILEHEADER (14 bytes)
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&file_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved
        out.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset
                                                     // BITMAPINFOHEADER (40 bytes)
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(w as i32).to_le_bytes());
        out.extend_from_slice(&(h as i32).to_le_bytes()); // positive = bottom-up
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&24u16.to_le_bytes()); // bpp
        out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        out.extend_from_slice(&pixel_size.to_le_bytes());
        out.extend_from_slice(&2835i32.to_le_bytes()); // 72 DPI x
        out.extend_from_slice(&2835i32.to_le_bytes()); // 72 DPI y
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // Pixel rows (bottom-up).  Each row is BGR + padding to 4-byte align.
        let pad = (row_bytes - w * 3) as usize;
        for y in (0..h).rev() {
            let row_start = (y * padded_bpr) as usize;
            for x in 0..w {
                let p = row_start + (x * bpp) as usize;
                // BMP wants BGR; surface is BGRA8 typically (or RGBA on some drivers).
                let (r, g, b) = if is_bgra {
                    (data[p + 2], data[p + 1], data[p])
                } else {
                    (data[p], data[p + 1], data[p + 2])
                };
                out.push(b);
                out.push(g);
                out.push(r);
            }
            for _ in 0..pad {
                out.push(0);
            }
        }
        drop(data);
        buffer.unmap();

        match std::fs::write(&path, &out) {
            Ok(()) => {
                println!("  📸 saved: {}", path);
                self.screenshot_flash_t = 0.6;
                if self.beauty_mode {
                    self.beauty_capture_complete = true;
                }
            }
            Err(e) => eprintln!("  ⚠ screenshot save failed: {}", e),
        }
    }
}

/// Slab-method ray vs axis-aligned box (mirrors core/brick.rs::ray_aabb).
fn ray_aabb_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3, max_dist: f32) -> Option<f32> {
    let dir = dir.normalize_or_zero();
    if dir.length_squared() < 1e-6 {
        return None;
    }
    let inv = Vec3::new(
        if dir.x.abs() > 1e-8 { 1.0 / dir.x } else { 1e8 },
        if dir.y.abs() > 1e-8 { 1.0 / dir.y } else { 1e8 },
        if dir.z.abs() > 1e-8 { 1.0 / dir.z } else { 1e8 },
    );
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let tmin = t1.min(t2).max_element();
    let tmax = t1.max(t2).min_element();
    if tmax >= tmin.max(0.0) && tmin <= max_dist {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// XZ-plane cylinder-vs-axis-aligned-box collision resolution.
/// Cat is a vertical cylinder at `pos` with the given radius, from
/// CAT_COLLIDER_Y_LO+pos.y to CAT_COLLIDER_Y_HI+pos.y. Bricks whose world AABBs
/// live outside that vertical slab (floors, high lintels) are skipped.
/// Voxel chunks (already-damaged objects) also push the cat — except those
/// fully empty.  V1 uses the chunk's outer AABB; V2 could query the SVO
/// for tighter collisions through holes.
fn resolve_cat_collision(
    pos_in: Vec3,
    apt: &BrickModel,
    vox: &[VoxObject],
    radius: f32,
    debug: bool,
) -> Vec3 {
    let mut pos = pos_in;
    let mut debug_blockers: Vec<String> = Vec::new();
    // Cat collider's vertical slab moves with the cat (so jumping clears low furniture).
    let slab_lo = pos.y + CAT_COLLIDER_Y_LO;
    let slab_hi = pos.y + CAT_COLLIDER_Y_HI;
    // Three passes handle corner pockets where two walls push you into a third.
    for _ in 0..3 {
        let mut any = false;
        // 1) Polygonal bricks
        for b in &apt.bricks {
            if !b.visible {
                continue;
            }
            let he = Vec3::new(
                b.half_extents.x * b.scale.x,
                b.half_extents.y * b.scale.y,
                b.half_extents.z * b.scale.z,
            );
            let min_y = b.world_position.y - he.y;
            let max_y = b.world_position.y + he.y;
            // <= so cat sitting exactly on top isn't shoved sideways.
            if max_y <= slab_lo || min_y >= slab_hi {
                continue;
            }

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
                if debug {
                    debug_blockers.push(b.name.clone());
                }
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
                    if m == pen_xp {
                        pos.x = max_x + radius;
                    } else if m == pen_xn {
                        pos.x = min_x - radius;
                    } else if m == pen_zp {
                        pos.z = max_z + radius;
                    } else {
                        pos.z = min_z - radius;
                    }
                }
            }
        }
        // 2) Voxel chunks (already-damaged objects).  We use the original
        // brick's AABB for the broad pre-test, then sample the SVO so the
        // cat can walk through carved holes.
        for v in vox {
            if v.is_empty() {
                continue;
            }
            let (mn, mx) = v.world_aabb();
            if mx.y <= slab_lo || mn.y >= slab_hi {
                continue;
            }
            // Single point sample at the cat's centre — keeps small carved
            // holes passable.  The cat's body squeezes through if the column
            // it stands on is empty in the SVO, even if the original AABB
            // still surrounds it.
            if !v.column_solid(glam::Vec2::new(pos.x, pos.z), slab_lo, slab_hi) {
                continue;
            }
            let cx = pos.x.clamp(mn.x, mx.x);
            let cz = pos.z.clamp(mn.z, mx.z);
            let dx = pos.x - cx;
            let dz = pos.z - cz;
            let d2 = dx * dx + dz * dz;
            let r2 = radius * radius;
            if d2 < r2 - 1e-4 {
                any = true;
                if debug {
                    debug_blockers.push(format!(
                        "vox#{}",
                        vox.iter().position(|x| std::ptr::eq(x, v)).unwrap_or(0)
                    ));
                }
                if d2 > 1e-4 {
                    let d = d2.sqrt();
                    let push = radius - d;
                    pos.x += dx / d * push;
                    pos.z += dz / d * push;
                } else {
                    let pen_xp = (mx.x + radius) - pos.x;
                    let pen_xn = pos.x - (mn.x - radius);
                    let pen_zp = (mx.z + radius) - pos.z;
                    let pen_zn = pos.z - (mn.z - radius);
                    let m = pen_xp.min(pen_xn).min(pen_zp).min(pen_zn);
                    if m == pen_xp {
                        pos.x = mx.x + radius;
                    } else if m == pen_xn {
                        pos.x = mn.x - radius;
                    } else if m == pen_zp {
                        pos.z = mx.z + radius;
                    } else {
                        pos.z = mn.z - radius;
                    }
                }
            }
        }
        if !any {
            break;
        }
    }
    if debug && !debug_blockers.is_empty() {
        let pushed = (pos - pos_in).length();
        if pushed > 0.05 {
            // Dedup names to avoid spamming "wall_seg2 wall_seg2 wall_seg2"
            debug_blockers.sort();
            debug_blockers.dedup();
            println!(
                "  🐛 cat ({:.0},{:.0}) pushed {:.1}cm by [{}]",
                pos.x,
                pos.z,
                pushed,
                debug_blockers.join(", ")
            );
        }
    }
    pos
}

/// Walk the apartment's bricks and tag everything that isn't structure as breakable.
/// Walls, floors, ceilings, dividers and door frames stay indestructible —
/// everything else gets a Durability that matches its material by name.
fn mark_apartment_breakables(apt: &mut BrickModel) {
    let mut by_mat = std::collections::HashMap::<&'static str, usize>::new();
    for b in apt.bricks.iter_mut() {
        let lname = b.name.to_lowercase();
        // Building shell — never breakable.  apartment.rs uses these prefixes:
        //   ow_*           outer walls
        //   fl_*           floors
        //   west_corr_*    long corridor wall (segments, lintels, frames)
        //   east_corr_*    long corridor wall (segments, lintels, frames)
        //   div_left_*     bath/br2/br1 dividers
        //   div_right_*    entry/kitchen/living dividers
        //   entry_frame_*  outside-door frame
        //   *_lintel*      lintel above any door gap
        //   *_frame_*      decorative door frames
        if lname.starts_with("ow_")
            || lname.starts_with("fl_")
            || lname.starts_with("ceil")
            || lname.starts_with("wall")
            || lname.starts_with("floor")
            || lname.starts_with("entry_frame")
            || lname.starts_with("west_corr")
            || lname.starts_with("east_corr")
            || lname.starts_with("div_")
            || lname.starts_with("front_door")
            || lname.contains("_lintel")
            || lname.contains("_frame_")
            || lname.contains("divider")
            || lname.contains("corridor_wall")
            || lname.contains("door_frame")
            || lname.contains("baseboard")
        {
            continue;
        }
        // Preserve bricks whose durability was deliberately tuned elsewhere (e.g. the
        // reinforced doors from close_some_doors, ~5 swipes to bust through). The
        // generic material pass below must not silently downgrade them to wood (3 hp).
        if b.durability.is_some() {
            continue;
        }
        let (dur, tag) = if lname.contains("bulb")
            || lname.contains("mirror")
            || lname.contains("tv_screen")
            || lname.contains("window")
            || lname.contains("vase")
            || lname.contains("glass")
            || lname.contains("lamp_shade")
        {
            (Durability::glass(), "glass")
        } else if lname.contains("plate")
            || lname.contains("fruit")
            || lname.contains("bowl")
            || lname.contains("pot")
            || lname.contains("porcelain")
            || lname.contains("toilet")
            || lname.contains("sink")
            || lname.contains("tub")
        {
            (Durability::ceramic(), "ceramic")
        } else if lname.contains("pillow")
            || lname.contains("blanket")
            || lname.contains("linen")
            || lname.contains("rug")
            || lname.contains("towel")
            || lname.contains("curtain")
            || lname.contains("jacket")
            || lname.contains("mattress")
            || lname.contains("sofa")
            || lname.contains("cushion")
            || lname.contains("bed_frame_mattress")
        {
            (Durability::fabric(), "fabric")
        } else if lname.contains("fridge")
            || lname.contains("stove")
            || lname.contains("metal")
            || lname.contains("oven")
        {
            (Durability::metal(), "metal")
        } else if lname.contains("book") || lname.contains("paper") {
            (Durability::paper(), "paper")
        } else {
            // Generic — wood (tables, chairs, frames, doors, shelves, …)
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
        if self.win.is_some() {
            return;
        }
        let build = env!("CARGO_PKG_VERSION");
        let build_tag = env!("BUILD_TAG");
        let w = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title(format!(
                        "PROMETHEUS — Cat in the Apartment — v{build} build {build_tag}"
                    ))
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
            )
            .unwrap(),
        );
        self.init_gpu(w);
    }

    fn window_event(
        &mut self,
        el: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        ev: WindowEvent,
    ) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => {
                self.render();
                if self.beauty_mode && self.beauty_capture_complete {
                    el.exit();
                }
            }
            WindowEvent::Resized(size) => {
                if let (Some(device), Some(surface), Some(config)) = (
                    self.device.as_ref(),
                    self.surface.as_ref(),
                    self.config.as_mut(),
                ) {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(device, config);
                    let (_, dv) =
                        render_mesh::create_depth_texture(device, config.width, config.height);
                    self.depth_view = Some(dv);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state.is_pressed();
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) if pressed => {
                        // Toggle pause / menu.  First Esc opens menu;
                        // a second Esc resumes the world.
                        self.paused = !self.paused;
                        println!(
                            "  {} (Esc again to {})",
                            if self.paused {
                                "⏸ PAUSED — menu open"
                            } else {
                                "▶ Resumed"
                            },
                            if self.paused { "resume" } else { "pause" }
                        );
                    }
                    // Q in pause menu = quit; in cat-mode it's reserved for
                    // future use (and the legacy fly-cam vertical descend).
                    PhysicalKey::Code(KeyCode::Tab) if pressed => {
                        self.mode = match self.mode {
                            Mode::Cat => {
                                let (e, _) = self.compute_camera();
                                self.fly_pos = e;
                                Mode::ManualFly
                            }
                            Mode::ManualFly => Mode::Cat,
                        };
                        println!(
                            "  Mode: {}",
                            match self.mode {
                                Mode::Cat => "CAT (WASD=move, Space=swipe, drag mouse=orbit)",
                                Mode::ManualFly => "FLY (WASD/QE, drag mouse, Shift=fast)",
                            }
                        );
                    }
                    PhysicalKey::Code(KeyCode::Space) if pressed => {
                        // SPACE = jump; only if cat is on the ground.
                        if self.mode == Mode::Cat && self.cat_grounded {
                            self.cat_vy = CAT_JUMP_SPEED;
                            self.cat_grounded = false;
                        }
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
                        if pressed =>
                    {
                        self.fov = (self.fov + 5.0).min(120.0)
                    }
                    PhysicalKey::Code(KeyCode::Minus)
                    | PhysicalKey::Code(KeyCode::NumpadSubtract)
                        if pressed =>
                    {
                        self.fov = (self.fov - 5.0).max(20.0)
                    }
                    PhysicalKey::Code(KeyCode::Delete) if pressed => {
                        self.screenshot_pending = true;
                        println!("  📸 screenshot queued");
                    }
                    PhysicalKey::Code(KeyCode::F1) if pressed => {
                        self.debug_collision = !self.debug_collision;
                        println!(
                            "  🐛 collision debug: {}",
                            if self.debug_collision { "ON" } else { "OFF" }
                        );
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state.is_pressed();
                // Swallow gameplay mouse input while paused.
                if self.paused {
                    return;
                }
                match button {
                    winit::event::MouseButton::Left => match self.mode {
                        Mode::Cat => {
                            if pressed {
                                self.keys.space_edge = true;
                            }
                        }
                        Mode::ManualFly => self.mouse_dragging = pressed,
                    },
                    winit::event::MouseButton::Right => {
                        // RMB held = free orbit camera around the cat.
                        // On release, snap the camera back behind the cat.
                        self.rmb_held = pressed;
                        if !pressed && self.mode == Mode::Cat {
                            self.cam_yaw = 0.0;
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Gameplay yaw/pitch live in `device_event::MouseMotion` so they
                // keep working when the cursor is grabbed.  We just track the
                // last absolute position for any future menu-mode use.
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

    /// Raw mouse motion — works even when the cursor is grabbed/hidden.
    /// `WindowEvent::CursorMoved` is suppressed under CursorGrabMode::Locked,
    /// so we route gameplay yaw/pitch through the device-level event.
    fn device_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.paused {
                return;
            }
            let dx = delta.0 as f32;
            let dy = delta.1 as f32;
            match self.mode {
                Mode::Cat => {
                    if self.rmb_held {
                        self.cam_yaw -= dx * CAT_MOUSE_SENS;
                    } else {
                        self.cat_yaw -= dx * CAT_MOUSE_SENS;
                        self.cat_yaw = self.cat_yaw.rem_euclid(std::f32::consts::TAU);
                    }
                    self.cam_pitch = (self.cam_pitch + dy * 0.003).clamp(-0.35, 1.1);
                }
                Mode::ManualFly => {
                    if self.mouse_dragging {
                        self.fly_yaw -= dx * 0.004;
                        self.fly_pitch = (self.fly_pitch + dy * 0.003).clamp(-1.3, 1.3);
                    }
                }
            }
        }
    }
}

fn main() {
    env_logger::init();
    let beauty_mode = std::env::args().any(|arg| arg == "--cat-beauty");
    println!();
    println!("  ═══════════════════════════════════════════");
    println!("  🐱  PROMETHEUS — Cat in the Apartment");
    println!("      Chibi tabby in a 72 m² П-44.  1 vox = 1 cm.");
    println!("  ═══════════════════════════════════════════");
    println!();
    println!("  Controls (mouse turns the cat, body-relative WASD):");
    println!("    Mouse         — turn the cat (yaw) + camera pitch");
    println!("    W / S         — forward / back");
    println!("    A / D         — strafe left / right (no rotation)");
    println!("    Shift         — run");
    println!("    Left mouse    — paw SWIPE");
    println!("    Space         — JUMP");
    println!(
        "    Right mouse   — hold for free orbit (cat stands still); release snaps camera back"
    );
    println!("    Wheel         — zoom camera in / out");
    println!("    + / -         — FOV");
    println!("    Tab           — toggle fly-through debug camera");
    println!("    Esc           — pause / resume (cursor unlocks while paused)");
    println!("    Delete        — save screenshot to ./debug/screenshot_NNNN.bmp");
    println!("    F1            — toggle collision debug log (which bricks block the cat)");
    println!();
    println!("  Three corridor doors are boarded shut: bath, kitchen, bedroom-1.");
    println!("  Bust through — about five swipes each.  Other three are open.");
    println!();

    let el = EventLoop::new().unwrap();
    if beauty_mode {
        println!("  CAT BEAUTY SPIKE: studio view and automatic proof capture enabled.");
    }
    let mut app = App::new(beauty_mode);
    el.run_app(&mut app).unwrap();
}

fn build_beauty_stage() -> BrickModel {
    let mut stage = BrickModel::new("CatBeautyStage");

    stage.add(
        Brick::new(
            "studio_floor",
            Vec3::new(120.0, 1.0, 110.0),
            [224, 218, 205],
        )
        .with_position(Vec3::new(0.0, -1.0, 0.0)),
    );
    stage.add(
        Brick::new(
            "display_plinth",
            Vec3::new(34.0, 2.0, 28.0),
            [203, 194, 178],
        )
        .with_position(Vec3::new(0.0, 1.5, 0.0)),
    );

    stage
}
