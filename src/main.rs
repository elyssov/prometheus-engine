// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Phase 1: Skeleton System
// GPU raymarching + proper skeleton with FK, constraints, attachments.
// ═══════════════════════════════════════════════════════════════

mod core;

use glam::{Mat4, Vec3};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

const GRID: usize = 256;
const TOTAL: usize = GRID * GRID * GRID;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Voxel { packed: u32, flags: u32 }

impl Voxel {
    fn solid(mat: u8, r: u8, g: u8, b: u8) -> Self {
        Self { packed: (mat as u32)|((r as u32)<<8)|((g as u32)<<16)|((b as u32)<<24), flags: 0 }
    }
    fn empty() -> Self { Self { packed: 0, flags: 0 } }
    fn is_solid(&self) -> bool { self.packed & 0xFF != 0 }
}

// ─── Pose (skeleton keyframe) ────────────────────────────────
type J = (f32,f32,f32); // joint position

fn lerp_j(a:J, b:J, t:f32) -> J {
    (a.0+(b.0-a.0)*t, a.1+(b.1-a.1)*t, a.2+(b.2-a.2)*t)
}

#[derive(Clone,Copy)]
struct Pose {
    l_foot:J, l_ankle:J, l_knee:J, l_hip:J,
    r_foot:J, r_ankle:J, r_knee:J, r_hip:J,
    l_shoulder:J, l_elbow:J, l_wrist:J,
    r_shoulder:J, r_elbow:J, r_wrist:J,
    head:J,
}

impl Pose {
    const CX:f32 = 128.0;
    const CZ:f32 = 128.0;

    fn lerp(&self, other:&Pose, t:f32) -> Pose {
        Pose {
            l_foot:lerp_j(self.l_foot,other.l_foot,t), l_ankle:lerp_j(self.l_ankle,other.l_ankle,t),
            l_knee:lerp_j(self.l_knee,other.l_knee,t), l_hip:lerp_j(self.l_hip,other.l_hip,t),
            r_foot:lerp_j(self.r_foot,other.r_foot,t), r_ankle:lerp_j(self.r_ankle,other.r_ankle,t),
            r_knee:lerp_j(self.r_knee,other.r_knee,t), r_hip:lerp_j(self.r_hip,other.r_hip,t),
            l_shoulder:lerp_j(self.l_shoulder,other.l_shoulder,t), l_elbow:lerp_j(self.l_elbow,other.l_elbow,t),
            l_wrist:lerp_j(self.l_wrist,other.l_wrist,t),
            r_shoulder:lerp_j(self.r_shoulder,other.r_shoulder,t), r_elbow:lerp_j(self.r_elbow,other.r_elbow,t),
            r_wrist:lerp_j(self.r_wrist,other.r_wrist,t),
            head:lerp_j(self.head,other.head,t),
        }
    }

    /// Standing shooting pose (from refs)
    fn standing_shoot() -> Self {
        let (cx,cz) = (Self::CX,Self::CZ);
        Pose {
            l_foot:(cx-16.0,2.0,cz-20.0), l_ankle:(cx-16.0,16.0,cz-20.0),
            l_knee:(cx-12.0,56.0,cz-12.0), l_hip:(cx-10.0,92.0,cz-4.0),
            r_foot:(cx+16.0,2.0,cz+16.0), r_ankle:(cx+16.0,16.0,cz+16.0),
            r_knee:(cx+12.0,56.0,cz+8.0), r_hip:(cx+10.0,92.0,cz+4.0),
            l_shoulder:(cx-26.0,148.0,cz-6.0), l_elbow:(cx-16.0,136.0,cz-28.0), l_wrist:(cx-8.0,144.0,cz-48.0),
            r_shoulder:(cx+26.0,148.0,cz+6.0), r_elbow:(cx+16.0,136.0,cz-8.0), r_wrist:(cx+8.0,144.0,cz-28.0),
            head:(cx+4.0,168.0,cz-8.0),
        }
    }

    /// Calculate leg joints from hip and knee angles (in radians).
    /// hip_angle: + = forward (flexion), - = backward (extension). Max ±0.35 rad (±20°)
    /// Biomechanical leg IK (from Aelis data).
    /// hip_angle: radians, + = forward (flexion), - = backward (extension). Max ±30°.
    /// knee_flex: radians, ALWAYS POSITIVE, minimum 0.087 (5°). KNEE NEVER BENDS BACKWARDS.
    fn calc_leg(hip_pos: J, hip_angle: f32, knee_flex: f32, thigh_len: f32, shin_len: f32) -> (J, J, J, J) {
        let hip_a = hip_angle.clamp(-0.52, 0.52);      // ±30°
        let knee_f = knee_flex.clamp(0.087, 2.09);      // 5°-120°, ALWAYS POSITIVE

        // Thigh: swings from hip in YZ plane
        // hip_a > 0 = thigh swings forward (Z-), hip_a < 0 = backward (Z+)
        let thigh_dy = -thigh_len * hip_a.cos();
        let thigh_dz = thigh_len * hip_a.sin();  // + hip_a = forward = -Z in our world → FLIP sign
        let knee = (hip_pos.0, hip_pos.1 + thigh_dy, hip_pos.2 + thigh_dz);

        // Shin: bends from thigh direction by knee_flex angle
        // KEY FIX: shin angle = thigh's absolute angle PLUS knee flexion
        // Since knee_flex is ALWAYS positive, shin ALWAYS goes further backward
        // than the thigh. This prevents "chicken legs" (backwards knees).
        let shin_abs_angle = hip_a + knee_f;
        // Extra safety: shin can never point forward of vertical
        let shin_clamped = shin_abs_angle.max(0.087); // minimum ~5° from vertical, backward
        let shin_dy = -shin_len * shin_clamped.cos();
        let shin_dz = shin_len * shin_clamped.sin();
        let ankle = (knee.0, knee.1 + shin_dy, knee.2 + shin_dz);

        // Foot: never below platform (y=2)
        let foot_y = 4.0_f32.max(ankle.1 - 10.0);
        let foot = (ankle.0, foot_y, ankle.2);
        let ankle_adj = (ankle.0, foot_y + 10.0, ankle.2);

        (foot, ankle_adj, knee, hip_pos)
    }

    /// Procedural walk — biomechanically accurate.
    /// Real gait data: hip ±20°, knee 0-65°, proper phase relationships.
    /// phase: 0.0-1.0 = one full stride (two steps)
    fn walk_at_phase(phase: f32) -> Self {
        let (cx, cz) = (Self::CX, Self::CZ);
        let pi = std::f32::consts::PI;
        let pi2 = pi * 2.0;
        let thigh = 44.0;
        let shin = 40.0;

        // Each leg: 0.0-0.6 = stance (foot on ground), 0.6-1.0 = swing (foot in air)
        // Right leg is offset by 0.5

        // Degrees to radians
        fn d2r(deg: f32) -> f32 { deg * std::f32::consts::PI / 180.0 }

        fn leg_angles(phase: f32) -> (f32, f32) {
            let p = phase.rem_euclid(1.0);
            let pi = std::f32::consts::PI;

            // Hip angle from Aelis table (degrees → radians):
            // IC(0%)=+30°, LR(6%)=+25°, MSt(20%)=+10°, TSt(40%)=-10°,
            // PSw(55%)=-10°, ISw(65%)=+15°, MSw(80%)=+25°, TSw(95%)=+30°
            // Approximation: sin wave with range -10° to +30°, offset +10°
            let hip_deg = 10.0 + 20.0 * (pi * 2.0 * (0.25 - p)).sin();
            let hip = d2r(hip_deg);

            // Knee angle from Aelis table (ALWAYS POSITIVE, min 5°):
            // IC=5°, LR=15°, MSt=5°, TSt=5°, PSw=35°, ISw=60°, MSw=30°, TSw=5°
            let knee_deg = if p < 0.06 {
                // IC → LR: 5° → 15° (shock absorption)
                5.0 + 10.0 * (p / 0.06)
            } else if p < 0.12 {
                // LR → post-LR: 15° → 5°
                15.0 - 10.0 * ((p - 0.06) / 0.06)
            } else if p < 0.50 {
                // MSt → TSt: stays ~5° (nearly straight, supporting weight)
                5.0
            } else if p < 0.55 {
                // PSw: 5° → 35° (preparing to lift)
                5.0 + 30.0 * ((p - 0.50) / 0.05)
            } else if p < 0.65 {
                // ISw: 35° → 60° (peak flexion — foot clears ground)
                35.0 + 25.0 * ((p - 0.55) / 0.10)
            } else if p < 0.80 {
                // MSw: 60° → 25° (extending forward)
                60.0 - 35.0 * ((p - 0.65) / 0.15)
            } else {
                // TSw: 25° → 5° (straightening for heel strike)
                25.0 - 20.0 * ((p - 0.80) / 0.20)
            };

            (hip, d2r(knee_deg.max(5.0))) // NEVER below 5°
        }

        let (l_hip_a, l_knee_f) = leg_angles(phase);
        let (r_hip_a, r_knee_f) = leg_angles(phase + 0.5);

        // Bounce: body dips ~1.5 voxels, 2x per cycle (at each heel strike)
        let bounce = -2.0 * (phase * pi2 * 2.0).cos().max(0.0);

        let tilt = 2.0 * (phase * pi2).sin();

        let pelvis_y = 92.0 + bounce;
        let l_hip_pos = (cx - 10.0, pelvis_y + tilt, cz - 4.0);
        let r_hip_pos = (cx + 10.0, pelvis_y - tilt, cz - 4.0);

        let (lf,la,lk,lhip) = Self::calc_leg(l_hip_pos, l_hip_a, l_knee_f, thigh, shin);
        let (rf,ra,rk,rhip) = Self::calc_leg(r_hip_pos, r_hip_a, r_knee_f, thigh, shin);

        let sh_y = 148.0 + bounce;
        let sway = 2.0 * (phase * pi2).sin();

        // Arms: SAME as standing_shoot — weapon raised, walking in combat stance
        // Only legs move, upper body holds shooting position
        let shoot = Pose::standing_shoot();

        Pose {
            l_foot:lf, l_ankle:la, l_knee:lk, l_hip:lhip,
            r_foot:rf, r_ankle:ra, r_knee:rk, r_hip:rhip,
            // Upper body: locked in shooting stance, just sways with walk
            l_shoulder:(shoot.l_shoulder.0+sway, shoot.l_shoulder.1+bounce, shoot.l_shoulder.2),
            l_elbow:(shoot.l_elbow.0+sway, shoot.l_elbow.1+bounce, shoot.l_elbow.2),
            l_wrist:(shoot.l_wrist.0+sway, shoot.l_wrist.1+bounce, shoot.l_wrist.2),
            r_shoulder:(shoot.r_shoulder.0+sway, shoot.r_shoulder.1+bounce, shoot.r_shoulder.2),
            r_elbow:(shoot.r_elbow.0+sway, shoot.r_elbow.1+bounce, shoot.r_elbow.2),
            r_wrist:(shoot.r_wrist.0+sway, shoot.r_wrist.1+bounce, shoot.r_wrist.2),
            head:(shoot.head.0+sway*0.3, shoot.head.1+bounce, shoot.head.2),
        }
    }

    fn walk_left() -> Self { Self::walk_at_phase(0.0) }
    fn walk_right() -> Self { Self::walk_at_phase(0.5) }

    /// Standing shoot pose rotated by yaw (radians) around Y axis
    fn standing_shoot_aimed(yaw: f32, s: f32) -> Self {
        let base = Self::standing_shoot();
        let (cx, cz) = (Self::CX, Self::CZ);
        let cos_y = yaw.cos();
        let sin_y = yaw.sin();

        // Rotate joint around (cx, cz)
        let rot = |j: J| -> J {
            let dx = j.0 - cx;
            let dz = j.2 - cz;
            (cx + dx*cos_y - dz*sin_y, j.1, cz + dx*sin_y + dz*cos_y)
        };

        Pose {
            l_foot: rot(base.l_foot), l_ankle: rot(base.l_ankle),
            l_knee: rot(base.l_knee), l_hip: rot(base.l_hip),
            r_foot: rot(base.r_foot), r_ankle: rot(base.r_ankle),
            r_knee: rot(base.r_knee), r_hip: rot(base.r_hip),
            l_shoulder: rot(base.l_shoulder), l_elbow: rot(base.l_elbow), l_wrist: rot(base.l_wrist),
            r_shoulder: rot(base.r_shoulder), r_elbow: rot(base.r_elbow), r_wrist: rot(base.r_wrist),
            head: rot(base.head),
        }
    }

    /// Crouch shooting pose — right knee near ground
    fn crouch_shoot() -> Self {
        let (cx,cz) = (Self::CX,Self::CZ);
        let thigh = 44.0;
        let shin = 40.0;

        // Kneeling (×2)
        let lhip = (cx-12.0, 64.0, cz-4.0);
        let lk = (cx-12.0, 32.0, cz-24.0);
        let la = (cx-12.0, 12.0, cz-20.0);
        let lf = (cx-12.0, 2.0, cz-20.0);

        let rhip = (cx+12.0, 64.0, cz+4.0);
        let rk = (cx+12.0, 12.0, cz+8.0);
        let ra = (cx+12.0, 16.0, cz+24.0);
        let rf = (cx+12.0, 6.0, cz+28.0);

        Pose {
            l_foot:lf, l_ankle:la, l_knee:lk, l_hip:lhip,
            r_foot:rf, r_ankle:ra, r_knee:rk, r_hip:rhip,
            l_shoulder:(cx-24.0,116.0,cz-6.0), l_elbow:(cx-14.0,104.0,cz-28.0), l_wrist:(cx-6.0,112.0,cz-44.0),
            r_shoulder:(cx+24.0,116.0,cz+6.0), r_elbow:(cx+14.0,104.0,cz-8.0), r_wrist:(cx+6.0,112.0,cz-28.0),
            head:(cx+4.0,136.0,cz-8.0),
        }
    }
}

// ─── Animation ───────────────────────────────────────────────
struct Animation {
    keyframes: Vec<(Pose, f32)>, // (pose, duration in seconds)
    time: f32,
    total_time: f32,
}

impl Animation {
    fn soldier_cycle() -> Self {
        // Phases: walk(3s) → stop+shoot(1.5s) → crouch+shoot(1.5s) → stand+shoot(1.5s)
        // Total: 7.5s cycle
        Animation {
            keyframes: vec![], // not used — we compute procedurally
            time: 0.0,
            total_time: 7.5,
        }
    }

    fn advance(&mut self, dt: f32) -> Pose {
        self.time = (self.time + dt) % self.total_time;
        let t = self.time;

        if t < 3.0 {
            // Walk phase: continuous procedural walk
            let walk_phase = (t / 0.75) % 1.0; // one full step every 0.75s
            Pose::walk_at_phase(walk_phase)
        } else if t < 3.5 {
            // Transition: walk → stand shoot
            let blend = (t - 3.0) / 0.5;
            let s = blend * blend * (3.0 - 2.0 * blend);
            Pose::walk_at_phase(0.0).lerp(&Pose::standing_shoot(), s)
        } else if t < 4.5 {
            // Standing shoot (hold)
            Pose::standing_shoot()
        } else if t < 5.0 {
            // Transition: stand → crouch
            let blend = (t - 4.5) / 0.5;
            let s = blend * blend * (3.0 - 2.0 * blend);
            Pose::standing_shoot().lerp(&Pose::crouch_shoot(), s)
        } else if t < 6.0 {
            // Crouch shoot (hold)
            Pose::crouch_shoot()
        } else if t < 6.5 {
            // Transition: crouch → stand
            let blend = (t - 6.0) / 0.5;
            let s = blend * blend * (3.0 - 2.0 * blend);
            Pose::crouch_shoot().lerp(&Pose::standing_shoot(), s)
        } else if t < 7.0 {
            // Standing shoot again
            Pose::standing_shoot()
        } else {
            // Transition: stand → walk
            let blend = (t - 7.0) / 0.5;
            let s = blend * blend * (3.0 - 2.0 * blend);
            Pose::standing_shoot().lerp(&Pose::walk_at_phase(0.0), s)
        }
    }
}

struct Grid { data: Vec<Voxel> }

impl Grid {
    fn new() -> Self { Self { data: vec![Voxel::empty(); TOTAL] } }
    fn idx(x: usize, y: usize, z: usize) -> usize { z*GRID*GRID + y*GRID + x }
    fn set(&mut self, x: usize, y: usize, z: usize, v: Voxel) {
        if x<GRID && y<GRID && z<GRID { self.data[Self::idx(x,y,z)] = v; }
    }
    fn get(&self, x: usize, y: usize, z: usize) -> &Voxel { &self.data[Self::idx(x,y,z)] }

    fn fill_box(&mut self, x0:usize,y0:usize,z0:usize, x1:usize,y1:usize,z1:usize, v:Voxel) {
        for z in z0..=z1.min(GRID-1) { for y in y0..=y1.min(GRID-1) { for x in x0..=x1.min(GRID-1) {
            self.set(x,y,z,v);
        }}}
    }

    fn fill_cyl(&mut self, cx:f32, cz:f32, r:f32, y0:usize, y1:usize, v:Voxel) {
        let r2=r*r; let ri=r.ceil() as i32;
        for y in y0..=y1.min(GRID-1) { for dz in -ri..=ri { for dx in -ri..=ri {
            if (dx*dx+dz*dz) as f32 <= r2 {
                let x=(cx as i32+dx) as usize; let z=(cz as i32+dz) as usize;
                if x<GRID && z<GRID { self.set(x,y,z,v); }
            }
        }}}
    }

    fn fill_sphere(&mut self, cx:f32,cy:f32,cz:f32, r:f32, v:Voxel) {
        let r2=r*r; let ri=r.ceil() as i32;
        for dz in -ri..=ri { for dy in -ri..=ri { for dx in -ri..=ri {
            if (dx*dx+dy*dy+dz*dz) as f32 <= r2 {
                let x=(cx as i32+dx) as usize; let y=(cy as i32+dy) as usize; let z=(cz as i32+dz) as usize;
                if x<GRID&&y<GRID&&z<GRID { self.set(x,y,z,v); }
            }
        }}}
    }

    /// Draw a solid limb between two 3D points using spheres (no gaps on diagonals)
    fn limb(&mut self, ax:f32,ay:f32,az:f32, bx:f32,by:f32,bz:f32, r:f32, v:Voxel) {
        let dx=bx-ax; let dy=by-ay; let dz=bz-az;
        let len = (dx*dx+dy*dy+dz*dz).sqrt();
        let steps = (len*3.0).max(1.0) as usize; // dense steps to avoid gaps
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let cx = ax+dx*t; let cy = ay+dy*t; let cz = az+dz*t;
            self.fill_sphere(cx, cy, cz, r, v);
        }
    }

    fn destroy(&mut self, cx:f32,cy:f32,cz:f32, r:f32) {
        let r2=r*r;
        for z in 0..GRID { for y in 0..GRID { for x in 0..GRID {
            let dx=x as f32+0.5-cx; let dy=y as f32+0.5-cy; let dz=z as f32+0.5-cz;
            if dx*dx+dy*dy+dz*dz<r2 { self.data[Self::idx(x,y,z)]=Voxel::empty(); }
        }}}
    }

    /// Remove invisible interior voxels (all 6 neighbors solid)
    fn hollow(&mut self) {
        let mut remove = Vec::new();
        for z in 1..GRID-1 { for y in 1..GRID-1 { for x in 1..GRID-1 {
            let i = Self::idx(x,y,z);
            if !self.data[i].is_solid() { continue; }
            if self.data[i-1].is_solid() && self.data[i+1].is_solid()
            && self.data[i-GRID].is_solid() && self.data[i+GRID].is_solid()
            && self.data[i-GRID*GRID].is_solid() && self.data[i+GRID*GRID].is_solid() {
                remove.push(i);
            }
        }}}
        let n = remove.len();
        for i in remove { self.data[i] = Voxel::empty(); }
        println!("  Hollow: removed {} interior voxels", n);
    }

    fn clear(&mut self) { self.data.fill(Voxel::empty()); }

    /// Build scene: room + soldier
    fn build_from_pose(&mut self, p: &Pose) {
        self.clear();
        self.build_room();
        self.build_soldier_internal(p);
    }

    fn build_soldier(&mut self) {
        self.clear();
        self.build_room();
        self.build_soldier_internal(&Pose::standing_shoot());
    }

    /// Build a room around the soldier — walls, floor, ceiling, furniture
    fn build_room(&mut self) {
        let s = GRID as f32 / 128.0;
        let cx = GRID as f32 / 2.0;
        let cz = GRID as f32 / 2.0;
        let gs = GRID;

        // Materials
        let floor_m   = Voxel::solid(7, 180,155,115);   // bright wood floor
        let wall_m    = Voxel::solid(7, 220,215,205);   // bright plaster walls
        let wall_d    = Voxel::solid(7, 190,185,175);   // wall trim
        let ceiling_m = Voxel::solid(7, 235,235,240);   // white ceiling
        let table_m   = Voxel::solid(1, 110,70,35);     // dark wood table
        let table_top = Voxel::solid(1, 130,85,45);     // table top lighter
        let chair_m   = Voxel::solid(1, 100,65,30);     // chair wood
        let glass_m   = Voxel::solid(2, 180,210,230);   // glass/window
        let shelf_m   = Voxel::solid(1, 95,60,28);      // shelf
        let vase_m    = Voxel::solid(3, 180,60,50);     // red ceramic vase
        let book_m    = Voxel::solid(9, 60,80,140);     // blue book
        let book2_m   = Voxel::solid(9, 140,50,50);     // red book
        let lamp_m    = Voxel::solid(6, 200,200,180);   // lamp shade
        let metal_m   = Voxel::solid(4, 150,150,155);   // metal

        // Room fills most of the grid — soldier is ~100 voxels tall in 256 grid
        let margin = (10.0*s) as usize;
        let wall_t = (2.0*s).max(2.0) as usize;
        let x0 = margin;
        let x1 = gs - margin;
        let z0 = margin;
        let z1 = gs - margin;
        let room_h = gs - margin;

        // Floor
        self.fill_box(x0, 0, z0, x1, wall_t, z1, floor_m);

        // Ceiling (front edge open)
        self.fill_box(x0, room_h-wall_t, z0+wall_t, x1, room_h, z1, ceiling_m);

        // Back wall (Z = z1)
        self.fill_box(x0, 0, z1-wall_t, x1, room_h, z1, wall_m);

        // NO FRONT WALL — we look through it like a dollhouse

        // Left wall (X = x0) — starts at z0+wall_t to leave front open
        self.fill_box(x0, 0, z0+wall_t, x0+wall_t, room_h, z1, wall_m);
        let door_z = (cz as usize) - (10.0*s) as usize;
        let door_w = (10.0*s) as usize;
        let door_h = (40.0*s) as usize;
        for z in door_z..door_z+door_w { for y in wall_t+1..door_h { for x in x0..x0+wall_t+1 {
            if x<gs&&y<gs&&z<gs { self.set(x,y,z, Voxel::empty()); }
        }}}

        // Right wall (X = x1) — also starts at z0+wall_t
        self.fill_box(x1-wall_t, 0, z0+wall_t, x1, room_h, z1, wall_m);

        // Baseboard trim (darker strip at bottom of walls)
        let trim_h = (3.0*s) as usize;
        self.fill_box(x0, wall_t+1, z1-wall_t-1, x1, wall_t+trim_h, z1-wall_t, wall_d);
        self.fill_box(x0, wall_t+1, z0+wall_t, x1, wall_t+trim_h, z0+wall_t+1, wall_d);
        self.fill_box(x0+wall_t, wall_t+1, z0, x0+wall_t+1, wall_t+trim_h, z1, wall_d);
        self.fill_box(x1-wall_t-1, wall_t+1, z0, x1-wall_t, wall_t+trim_h, z1, wall_d);

        // === FURNITURE (proportional to soldier ~100 vox tall) ===

        // Table (back-right area)
        let tx = (cx + 35.0*s) as usize;
        let tz = (cz + 35.0*s) as usize;
        let tw = (10.0*s) as usize;
        let td = (6.0*s) as usize;
        let th = (20.0*s) as usize;
        let leg = (1.5*s).max(1.0) as usize;
        // Legs
        self.fill_box(tx-tw, wall_t+1, tz-td, tx-tw+leg, th, tz-td+leg, table_m);
        self.fill_box(tx+tw-leg, wall_t+1, tz-td, tx+tw, th, tz-td+leg, table_m);
        self.fill_box(tx-tw, wall_t+1, tz+td-leg, tx-tw+leg, th, tz+td, table_m);
        self.fill_box(tx+tw-leg, wall_t+1, tz+td-leg, tx+tw, th, tz+td, table_m);
        // Top
        let top_t = (1.5*s).max(1.0) as usize;
        self.fill_box(tx-tw-1, th, tz-td-1, tx+tw+1, th+top_t, tz+td+1, table_top);

        // Vase on table
        self.fill_sphere(tx as f32, (th+top_t) as f32 + 5.0*s, tz as f32, 3.0*s, vase_m);
        self.fill_sphere(tx as f32, (th+top_t) as f32 + 8.0*s, tz as f32, 2.5*s, vase_m);

        // Chair near table
        let chx = tx - (12.0*s) as usize;
        let ch_h = (16.0*s) as usize;
        let ch_w = (5.0*s) as usize;
        // Seat
        self.fill_box(chx-ch_w, ch_h, tz-ch_w, chx+ch_w, ch_h+(1.0*s).max(1.0) as usize, tz+ch_w, chair_m);
        // Legs
        let cl = (1.0*s).max(1.0) as usize;
        self.fill_box(chx-ch_w, wall_t+1, tz-ch_w, chx-ch_w+cl, ch_h, tz-ch_w+cl, chair_m);
        self.fill_box(chx+ch_w-cl, wall_t+1, tz-ch_w, chx+ch_w, ch_h, tz-ch_w+cl, chair_m);
        self.fill_box(chx-ch_w, wall_t+1, tz+ch_w-cl, chx-ch_w+cl, ch_h, tz+ch_w, chair_m);
        self.fill_box(chx+ch_w-cl, wall_t+1, tz+ch_w-cl, chx+ch_w, ch_h, tz+ch_w, chair_m);
        // Back
        self.fill_box(chx-ch_w, ch_h, tz+ch_w-cl, chx+ch_w, ch_h+(14.0*s) as usize, tz+ch_w, chair_m);

        // Bookshelf (back-left, against back wall)
        let bx = (cx - 40.0*s) as usize;
        let bz = z1 - wall_t - (6.0*s) as usize;
        let bw = (12.0*s) as usize;
        let bh = (42.0*s) as usize;
        let bd = (3.0*s) as usize;
        // Frame
        self.fill_box(bx-bw, wall_t+1, bz-bd, bx+bw, bh, bz+bd, shelf_m);
        // Shelves (horizontal gaps)
        for shelf_y_base in [12.0, 22.0, 32.0] {
            let sy = (shelf_y_base * s) as usize;
            // Clear shelf space
            self.fill_box(bx-bw+2, sy, bz-bd+1, bx+bw-2, sy+(9.0*s) as usize, bz+bd-1, Voxel::empty());
        }
        // Books on shelves
        self.fill_box(bx-bw+3, (12.0*s) as usize, bz-bd+1, bx-bw+3+(3.0*s) as usize, (12.0*s+7.0*s) as usize, bz, book_m);
        self.fill_box(bx-bw+3+(4.0*s) as usize, (12.0*s) as usize, bz-bd+1, bx-bw+3+(7.0*s) as usize, (12.0*s+6.0*s) as usize, bz, book2_m);
        self.fill_box(bx+2, (22.0*s) as usize, bz-bd+1, bx+2+(5.0*s) as usize, (22.0*s+8.0*s) as usize, bz, book_m);

        // Floor lamp (front-right)
        let lx = (cx + 40.0*s) as usize;
        let lz = (cz - 35.0*s) as usize;
        let pole_r = (1.0*s).max(1.0) as usize;
        // Pole
        self.fill_box(lx-pole_r, wall_t+1, lz-pole_r, lx+pole_r, (35.0*s) as usize, lz+pole_r, metal_m);
        // Shade
        self.fill_sphere(lx as f32, 38.0*s, lz as f32, 4.0*s, lamp_m);
    }

    fn build_soldier_internal(&mut self, p: &Pose) {
        // Scale: all base values are for 128³, multiply by s for current grid
        let s = GRID as f32 / 128.0;

        let coat   = Voxel::solid(5, 130,135,150);
        let coat_d = Voxel::solid(5, 100,102,118);
        let pants  = Voxel::solid(5, 95,100,110);
        let skin   = Voxel::solid(10,245,215,175);
        let skin_d = Voxel::solid(10,225,190,150);
        let visor  = Voxel::solid(2, 30,250,255);
        let gun    = Voxel::solid(4, 60,60,68);
        let gun_d  = Voxel::solid(4, 40,40,48);
        let rune   = Voxel::solid(1, 120,180,255);
        let boot   = Voxel::solid(10,80,68,55);
        let belt   = Voxel::solid(10,150,130,95);
        let hair   = Voxel::solid(10,90,70,45);
        let floor  = Voxel::solid(7, 120,120,128);
        let equip  = Voxel::solid(4,80,72,58);

        let cx = GRID as f32 / 2.0;
        let cz = GRID as f32 / 2.0;

        // Skeleton from pose
        let (lf,la,lk,lh) = (p.l_foot,p.l_ankle,p.l_knee,p.l_hip);
        let (rf,ra,rk,rh) = (p.r_foot,p.r_ankle,p.r_knee,p.r_hip);
        let (ls,le,lw) = (p.l_shoulder,p.l_elbow,p.l_wrist);
        let (rs,re,rw) = (p.r_shoulder,p.r_elbow,p.r_wrist);
        let head = p.head;

        // Floor
        let fp = (20.0*s) as usize;
        self.fill_box((cx as usize).saturating_sub(fp), 0, (cz as usize).saturating_sub(fp),
                      (cx as usize)+fp, 0, (cz as usize)+fp, floor);

        // Boots
        let br = 5.0*s;
        self.fill_box((lf.0-br) as usize, 1, (lf.2-br) as usize,
                      (lf.0+br) as usize, (3.0*s) as usize, (lf.2+br*0.8) as usize, boot);
        self.limb(lf.0, 3.0*s, lf.2, la.0, la.1, la.2, br, boot);
        self.fill_box((rf.0-br) as usize, 1, (rf.2-br) as usize,
                      (rf.0+br) as usize, (3.0*s) as usize, (rf.2+br*0.8) as usize, boot);
        self.limb(rf.0, 3.0*s, rf.2, ra.0, ra.1, ra.2, br, boot);

        // Legs
        self.limb(la.0,la.1,la.2, lk.0,lk.1,lk.2, 4.5*s, pants);
        self.limb(lk.0,lk.1,lk.2, lh.0,lh.1,lh.2, 5.0*s, pants);
        self.limb(ra.0,ra.1,ra.2, rk.0,rk.1,rk.2, 4.5*s, pants);
        self.limb(rk.0,rk.1,rk.2, rh.0,rh.1,rh.2, 5.0*s, pants);
        self.limb(lh.0,lh.1,lh.2, rh.0,rh.1,rh.2, 5.0*s, pants);

        // Torso
        let hip_y = (lh.1 + rh.1) / 2.0;
        let sh_y = (ls.1 + rs.1) / 2.0;
        let mid_y = (hip_y + sh_y) / 2.0;
        // Belt
        self.fill_box((cx-8.0*s) as usize, (hip_y-1.0*s) as usize, (cz-5.0*s) as usize,
                      (cx+8.0*s) as usize, (hip_y+2.0*s) as usize, (cz+5.0*s) as usize, belt);
        // Waist
        self.fill_box((cx-7.0*s) as usize, (hip_y+2.0*s) as usize, (cz-4.0*s) as usize,
                      (cx+7.0*s) as usize, mid_y as usize, (cz+5.0*s) as usize, coat);
        // Chest
        self.fill_box((cx-9.0*s) as usize, mid_y as usize, (cz-5.0*s) as usize,
                      (cx+9.0*s) as usize, sh_y as usize, (cz+5.0*s) as usize, coat);
        // Front shadow
        self.fill_box((cx-5.0*s) as usize, (hip_y+4.0*s) as usize, (cz-6.0*s) as usize,
                      (cx+5.0*s) as usize, (sh_y-4.0*s) as usize, (cz-5.0*s) as usize, coat_d);
        // Coat tails
        self.fill_box((cx-8.0*s) as usize, (hip_y-6.0*s) as usize, (cz+4.0*s) as usize,
                      (cx+8.0*s) as usize, hip_y as usize, (cz+8.0*s) as usize, coat_d);

        // Rune glow (relative to torso center)
        let rc = (cz - 10.0*s) as usize;
        let rmid = cx as usize;
        for dy in 0..(12.0*s) as usize {
            self.set(rmid - (2.0*s) as usize, (mid_y-2.0*s) as usize + dy, rc, rune);
            self.set(rmid + (2.0*s) as usize, (mid_y-2.0*s) as usize + dy, rc, rune);
        }

        // Shoulders
        self.fill_sphere(ls.0, ls.1, ls.2, 6.0*s, coat);
        self.fill_sphere(rs.0, rs.1, rs.2, 6.0*s, coat);

        // Arms
        self.limb(ls.0,ls.1,ls.2, le.0,le.1,le.2, 3.5*s, coat);
        self.limb(le.0,le.1,le.2, lw.0,lw.1,lw.2, 3.0*s, coat);
        self.fill_sphere(lw.0, lw.1, lw.2, 2.8*s, skin);
        self.limb(rs.0,rs.1,rs.2, re.0,re.1,re.2, 3.5*s, coat);
        self.limb(re.0,re.1,re.2, rw.0,rw.1,rw.2, 3.0*s, coat);
        self.fill_sphere(rw.0, rw.1, rw.2, 2.8*s, skin);

        // Weapon
        self.limb(rw.0,rw.1,rw.2, lw.0,lw.1,lw.2, 2.5*s, gun);
        let dx = lw.0-rw.0; let dy = lw.1-rw.1; let dz = lw.2-rw.2;
        let dl = (dx*dx+dy*dy+dz*dz).sqrt().max(0.1);
        let (nx,ny,nz) = (dx/dl, dy/dl, dz/dl);
        self.limb(lw.0,lw.1,lw.2, lw.0+nx*16.0*s, lw.1+ny*16.0*s, lw.2+nz*16.0*s, 1.5*s, gun_d);
        self.limb(rw.0,rw.1,rw.2, rs.0-3.0*s, rs.1-1.0*s, rs.2, 2.5*s, gun);
        let mx=(rw.0+lw.0)/2.0; let my=(rw.1+lw.1)/2.0; let mz=(rw.2+lw.2)/2.0;
        self.limb(mx,my-1.0*s,mz, mx,my-10.0*s,mz+1.0*s, 2.0*s, gun_d);

        // Collar + Neck
        self.fill_sphere(cx, sh_y+2.0*s, cz-2.0*s, 5.5*s, coat);
        self.limb(cx, sh_y+3.0*s, cz-2.0*s, head.0, head.1-5.0*s, head.2, 4.0*s, skin);

        // Head
        self.fill_sphere(head.0, head.1, head.2, 6.0*s, skin);
        self.fill_sphere(head.0, head.1+2.0*s, head.2+1.0*s, 5.5*s, hair);
        // Clear face from hair
        let (hx,hy,hz) = (head.0 as usize, head.1 as usize, head.2 as usize);
        let hr = (7.0*s) as usize;
        for z in hz.saturating_sub(hr)..=hz.saturating_sub(hr/3) {
            for x in hx.saturating_sub(hr)..=(hx+hr) {
                for y in hy.saturating_sub(hr/2)..=(hy+hr/2) {
                    if x<GRID&&y<GRID&&z<GRID && self.get(x,y,z).packed==hair.packed {
                        self.set(x,y,z, skin);
                    }
                }
            }
        }

        // Face
        let fz = (head.2 - 6.0*s) as usize;
        let vr = (5.0*s) as usize;
        for x in hx.saturating_sub(vr)..=(hx+vr) {
            self.set(x, hy, fz, visor);
            self.set(x, hy+1, fz, visor);
        }
        self.set(hx, hy-(2.0*s) as usize, fz.saturating_sub(1), skin_d);
        for x in hx.saturating_sub((2.0*s) as usize)..=(hx+(2.0*s) as usize) {
            self.set(x, hy-(3.0*s) as usize, fz, skin_d);
        }
        self.fill_sphere(head.0-6.5*s, head.1, head.2, 1.5*s, skin);
        self.fill_sphere(head.0+6.5*s, head.1, head.2, 1.5*s, skin);

        // Equipment
        self.fill_sphere(lh.0-5.0*s, lh.1, lh.2, 3.0*s, equip);
        self.fill_sphere(rh.0+5.0*s, rh.1, rh.2, 3.0*s, equip);
        self.fill_sphere(rh.0+5.0*s, rh.1-4.0*s, rh.2, 2.5*s, equip);
        self.limb(le.0-1.5*s, le.1-2.0*s, le.2, le.0-1.5*s, le.1+3.0*s, le.2, 2.0*s,
                  Voxel::solid(1,65,105,175));
    }

    #[allow(dead_code)]
    fn build_soldier_internal_OLD(&mut self, p: &Pose) {
        let _s: f32 = GRID as f32 / 128.0;
        let cx = GRID as f32 / 2.0;
        let cz = GRID as f32 / 2.0;

        // Palette — HIGH CONTRAST between body parts
        let coat   = Voxel::solid(5, 130,135,150);     // blue-grey coat (distinct from pants)
        let coat_d = Voxel::solid(5, 100,102,118);     // coat folds
        let pants  = Voxel::solid(5, 95,100,110);      // DARKER pants (contrast with coat)
        let skin   = Voxel::solid(10,245,215,175);      // WARM bright skin (stands out)
        let skin_d = Voxel::solid(10,225,190,150);      // skin shadow
        let visor  = Voxel::solid(2, 30,250,255);       // bright cyan visor (perfect)
        let gun    = Voxel::solid(4, 60,60,68);         // DARK gun metal (contrast with hands)
        let gun_d  = Voxel::solid(4, 40,40,48);         // barrel darker
        let rune   = Voxel::solid(1, 120,180,255);      // glowing blue rune
        let boot   = Voxel::solid(10,80,68,55);         // DARK brown boots (contrast with pants)
        let belt   = Voxel::solid(10,150,130,95);       // BRIGHT tan belt (reads clearly)
        let hair   = Voxel::solid(10,90,70,45);         // dark brown hair
        let floor  = Voxel::solid(7, 120,120,128);      // light grey floor

        let cx = 64.0_f32;
        let cz = 64.0_f32;

        // Floor
        self.fill_box(88,0,88, 168,0,168, floor);

        // --- SKELETON from Pose ---
        let (lf,la,lk,lh) = (p.l_foot,p.l_ankle,p.l_knee,p.l_hip);
        let (rf,ra,rk,rh) = (p.r_foot,p.r_ankle,p.r_knee,p.r_hip);
        let (ls,le,lw) = (p.l_shoulder,p.l_elbow,p.l_wrist);
        let (rs,re,rw) = (p.r_shoulder,p.r_elbow,p.r_wrist);
        let head = p.head;

        // --- SABATON BOOTS ×2 ---
        self.fill_box((lf.0-10.0) as usize, 1, (lf.2-12.0) as usize,
                      (lf.0+10.0) as usize, 6, (lf.2+8.0) as usize, boot);
        self.limb(lf.0, 8.0, lf.2, la.0, la.1, la.2, 10.0, boot);
        self.fill_box((rf.0-10.0) as usize, 1, (rf.2-12.0) as usize,
                      (rf.0+10.0) as usize, 6, (rf.2+8.0) as usize, boot);
        self.limb(rf.0, 8.0, rf.2, ra.0, ra.1, ra.2, 10.0, boot);

        // --- LEGS (×2) ---
        self.limb(la.0,la.1,la.2, lk.0,lk.1,lk.2, 9.0, pants);
        self.limb(lk.0,lk.1,lk.2, lh.0,lh.1,lh.2, 10.0, pants);
        self.limb(ra.0,ra.1,ra.2, rk.0,rk.1,rk.2, 9.0, pants);
        self.limb(rk.0,rk.1,rk.2, rh.0,rh.1,rh.2, 10.0, pants);
        self.limb(lh.0, lh.1, lh.2, rh.0, rh.1, rh.2, 10.0, pants);

        // --- TORSO (×2) ---
        let hip_y = (lh.1 + rh.1) / 2.0;
        let sh_y_t = (ls.1 + rs.1) / 2.0;
        let mid_y = (hip_y + sh_y_t) / 2.0;

        self.fill_box((cx-16.0) as usize, (hip_y-2.0) as usize, (cz-10.0) as usize,
                      (cx+16.0) as usize, (hip_y+4.0) as usize, (cz+10.0) as usize, belt);
        self.fill_box((cx-14.0) as usize, (hip_y+4.0) as usize, (cz-8.0) as usize,
                      (cx+14.0) as usize, mid_y as usize, (cz+10.0) as usize, coat);
        self.fill_box((cx-18.0) as usize, mid_y as usize, (cz-10.0) as usize,
                      (cx+18.0) as usize, sh_y_t as usize, (cz+10.0) as usize, coat);
        self.fill_box((cx-10.0) as usize, (hip_y+8.0) as usize, (cz-12.0) as usize,
                      (cx+10.0) as usize, (sh_y_t-8.0) as usize, (cz-10.0) as usize, coat_d);
        self.fill_box((cx-16.0) as usize, (hip_y-12.0) as usize, (cz+8.0) as usize,
                      (cx+16.0) as usize, (hip_y) as usize, (cz+16.0) as usize, coat_d);

        // --- RUNE GLOW (×2) ---
        let rcz = (cz-20.0) as usize;
        for dy in 0..24 {
            self.set(124, 116+dy, rcz, rune);
            self.set(132, 116+dy, rcz, rune);
        }
        for dx in [126,127,128,129,130] { self.set(dx, 124, rcz, rune); self.set(dx, 132, rcz, rune); }

        // --- SHOULDERS (×2) ---
        self.fill_sphere(ls.0, ls.1, ls.2, 12.0, coat);
        self.fill_sphere(rs.0, rs.1, rs.2, 12.0, coat);

        // --- ARMS (×2) ---
        self.limb(ls.0,ls.1,ls.2, le.0,le.1,le.2, 7.0, coat);
        self.limb(le.0,le.1,le.2, lw.0,lw.1,lw.2, 6.0, coat);
        self.fill_sphere(lw.0, lw.1, lw.2, 5.5, skin);
        self.limb(rs.0,rs.1,rs.2, re.0,re.1,re.2, 7.0, coat);
        self.limb(re.0,re.1,re.2, rw.0,rw.1,rw.2, 6.0, coat);
        self.fill_sphere(rw.0, rw.1, rw.2, 5.5, skin);

        // --- WEAPON (×2) ---
        self.limb(rw.0, rw.1, rw.2, lw.0, lw.1, lw.2, 5.0, gun);
        let dir_x = lw.0 - rw.0;
        let dir_y = lw.1 - rw.1;
        let dir_z = lw.2 - rw.2;
        let dir_len = (dir_x*dir_x + dir_y*dir_y + dir_z*dir_z).sqrt().max(0.1);
        let nx = dir_x / dir_len;
        let ny = dir_y / dir_len;
        let nz = dir_z / dir_len;
        self.limb(lw.0, lw.1, lw.2,
                  lw.0 + nx*32.0, lw.1 + ny*32.0, lw.2 + nz*32.0, 3.0, gun_d);
        self.limb(rw.0, rw.1, rw.2, rs.0-6.0, rs.1-2.0, rs.2, 5.0, gun);

        // Magazine (×2)
        let mag_x = (rw.0+lw.0)/2.0;
        let mag_y = (rw.1+lw.1)/2.0;
        let mag_z = (rw.2+lw.2)/2.0;
        self.limb(mag_x, mag_y-1.0, mag_z,
                  mag_x, mag_y-10.0, mag_z+1.0, 2.0, gun_d);

        // --- COLLAR (×2) ---
        let collar_y = sh_y_t + 2.0;
        self.fill_sphere(cx, collar_y, cz-4.0, 11.0, coat);

        // --- NECK (×2) ---
        self.limb(cx, collar_y+2.0, cz-4.0, head.0, head.1-10.0, head.2, 8.0, skin);

        // --- HEAD (×2, radius 12) ---
        self.fill_sphere(head.0, head.1, head.2, 12.0, skin);
        self.fill_sphere(head.0, head.1+4.0, head.2+2.0, 11.0, hair);
        let hx = head.0 as usize; let hy = head.1 as usize; let hz = head.2 as usize;
        for z in hz.saturating_sub(14)..=hz.saturating_sub(4) {
            for x in hx.saturating_sub(10)..=(hx+10) {
                for y in hy.saturating_sub(8)..=(hy+6) {
                    if x<GRID&&y<GRID&&z<GRID && self.get(x,y,z).packed==hair.packed {
                        self.set(x,y,z, skin);
                    }
                }
            }
        }

        // --- FACE (×2) ---
        let fz = (head.2 - 12.0) as usize;
        for x in (hx.saturating_sub(10))..=(hx+10) {
            self.set(x, hy, fz, visor);
            self.set(x, hy+1, fz, visor);
            self.set(x, hy+2, fz, visor);
        }
        self.set(hx, hy-2, fz.saturating_sub(1), skin_d);
        self.set(hx, hy-3, fz.saturating_sub(1), skin_d);
        for x in (hx.saturating_sub(4))..=(hx+4) { self.set(x, hy-6, fz, skin_d); }
        self.fill_sphere(head.0-13.0, head.1, head.2, 3.0, skin);
        self.fill_sphere(head.0+13.0, head.1, head.2, 3.0, skin);

        // --- EQUIPMENT (×2) ---
        let equip = Voxel::solid(4,80,72,58);
        self.fill_sphere(lh.0-10.0, lh.1, lh.2, 6.0, equip);
        self.fill_sphere(rh.0+10.0, rh.1, rh.2, 6.0, equip);
        self.fill_sphere(rh.0+10.0, rh.1-8.0, rh.2, 5.0, equip);
        self.limb(le.0-3.0, le.1-4.0, le.2, le.0-3.0, le.1+6.0, le.2, 4.0,
                  Voxel::solid(1,65,105,175));
    }
}

// ─── Camera ──────────────────────────────────────────────────
struct Camera { angle:f32, pitch:f32, dist:f32, center:Vec3 }
impl Camera {
    fn new() -> Self { Self {
        angle: 0.0,
        pitch: 0.15,
        dist: 380.0,
        center: Vec3::new(128.0, 90.0, 128.0),
    }}
    fn eye(&self) -> Vec3 {
        // Fixed front view: camera in front of room (low Z), looking into room (+Z)
        Vec3::new(
            self.center.x + self.dist * 0.1 * self.angle.sin(), // slight side pan if rotated
            self.center.y + self.dist * self.pitch.sin(),
            self.center.z - self.dist * self.pitch.cos(), // NEGATIVE Z = in front of room
        )
    }
    fn view(&self)->Mat4 { Mat4::look_at_rh(self.eye(),self.center,Vec3::Y) }
    fn proj(&self,a:f32)->Mat4 { Mat4::perspective_rh(std::f32::consts::FRAC_PI_4,a,0.1,500.0) }
}

// ─── Uniforms ────────────────────────────────────────────────
#[repr(C)]
#[derive(Copy,Clone,bytemuck::Pod,bytemuck::Zeroable)]
struct Uniforms { inv_vp:[[f32;4];4], eye:[f32;4], info:[f32;4] }

// ─── App ─────────────────────────────────────────────────────
struct GpuState {
    device:wgpu::Device, queue:wgpu::Queue, surface:wgpu::Surface<'static>,
    config:wgpu::SurfaceConfiguration, pipeline:wgpu::RenderPipeline,
    ubuf:wgpu::Buffer, vbuf:wgpu::Buffer, bg:wgpu::BindGroup,
}

// ─── Bullet system ───────────────────────────────────────────
struct Bullet {
    x:f32, y:f32, z:f32,
    dx:f32, dy:f32, dz:f32, // direction (normalized) × speed
    alive: bool,
}

struct App {
    win: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    grid: Grid,
    cam: Camera,
    time: f32,
    rotate: bool,
    frame_count: u32,
    bullets: Vec<Bullet>,
    soldier: core::entity::Entity,
    target_idx: usize,
    shoot_timer: f32,
}

impl App {
    fn new() -> Self {
        let mut g = Grid::new();
        // Build room into static grid
        g.build_room();

        // Create soldier entity
        let s = GRID as f32 / 128.0;
        let mut soldier = core::entity::Entity::orpp_soldier(s);
        soldier.set_position(Vec3::new(GRID as f32 / 2.0, 46.0 * s, GRID as f32 / 2.0));
        soldier.update();

        Self {
            win: None, gpu: None, grid: g, cam: Camera::new(),
            time: 0.0, rotate: false, frame_count: 0,
            bullets: Vec::new(), soldier, target_idx: 0, shoot_timer: 0.0,
        }
    }

    fn init_gpu(&mut self, w: Arc<Window>) {
        let inst = wgpu::Instance::new(&wgpu::InstanceDescriptor{backends:wgpu::Backends::all(),..Default::default()});
        let surf = inst.create_surface(w.clone()).unwrap();
        let adap = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions{
            power_preference:wgpu::PowerPreference::HighPerformance, compatible_surface:Some(&surf), force_fallback_adapter:false,
        })).expect("No GPU");
        println!("  GPU: {}", adap.get_info().name);
        let (dev,q) = pollster::block_on(adap.request_device(&wgpu::DeviceDescriptor{
            label:Some("P"), required_features:wgpu::Features::empty(), required_limits:wgpu::Limits::default(), memory_hints:wgpu::MemoryHints::default(),
        },None)).unwrap();
        let sz = w.inner_size();
        let cfg = surf.get_default_config(&adap,sz.width.max(1),sz.height.max(1)).unwrap();
        surf.configure(&dev,&cfg);
        let sh = dev.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("RM"),source:wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into())});
        let ubuf = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:Some("U"),
            contents:bytemuck::bytes_of(&Uniforms{inv_vp:Mat4::IDENTITY.to_cols_array_2d(),eye:[0.0;4],info:[GRID as f32,0.0,0.0,0.0]}),
            usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST});
        let vbuf = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:Some("V"),
            contents:bytemuck::cast_slice(&self.grid.data), usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_DST});
        let bgl = dev.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:&[
            wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
            wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::FRAGMENT,
                ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None},
        ]});
        let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&bgl,entries:&[
            wgpu::BindGroupEntry{binding:0,resource:ubuf.as_entire_binding()},
            wgpu::BindGroupEntry{binding:1,resource:vbuf.as_entire_binding()},
        ]});
        let pl = dev.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:None,bind_group_layouts:&[&bgl],push_constant_ranges:&[]});
        let pip = dev.create_render_pipeline(&wgpu::RenderPipelineDescriptor{
            label:Some("RM"),layout:Some(&pl),
            vertex:wgpu::VertexState{module:&sh,entry_point:Some("vs_main"),buffers:&[],compilation_options:Default::default()},
            fragment:Some(wgpu::FragmentState{module:&sh,entry_point:Some("fs_main"),targets:&[Some(cfg.format.into())],compilation_options:Default::default()}),
            primitive:wgpu::PrimitiveState{topology:wgpu::PrimitiveTopology::TriangleList,..Default::default()},
            depth_stencil:None,multisample:Default::default(),multiview:None,cache:None,
        });
        self.win=Some(w); self.gpu=Some(GpuState{device:dev,queue:q,surface:surf,config:cfg,pipeline:pip,ubuf,vbuf,bg});
    }

    fn upload(&self) { if let Some(g)=&self.gpu { g.queue.write_buffer(&g.vbuf,0,bytemuck::cast_slice(&self.grid.data)); } }

    fn render(&mut self) {
        let g=self.gpu.as_ref().unwrap();
        if self.rotate { self.cam.angle+=0.008; }
        self.time+=1.0/60.0;
        self.frame_count+=1;

        let s = GRID as f32 / 128.0;
        let cx = GRID as f32 / 2.0;
        let cz = GRID as f32 / 2.0;

        // ═══ ENTITY-BASED TARGETING ═══
        let targets = [
            Vec3::new(cx + 35.0*s, 22.0*s, cz + 35.0*s),
            Vec3::new(cx - 40.0*s, 25.0*s, cz + 50.0*s),
            Vec3::new(cx + 40.0*s, 30.0*s, cz - 35.0*s),
            Vec3::new(cx - 25.0*s, 18.0*s, cz + 30.0*s),
        ];

        let new_idx = ((self.time / 3.0) as usize) % targets.len();
        if new_idx != self.target_idx {
            self.target_idx = new_idx;
            self.shoot_timer = 0.0;
        }
        self.shoot_timer += 1.0 / 60.0;

        // ONE LINE: aim at target
        self.soldier.aim_at(targets[self.target_idx]);
        self.soldier.plant_feet(2.0 * s);
        self.soldier.update();

        // Rasterize every 4 frames
        if self.frame_count % 4 == 0 {
            self.grid.clear();
            self.grid.build_room();
            let grid_ref = &mut self.grid;
            self.soldier.rasterize(GRID, |x, y, z, mat, r, g, b| {
                grid_ref.set(x, y, z, Voxel::solid(mat, r, g, b));
            });
        }

        // FIRE from muzzle
        if self.shoot_timer > 1.5 && self.frame_count % 10 == 0 {
            if let Some(fire) = self.soldier.fire() {
                self.bullets.push(Bullet {
                    x: fire.origin.x, y: fire.origin.y, z: fire.origin.z,
                    dx: fire.direction.x * fire.speed * s,
                    dy: fire.direction.y * fire.speed * s,
                    dz: fire.direction.z * fire.speed * s,
                    alive: true,
                });
            }
        }

        // Update bullets
        let mut needs_upload = self.frame_count % 4 == 0;
        for b in self.bullets.iter_mut() {
            if !b.alive { continue; }
            b.x += b.dx; b.y += b.dy; b.z += b.dz;
            let (ix,iy,iz) = (b.x as usize, b.y as usize, b.z as usize);
            // Out of bounds
            if ix >= GRID || iy >= GRID || iz >= GRID { b.alive = false; continue; }
            // Hit a voxel?
            if self.grid.get(ix, iy, iz).is_solid() {
                // DESTROY sphere around impact
                self.grid.destroy(b.x, b.y, b.z, 3.0*s);
                b.alive = false;
                needs_upload = true;
            }
            // Draw bullet as bright tracer (small sphere for visibility)
            if b.alive {
                self.grid.fill_sphere(b.x, b.y, b.z, 2.0, Voxel::solid(1, 255, 220, 50));
                needs_upload = true;
            }
        }
        self.bullets.retain(|b| b.alive);

        if needs_upload { self.upload(); }
        let a=g.config.width as f32/g.config.height as f32;
        let inv=(self.cam.proj(a)*self.cam.view()).inverse();
        let e=self.cam.eye();
        g.queue.write_buffer(&g.ubuf,0,bytemuck::bytes_of(&Uniforms{inv_vp:inv.to_cols_array_2d(),eye:[e.x,e.y,e.z,0.0],info:[GRID as f32,self.time,0.0,0.0]}));
        let fr=match g.surface.get_current_texture(){Ok(f)=>f,Err(_)=>{g.surface.configure(&g.device,&g.config);return;}};
        let v=fr.texture.create_view(&Default::default());
        let mut enc=g.device.create_command_encoder(&Default::default());
        {let mut p=enc.begin_render_pass(&wgpu::RenderPassDescriptor{label:None,
            color_attachments:&[Some(wgpu::RenderPassColorAttachment{view:&v,resolve_target:None,
                ops:wgpu::Operations{load:wgpu::LoadOp::Clear(wgpu::Color{r:0.06,g:0.07,b:0.12,a:1.0}),store:wgpu::StoreOp::Store}})],
            depth_stencil_attachment:None,..Default::default()});
            p.set_pipeline(&g.pipeline); p.set_bind_group(0,&g.bg,&[]); p.draw(0..6,0..1);
        }
        g.queue.submit(std::iter::once(enc.finish())); fr.present();
        self.win.as_ref().unwrap().request_redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el:&winit::event_loop::ActiveEventLoop) {
        if self.win.is_some(){return;}
        let w=Arc::new(el.create_window(Window::default_attributes()
            .with_title("PROMETHEUS ENGINE — Phase 0: ORPP Soldier (128³)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280,720))).unwrap());
        self.init_gpu(w);
    }
    fn window_event(&mut self, el:&winit::event_loop::ActiveEventLoop, _:winit::window::WindowId, ev:WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(s) => { if let Some(g)=self.gpu.as_mut() { g.config.width=s.width.max(1); g.config.height=s.height.max(1); g.surface.configure(&g.device,&g.config); } }
            WindowEvent::KeyboardInput{event,..} if event.state.is_pressed() => match event.physical_key {
                PhysicalKey::Code(KeyCode::Space)=>{self.grid.destroy(128.0,120.0,128.0,32.0);self.upload();println!("  💥 DESTROY!");}
                PhysicalKey::Code(KeyCode::KeyR)=>{self.grid=Grid::new();self.grid.build_soldier();self.upload();println!("  🔄 Rebuilt.");}
                PhysicalKey::Code(KeyCode::KeyA)=>{self.rotate=!self.rotate;}
                PhysicalKey::Code(KeyCode::Escape)=>el.exit(),
                _=>{}
            }
            _=>{}
        }
    }
}

fn main() {
    env_logger::init();
    println!("\n  ═══════════════════════════════════════");
    println!("  🔥 PROMETHEUS ENGINE — Phase 0");
    println!("     128³ | Skeleton | Hollow Interior");
    println!("  ═══════════════════════════════════════\n");
    println!("  Space=destroy  R=rebuild  A=rotate  Esc=quit\n");
    let el=EventLoop::new().unwrap();
    let mut app=App::new();
    el.run_app(&mut app).unwrap();
}
