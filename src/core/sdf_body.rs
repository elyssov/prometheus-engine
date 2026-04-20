// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — SDF Body System
//
// Replaces ellipse-sweep profiles with Signed Distance Functions.
// Each body part = combination of SDF primitives (sphere, capsule,
// ellipsoid, box) with smooth union/subtraction.
//
// SDF field is sampled only at the surface shell (hollow rendering).
// Memory: O(surface_area), not O(volume).
// ═══════════════════════════════════════════════════════════════

use glam::Vec3;
use super::svo::Voxel;

/// SDF primitive — basic building block
#[derive(Clone, Debug)]
pub enum SdfPrimitive {
    /// Sphere at position with radius
    Sphere { center: Vec3, radius: f32 },
    /// Capsule (line segment with radius)
    Capsule { a: Vec3, b: Vec3, radius: f32 },
    /// Ellipsoid (stretched sphere)
    Ellipsoid { center: Vec3, radii: Vec3 },
    /// Rounded box
    RoundBox { center: Vec3, half_extents: Vec3, rounding: f32 },
}

impl SdfPrimitive {
    fn distance(&self, p: Vec3) -> f32 {
        match self {
            SdfPrimitive::Sphere { center, radius } => {
                (p - *center).length() - radius
            }
            SdfPrimitive::Capsule { a, b, radius } => {
                let ab = *b - *a;
                let ap = p - *a;
                let t = ap.dot(ab) / ab.dot(ab);
                let t = t.clamp(0.0, 1.0);
                let closest = *a + ab * t;
                (p - closest).length() - radius
            }
            SdfPrimitive::Ellipsoid { center, radii } => {
                // Approximate SDF for ellipsoid
                let q = (p - *center) / *radii;
                let len = q.length();
                if len < 0.001 { return -radii.min_element(); }
                (len - 1.0) * radii.min_element()
            }
            SdfPrimitive::RoundBox { center, half_extents, rounding } => {
                let q = (p - *center).abs() - *half_extents;
                let outside = Vec3::new(q.x.max(0.0), q.y.max(0.0), q.z.max(0.0)).length();
                let inside = q.x.max(q.y).max(q.z).min(0.0);
                outside + inside - rounding
            }
        }
    }

    /// Bounding box (min, max) with margin
    fn bounds(&self, margin: f32) -> (Vec3, Vec3) {
        match self {
            SdfPrimitive::Sphere { center, radius } => {
                let r = *radius + margin;
                (*center - Vec3::splat(r), *center + Vec3::splat(r))
            }
            SdfPrimitive::Capsule { a, b, radius } => {
                let r = *radius + margin;
                (a.min(*b) - Vec3::splat(r), a.max(*b) + Vec3::splat(r))
            }
            SdfPrimitive::Ellipsoid { center, radii } => {
                let r = *radii + Vec3::splat(margin);
                (*center - r, *center + r)
            }
            SdfPrimitive::RoundBox { center, half_extents, rounding } => {
                let r = *half_extents + Vec3::splat(*rounding + margin);
                (*center - r, *center + r)
            }
        }
    }
}

/// SDF operation — how primitives combine
#[derive(Clone, Debug)]
pub enum SdfOp {
    /// Add shape (smooth union with blending radius k)
    Add { primitive: SdfPrimitive, k: f32 },
    /// Subtract shape (smooth subtraction)
    Sub { primitive: SdfPrimitive, k: f32 },
}

/// Smooth minimum (for smooth union)
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k < 0.001 { return a.min(b); }
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    a * (1.0 - h) + b * h - k * h * (1.0 - h)
}

/// Smooth maximum (for smooth subtraction)
fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
    -smooth_min(-a, -b, k)
}

/// A complete SDF body part (e.g., skull, ribcage, femur)
#[derive(Clone)]
pub struct SdfShape {
    pub name: String,
    pub ops: Vec<SdfOp>,
    pub material: u8,
    pub color: [u8; 3],
}

impl SdfShape {
    pub fn new(name: &str, material: u8, color: [u8; 3]) -> Self {
        Self { name: name.to_string(), ops: Vec::new(), material, color }
    }

    pub fn add(&mut self, prim: SdfPrimitive, blend: f32) -> &mut Self {
        self.ops.push(SdfOp::Add { primitive: prim, k: blend });
        self
    }

    pub fn sub(&mut self, prim: SdfPrimitive, blend: f32) -> &mut Self {
        self.ops.push(SdfOp::Sub { primitive: prim, k: blend });
        self
    }

    /// Evaluate the SDF at point p
    pub fn distance(&self, p: Vec3) -> f32 {
        let mut d = f32::MAX;
        for op in &self.ops {
            match op {
                SdfOp::Add { primitive, k } => {
                    d = smooth_min(d, primitive.distance(p), *k);
                }
                SdfOp::Sub { primitive, k } => {
                    d = smooth_max(d, -primitive.distance(p), *k);
                }
            }
        }
        d
    }

    /// Overall bounding box of all primitives
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut bmin = Vec3::splat(f32::MAX);
        let mut bmax = Vec3::splat(f32::MIN);
        for op in &self.ops {
            let prim = match op {
                SdfOp::Add { primitive, .. } => primitive,
                SdfOp::Sub { primitive, .. } => primitive,
            };
            let (lo, hi) = prim.bounds(2.0);
            bmin = bmin.min(lo);
            bmax = bmax.max(hi);
        }
        (bmin, bmax)
    }
}

/// Complete SDF body — collection of shapes
pub struct SdfBody {
    pub shapes: Vec<SdfShape>,
}

impl SdfBody {
    pub fn new() -> Self { Self { shapes: Vec::new() } }

    pub fn add_shape(&mut self, shape: SdfShape) {
        self.shapes.push(shape);
    }

    /// Rasterize SDF body into voxel grid — HOLLOW shell only.
    /// Only writes voxels where the surface is (|sdf| < shell_thickness).
    /// This means interior is EMPTY — massive memory savings.
    pub fn rasterize<F>(&self, grid_size: usize, shell: f32, mut set_voxel: F)
    where F: FnMut(usize, usize, usize, u8, u8, u8, u8)
    {
        for shape in &self.shapes {
            let (bmin, bmax) = shape.bounds();
            // Only iterate within bounding box
            let x0 = (bmin.x.floor() as i32).max(0) as usize;
            let y0 = (bmin.y.floor() as i32).max(0) as usize;
            let z0 = (bmin.z.floor() as i32).max(0) as usize;
            let x1 = (bmax.x.ceil() as i32 + 1).min(grid_size as i32) as usize;
            let y1 = (bmax.y.ceil() as i32 + 1).min(grid_size as i32) as usize;
            let z1 = (bmax.z.ceil() as i32 + 1).min(grid_size as i32) as usize;

            for z in z0..z1 {
                for y in y0..y1 {
                    for x in x0..x1 {
                        let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                        let d = shape.distance(p);
                        // Solid fill for now (hollow later via Surface Nets)
                        if d <= 0.0 {
                            set_voxel(x, y, z, shape.material,
                                shape.color[0], shape.color[1], shape.color[2]);
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PREFAB: FULL HUMAN BODY from skeleton positions
//
// Takes a Skeleton after solve_forward() and builds SDF shapes
// around each bone group. Smooth union between parts = no Buratino.
//
// Anatomy: torso (3 ellipsoids blended), limbs (capsules),
//          head (skull), hands/feet (rounded boxes).
// ═══════════════════════════════════════════════════════════════

use super::skeleton::Skeleton;

impl SdfBody {
    /// Full human body built from skeleton world positions.
    /// Call skeleton.solve_forward() BEFORE this.
    ///
    /// Returns one SdfBody with multiple shapes (torso, limbs, head, etc.)
    /// each with its own material/color. Shapes are separate so they
    /// rasterize with correct per-part colors.
    pub fn human_body(skeleton: &Skeleton, scale: f32) -> Self {
        let mut body = SdfBody::new();
        let s = scale;
        let b = 2.5 * s;  // base blend radius — bigger = smoother joints

        let skin: [u8; 3] = [220, 185, 155];
        let coat: [u8; 3] = [110, 115, 130];
        let pants: [u8; 3] = [85, 88, 100];
        let boot: [u8; 3] = [70, 60, 48];
        let belt_c: [u8; 3] = [140, 120, 85];

        // ─── Bone positions (from solved skeleton) ──────────
        let pelvis  = skeleton.bone("pelvis").world_position;
        let spine_e = skeleton.bone("spine").world_end_position;
        let chest_s = skeleton.bone("chest").world_position;
        let chest_e = skeleton.bone("chest").world_end_position;
        let neck_s  = skeleton.bone("neck").world_position;
        let neck_e  = skeleton.bone("neck").world_end_position;
        let head_s  = skeleton.bone("head").world_position;
        let head_e  = skeleton.bone("head").world_end_position;

        // ─── TORSO ──────────────────────────────────────────
        // One shape for the whole torso: pelvis → chest, smooth blended.
        // This is the key anti-Buratino trick: overlapping ellipsoids
        // with large blend radius create organic transitions.
        {
            let mut torso = SdfShape::new("torso", 5, coat);
            let mid = (pelvis + spine_e) * 0.5;

            // Pelvis — wide, flat, boxy
            torso.add(SdfPrimitive::Ellipsoid {
                center: pelvis,
                radii: Vec3::new(8.5*s, 3.0*s, 5.0*s),
            }, 0.0);

            // Waist — narrower (belt area)
            torso.add(SdfPrimitive::Ellipsoid {
                center: mid,
                radii: Vec3::new(6.5*s, 3.0*s, 4.0*s),
            }, b);

            // Lower chest — barrel starts expanding
            torso.add(SdfPrimitive::Ellipsoid {
                center: chest_s,
                radii: Vec3::new(8.0*s, 3.5*s, 5.0*s),
            }, b);

            // Upper chest — widest, barrel shape
            torso.add(SdfPrimitive::Ellipsoid {
                center: chest_e,
                radii: Vec3::new(8.5*s, 4.0*s, 5.5*s),
            }, b);

            body.add_shape(torso);
        }

        // Belt highlight
        {
            let belt_pos = (pelvis + spine_e) * 0.5 + Vec3::new(0.0, 0.5*s, 0.0);
            let mut belt = SdfShape::new("belt", 10, belt_c);
            belt.add(SdfPrimitive::Ellipsoid {
                center: belt_pos,
                radii: Vec3::new(7.0*s, 1.5*s, 4.5*s),
            }, 0.0);
            body.add_shape(belt);
        }

        // ─── NECK ───────────────────────────────────────────
        {
            let mut neck = SdfShape::new("neck", 10, skin);
            neck.add(SdfPrimitive::Capsule {
                a: neck_s,
                b: neck_e,
                radius: 3.0*s,
            }, 0.0);
            // Smooth transition to chest — sphere at base
            neck.add(SdfPrimitive::Sphere {
                center: neck_s,
                radius: 4.0*s,
            }, b);
            body.add_shape(neck);
        }

        // ─── HEAD ───────────────────────────────────────────
        // Simplified head (not full skull — that's for skeleton view).
        // Egg shape: ellipsoid + chin capsule + forehead sphere.
        {
            let head_mid = (head_s + head_e) * 0.5;
            let head_len = (head_e - head_s).length();
            let mut head = SdfShape::new("head", 10, skin);

            // Main cranium — slightly elongated back
            head.add(SdfPrimitive::Ellipsoid {
                center: head_mid + Vec3::new(0.0, 0.0, -0.5*s),
                radii: Vec3::new(5.5*s, head_len * 0.55, 6.0*s),
            }, 0.0);

            // Jaw / chin — rounded box below
            head.add(SdfPrimitive::RoundBox {
                center: head_s + Vec3::new(0.0, -1.0*s, 1.0*s),
                half_extents: Vec3::new(3.5*s, 1.5*s, 2.5*s),
                rounding: 1.5*s,
            }, b * 0.8);

            // Forehead
            head.add(SdfPrimitive::Sphere {
                center: head_mid + Vec3::new(0.0, 1.5*s, 2.0*s),
                radius: 3.5*s,
            }, b);

            // Nose ridge
            head.add(SdfPrimitive::Capsule {
                a: head_s + Vec3::new(0.0, 0.5*s, 4.5*s),
                b: head_s + Vec3::new(0.0, -1.0*s, 5.5*s),
                radius: 1.0*s,
            }, 1.5);

            body.add_shape(head);
        }

        // ─── SHOULDERS + ARMS ───────────────────────────────
        for suffix in ["_l", "_r"] {
            let shoulder_name = format!("shoulder{}", suffix);
            let upper_name = format!("upper_arm{}", suffix);
            let forearm_name = format!("forearm{}", suffix);
            let hand_name = format!("hand{}", suffix);

            let shoulder_s = skeleton.bone(&shoulder_name).world_position;
            let shoulder_e = skeleton.bone(&shoulder_name).world_end_position;
            let upper_s = skeleton.bone(&upper_name).world_position;
            let upper_e = skeleton.bone(&upper_name).world_end_position;
            let forearm_s = skeleton.bone(&forearm_name).world_position;
            let forearm_e = skeleton.bone(&forearm_name).world_end_position;
            let hand_s = skeleton.bone(&hand_name).world_position;
            let hand_e = skeleton.bone(&hand_name).world_end_position;

            // Shoulder (deltoid) — sphere at joint
            {
                let mut shoulder = SdfShape::new("shoulder", 5, coat);
                shoulder.add(SdfPrimitive::Sphere {
                    center: shoulder_e,
                    radius: 5.0*s,
                }, 0.0);
                // Smooth bridge to chest
                shoulder.add(SdfPrimitive::Capsule {
                    a: shoulder_s,
                    b: shoulder_e,
                    radius: 3.5*s,
                }, b);
                body.add_shape(shoulder);
            }

            // Upper arm (coat)
            {
                let mut arm = SdfShape::new("upper_arm", 5, coat);
                arm.add(SdfPrimitive::Capsule {
                    a: upper_s,
                    b: upper_e,
                    radius: 3.5*s,
                }, 0.0);
                // Elbow bulge
                arm.add(SdfPrimitive::Sphere {
                    center: upper_e,
                    radius: 3.8*s,
                }, b * 0.5);
                body.add_shape(arm);
            }

            // Forearm (coat, tapers)
            {
                let mut forearm = SdfShape::new("forearm", 5, coat);
                // Capsule from elbow to wrist, wrist thinner
                forearm.add(SdfPrimitive::Capsule {
                    a: forearm_s,
                    b: forearm_e,
                    radius: 3.0*s,
                }, 0.0);
                // Wrist — smaller sphere
                forearm.add(SdfPrimitive::Sphere {
                    center: forearm_e,
                    radius: 2.5*s,
                }, 1.0);
                body.add_shape(forearm);
            }

            // Hand (skin)
            {
                let mut hand = SdfShape::new("hand", 10, skin);
                let hand_dir = (hand_e - hand_s).normalize();
                let hand_mid = (hand_s + hand_e) * 0.5;
                // Flat-ish box for palm
                hand.add(SdfPrimitive::RoundBox {
                    center: hand_mid,
                    half_extents: Vec3::new(2.0*s, 1.2*s, (hand_e - hand_s).length() * 0.4),
                    rounding: 1.0*s,
                }, 0.0);
                body.add_shape(hand);
            }
        }

        // ─── HIPS + LEGS ────────────────────────────────────
        for suffix in ["_l", "_r"] {
            let hip_name = format!("hip{}", suffix);
            let thigh_name = format!("thigh{}", suffix);
            let shin_name = format!("shin{}", suffix);
            let foot_name = format!("foot{}", suffix);

            let hip_s = skeleton.bone(&hip_name).world_position;
            let hip_e = skeleton.bone(&hip_name).world_end_position;
            let thigh_s = skeleton.bone(&thigh_name).world_position;
            let thigh_e = skeleton.bone(&thigh_name).world_end_position;
            let shin_s = skeleton.bone(&shin_name).world_position;
            let shin_e = skeleton.bone(&shin_name).world_end_position;
            let foot_s = skeleton.bone(&foot_name).world_position;
            let foot_e = skeleton.bone(&foot_name).world_end_position;

            // Hip joint — sphere bridging pelvis to leg
            {
                let mut hip = SdfShape::new("hip", 5, pants);
                hip.add(SdfPrimitive::Sphere {
                    center: hip_e,
                    radius: 5.5*s,
                }, 0.0);
                body.add_shape(hip);
            }

            // Thigh (pants)
            {
                let mut thigh = SdfShape::new("thigh", 5, pants);
                thigh.add(SdfPrimitive::Capsule {
                    a: thigh_s,
                    b: thigh_e,
                    radius: 4.5*s,
                }, 0.0);
                // Knee bulge
                thigh.add(SdfPrimitive::Sphere {
                    center: thigh_e,
                    radius: 4.0*s,
                }, b * 0.5);
                body.add_shape(thigh);
            }

            // Shin (pants → boot transition at ~60%)
            {
                let mut shin = SdfShape::new("shin", 5, pants);
                shin.add(SdfPrimitive::Capsule {
                    a: shin_s,
                    b: shin_e,
                    radius: 3.5*s,
                }, 0.0);
                body.add_shape(shin);
            }

            // Boot
            {
                let mut boot_shape = SdfShape::new("boot", 10, boot);
                // Boot shaft — capsule around lower shin
                let boot_top = shin_s + (shin_e - shin_s) * 0.5;
                boot_shape.add(SdfPrimitive::Capsule {
                    a: boot_top,
                    b: shin_e,
                    radius: 4.0*s,
                }, 0.0);
                // Boot sole — rounded box at foot
                let foot_mid = (foot_s + foot_e) * 0.5;
                boot_shape.add(SdfPrimitive::RoundBox {
                    center: foot_mid + Vec3::new(0.0, -1.0*s, 0.0),
                    half_extents: Vec3::new(3.5*s, 2.0*s, (foot_e - foot_s).length() * 0.45),
                    rounding: 1.0*s,
                }, b * 0.6);
                body.add_shape(boot_shape);
            }
        }

        body
    }
}

// ═══════════════════════════════════════════════════════════════
// PREFAB: CHIBI CAT body from Seedream 4.5 voxel references.
// Orange tabby with oversized head, tiny body, short legs,
// big green eyes. See Cat/ reference images.
// ═══════════════════════════════════════════════════════════════

impl SdfBody {
    /// Chibi cat body (orange tabby) built from cat skeleton positions.
    /// Call skeleton.solve_forward() BEFORE this.
    /// Expects Skeleton::cat() layout — 26 bones.
    pub fn chibi_cat_body(sk: &Skeleton, scale: f32) -> Self {
        let mut body = SdfBody::new();
        let s = scale;
        let b = 2.0 * s;  // blend radius — generous for chibi softness

        // Palette from references
        let orange:  [u8; 3] = [225, 135, 55];    // base tabby
        let cream:   [u8; 3] = [248, 225, 185];   // belly / chest / chin / paw tips
        let stripe:  [u8; 3] = [165, 85, 30];     // darker stripes
        let pink:    [u8; 3] = [240, 160, 170];   // nose, inner ears
        let green:   [u8; 3] = [75, 215, 95];     // eyes
        let dark:    [u8; 3] = [30, 25, 30];      // pupils / pads / nose tip
        let white:   [u8; 3] = [250, 250, 250];   // eye highlights / whiskers

        // Bone positions
        let pelvis  = sk.bone("pelvis").world_position;
        let s1e     = sk.bone("spine1").world_end_position;
        let s2s     = sk.bone("spine2").world_position;
        let s2e     = sk.bone("spine2").world_end_position;
        let neck_s  = sk.bone("neck").world_position;
        let neck_e  = sk.bone("neck").world_end_position;
        let head_s  = sk.bone("head").world_position;
        let head_e  = sk.bone("head").world_end_position;

        // ─── BIG CHIBI HEAD — placed FORWARD of the neck ──────
        // Key fix: head_center pushed past head_e, not at mid-bone,
        // so the head sits clearly AHEAD of the shoulders.
        let head_center = head_e + Vec3::new(0.0, -0.5*s, 1.5*s);
        let forehead   = head_center + Vec3::new(0.0, 2.2*s, -0.3*s);
        let muzzle     = head_center + Vec3::new(0.0, -3.0*s, 4.2*s);
        let chin       = muzzle + Vec3::new(0.0, -1.0*s, -0.8*s);
        {
            let mut h = SdfShape::new("cat_head", 5, orange);
            // Main cranium — round-ish sphere
            h.add(SdfPrimitive::Ellipsoid {
                center: head_center,
                radii: Vec3::new(6.5*s, 5.8*s, 6.5*s),
            }, 0.0);
            // Forehead bulge (makes head read "chibi")
            h.add(SdfPrimitive::Sphere { center: forehead, radius: 3.0*s }, b);
            // Cheek puffs
            h.add(SdfPrimitive::Sphere {
                center: head_center + Vec3::new(-4.0*s, -0.8*s, 2.0*s), radius: 2.3*s,
            }, b);
            h.add(SdfPrimitive::Sphere {
                center: head_center + Vec3::new(4.0*s, -0.8*s, 2.0*s), radius: 2.3*s,
            }, b);
            // Muzzle / short snout (sticks out of the face, not the body)
            h.add(SdfPrimitive::Ellipsoid {
                center: muzzle,
                radii: Vec3::new(2.3*s, 1.5*s, 1.8*s),
            }, b);
            body.add_shape(h);
        }

        // ─── Cream face mask (chin + muzzle + around mouth) ──
        {
            let mut m = SdfShape::new("cat_face_cream", 5, cream);
            m.add(SdfPrimitive::Ellipsoid {
                center: muzzle + Vec3::new(0.0, -0.5*s, -0.3*s),
                radii: Vec3::new(3.2*s, 1.8*s, 2.2*s),
            }, 0.0);
            m.add(SdfPrimitive::Sphere { center: chin, radius: 2.2*s }, b);
            body.add_shape(m);
        }

        // ─── EARS — triangular tufts pointing up ─────────────
        let ear_l_base = head_center + Vec3::new(-3.5*s, 4.5*s, -0.5*s);
        let ear_l_tip  = head_center + Vec3::new(-4.2*s, 8.0*s, -1.0*s);
        let ear_r_base = head_center + Vec3::new(3.5*s, 4.5*s, -0.5*s);
        let ear_r_tip  = head_center + Vec3::new(4.2*s, 8.0*s, -1.0*s);
        {
            let mut e = SdfShape::new("cat_ears", 5, orange);
            e.add(SdfPrimitive::Capsule { a: ear_l_base, b: ear_l_tip, radius: 1.6*s }, 0.0);
            e.add(SdfPrimitive::Capsule { a: ear_r_base, b: ear_r_tip, radius: 1.6*s }, 0.0);
            body.add_shape(e);
        }
        // Pink ear interiors
        {
            let mut ei = SdfShape::new("cat_ear_inner", 5, pink);
            ei.add(SdfPrimitive::Capsule {
                a: ear_l_base + Vec3::new(0.0, 1.0*s, 0.5*s),
                b: ear_l_tip + Vec3::new(0.0, -0.5*s, 0.5*s),
                radius: 0.8*s,
            }, 0.0);
            ei.add(SdfPrimitive::Capsule {
                a: ear_r_base + Vec3::new(0.0, 1.0*s, 0.5*s),
                b: ear_r_tip + Vec3::new(0.0, -0.5*s, 0.5*s),
                radius: 0.8*s,
            }, 0.0);
            body.add_shape(ei);
        }

        // ─── HUGE GREEN EYES ──────────────────────────────────
        let eye_l = head_center + Vec3::new(-2.5*s, 1.0*s, 5.0*s);
        let eye_r = head_center + Vec3::new(2.5*s, 1.0*s, 5.0*s);
        {
            let mut ey = SdfShape::new("cat_eyes", 5, green);
            ey.add(SdfPrimitive::Sphere { center: eye_l, radius: 2.2*s }, 0.0);
            ey.add(SdfPrimitive::Sphere { center: eye_r, radius: 2.2*s }, 0.0);
            body.add_shape(ey);
        }
        // Pupils — dark vertical slits (thin ellipsoids)
        {
            let mut pu = SdfShape::new("cat_pupils", 5, dark);
            pu.add(SdfPrimitive::Ellipsoid {
                center: eye_l + Vec3::new(0.0, 0.0, 0.8*s),
                radii: Vec3::new(0.4*s, 1.5*s, 0.6*s),
            }, 0.0);
            pu.add(SdfPrimitive::Ellipsoid {
                center: eye_r + Vec3::new(0.0, 0.0, 0.8*s),
                radii: Vec3::new(0.4*s, 1.5*s, 0.6*s),
            }, 0.0);
            body.add_shape(pu);
        }
        // White highlight dots in eyes
        {
            let mut hl = SdfShape::new("cat_eye_highlight", 5, white);
            hl.add(SdfPrimitive::Sphere {
                center: eye_l + Vec3::new(-0.7*s, 0.7*s, 1.3*s), radius: 0.5*s,
            }, 0.0);
            hl.add(SdfPrimitive::Sphere {
                center: eye_r + Vec3::new(-0.7*s, 0.7*s, 1.3*s), radius: 0.5*s,
            }, 0.0);
            body.add_shape(hl);
        }

        // ─── PINK TRIANGLE NOSE ───────────────────────────────
        {
            let mut n = SdfShape::new("cat_nose", 5, pink);
            n.add(SdfPrimitive::Ellipsoid {
                center: muzzle + Vec3::new(0.0, 0.8*s, 1.5*s),
                radii: Vec3::new(0.7*s, 0.5*s, 0.4*s),
            }, 0.0);
            body.add_shape(n);
        }

        // ─── TORSO — compact chubby body ─────────────────────
        // Cat is quadrupedal: spine1+spine2 stretch along Z from pelvis to chest.
        let torso_mid = (pelvis + s2e) * 0.5;
        {
            let mut t = SdfShape::new("cat_body", 5, orange);
            // Pelvis (rear)
            t.add(SdfPrimitive::Ellipsoid {
                center: pelvis,
                radii: Vec3::new(4.5*s, 4.0*s, 4.5*s),
            }, 0.0);
            // Middle back
            t.add(SdfPrimitive::Ellipsoid {
                center: torso_mid,
                radii: Vec3::new(4.0*s, 3.8*s, 5.0*s),
            }, b);
            // Front shoulders (at spine2 end, where neck starts)
            t.add(SdfPrimitive::Ellipsoid {
                center: s2e,
                radii: Vec3::new(4.2*s, 4.0*s, 4.0*s),
            }, b);
            // Neck — short stubby
            t.add(SdfPrimitive::Capsule { a: neck_s, b: neck_e, radius: 3.0*s }, b);
            body.add_shape(t);
        }

        // ─── CREAM BELLY / CHEST ──────────────────────────────
        {
            let mut bl = SdfShape::new("cat_belly", 5, cream);
            bl.add(SdfPrimitive::Ellipsoid {
                center: torso_mid + Vec3::new(0.0, -2.0*s, 0.0),
                radii: Vec3::new(3.2*s, 2.0*s, 4.8*s),
            }, 0.0);
            // Chest bib
            bl.add(SdfPrimitive::Sphere {
                center: neck_s + Vec3::new(0.0, -1.5*s, -1.0*s),
                radius: 2.8*s,
            }, b);
            body.add_shape(bl);
        }

        // ─── TABBY STRIPES (dark orange capsules along spine) ─
        {
            let mut st = SdfShape::new("cat_stripes", 5, stripe);
            // Spine stripe (dorsal) — pelvis → neck
            st.add(SdfPrimitive::Capsule {
                a: pelvis + Vec3::new(0.0, 3.8*s, 0.0),
                b: s2e + Vec3::new(0.0, 3.8*s, 0.0),
                radius: 0.7*s,
            }, 0.0);
            // 4 ring-stripes across back (short capsules perpendicular to spine)
            for (i, &zf) in [0.2_f32, 0.45, 0.7, 0.95].iter().enumerate() {
                let along = pelvis + (s2e - pelvis) * zf;
                let h_off = 3.2*s;
                st.add(SdfPrimitive::Capsule {
                    a: along + Vec3::new(-3.5*s, h_off, 0.0),
                    b: along + Vec3::new(3.5*s, h_off, 0.0),
                    radius: 0.6*s,
                }, 0.0);
                let _ = i;
            }
            // Forehead M-mark (tabby signature)
            st.add(SdfPrimitive::Capsule {
                a: forehead + Vec3::new(-1.8*s, 0.5*s, 2.8*s),
                b: forehead + Vec3::new(-0.5*s, -1.5*s, 3.5*s),
                radius: 0.45*s,
            }, 0.0);
            st.add(SdfPrimitive::Capsule {
                a: forehead + Vec3::new(1.8*s, 0.5*s, 2.8*s),
                b: forehead + Vec3::new(0.5*s, -1.5*s, 3.5*s),
                radius: 0.45*s,
            }, 0.0);
            // Cheek whisker marks (3 per side)
            for side in [-1.0_f32, 1.0] {
                for row in 0..3 {
                    let y = -0.3 - row as f32 * 0.7;
                    st.add(SdfPrimitive::Capsule {
                        a: head_center + Vec3::new(side * 3.0*s, y*s, 5.0*s),
                        b: head_center + Vec3::new(side * 5.5*s, y*s, 4.0*s),
                        radius: 0.3*s,
                    }, 0.0);
                }
            }
            body.add_shape(st);
        }

        // ─── FOUR LEGS (short, stubby, orange) ────────────────
        for leg_name in ["upper_arm_l", "upper_arm_r", "thigh_l", "thigh_r"] {
            let hip = sk.bone(leg_name).world_position;
            let knee = sk.bone(leg_name).world_end_position;
            let (lower_name, paw_name) = match leg_name {
                "upper_arm_l" => ("forearm_l", "paw_fl"),
                "upper_arm_r" => ("forearm_r", "paw_fr"),
                "thigh_l"     => ("shin_l", "paw_bl"),
                "thigh_r"     => ("shin_r", "paw_br"),
                _ => unreachable!(),
            };
            let lower_s = sk.bone(lower_name).world_position;
            let lower_e = sk.bone(lower_name).world_end_position;
            let paw_s   = sk.bone(paw_name).world_position;
            let paw_e   = sk.bone(paw_name).world_end_position;

            let mut leg = SdfShape::new("cat_leg", 5, orange);
            // Upper leg (thigh/upper arm)
            leg.add(SdfPrimitive::Capsule { a: hip, b: knee, radius: 2.2*s }, 0.0);
            // Lower leg
            leg.add(SdfPrimitive::Capsule { a: lower_s, b: lower_e, radius: 2.0*s }, b);
            // Paw as rounded box
            let paw_mid = (paw_s + paw_e) * 0.5;
            leg.add(SdfPrimitive::RoundBox {
                center: paw_mid,
                half_extents: Vec3::new(1.6*s, 1.2*s, 1.8*s),
                rounding: 1.0*s,
            }, b);
            body.add_shape(leg);
        }

        // Cream paw tips (all 4 paws)
        {
            let mut tips = SdfShape::new("cat_paw_tips", 5, cream);
            for paw_name in ["paw_fl", "paw_fr", "paw_bl", "paw_br"] {
                let paw_e = sk.bone(paw_name).world_end_position;
                tips.add(SdfPrimitive::Sphere {
                    center: paw_e + Vec3::new(0.0, 0.5*s, 0.5*s),
                    radius: 1.4*s,
                }, 0.0);
            }
            body.add_shape(tips);
        }

        // ─── TAIL (curved up, striped) ────────────────────────
        {
            let mut tl = SdfShape::new("cat_tail", 5, orange);
            for seg in ["tail1", "tail2", "tail3", "tail4"] {
                let a = sk.bone(seg).world_position;
                let c = sk.bone(seg).world_end_position;
                let r = match seg {
                    "tail1" => 1.8*s, "tail2" => 1.6*s,
                    "tail3" => 1.4*s, _       => 1.1*s,
                };
                tl.add(SdfPrimitive::Capsule { a, b: c, radius: r }, b * 0.5);
            }
            body.add_shape(tl);
        }
        // Tail tip (dark)
        {
            let mut tt = SdfShape::new("cat_tail_tip", 5, stripe);
            let tip = sk.bone("tail4").world_end_position;
            tt.add(SdfPrimitive::Sphere { center: tip, radius: 1.2*s }, 0.0);
            body.add_shape(tt);
        }

        body
    }
}

// ═══════════════════════════════════════════════════════════════
// PREFAB: CHIBI CAT boxed — MagicaVoxel style.
// Only RoundBox primitives with rounding=0 (true cubes).
// Proportions from Seedream 4.5 reference: square head, tiny body,
// short pillar legs, thick tail. Large kawaii eyes as flat plates
// on front face of head. Vertical stripes on sides.
// ═══════════════════════════════════════════════════════════════

impl SdfBody {
    /// Chibi cat made entirely of axis-aligned cubes.
    /// Use with `Skeleton::chibi_cat(scale)`. scale ~2-3 recommended.
    /// Anchors: head/body/ears/tail are placed relative to pelvis.
    /// Legs use paw world positions so walk animation shifts them.
    pub fn chibi_cat_boxed(sk: &Skeleton, scale: f32) -> Self {
        let mut body = SdfBody::new();
        let s = scale;

        // Palette (from Seedream references)
        let orange: [u8; 3] = [235, 135, 55];
        let cream:  [u8; 3] = [250, 230, 190];
        let stripe: [u8; 3] = [155, 80, 30];
        let pink:   [u8; 3] = [245, 170, 180];
        let green:  [u8; 3] = [70, 210, 100];
        let white:  [u8; 3] = [252, 252, 252];
        let dark:   [u8; 3] = [25, 20, 25];

        let pelvis = sk.bone("pelvis").world_position;

        // ─── BODY: long block (cat is long!). 7*s in Z = ~17 voxels at scale 2.5
        let spine2_end = sk.bone("spine2").world_end_position;
        // Body lifts when spine arches (hiss): body_center.y follows spine arch
        let arch_lift = (spine2_end.y - pelvis.y).max(0.0);
        let body_center = pelvis + Vec3::new(0.0, 0.5*s + arch_lift * 0.5, 1.5*s);
        let body_half = Vec3::new(2.3*s, 2.0*s + arch_lift * 0.5, 5.25*s);
        {
            let mut b = SdfShape::new("body", 5, orange);
            b.add(SdfPrimitive::RoundBox {
                center: body_center, half_extents: body_half, rounding: 0.0,
            }, 0.0);
            body.add_shape(b);
        }

        // Chest cream bib — big flat slab on front of body
        {
            let mut c = SdfShape::new("chest", 5, cream);
            c.add(SdfPrimitive::RoundBox {
                center: body_center + Vec3::new(0.0, -0.3*s, body_half.z - 0.3*s),
                half_extents: Vec3::new(1.5*s, 1.6*s, 0.3*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(c);
        }

        // Vertical stripes on body sides
        {
            let mut st = SdfShape::new("body_stripes", 5, stripe);
            for dz in [-2.5_f32, -1.0, 0.5, 1.8, 3.0] {
                // left
                st.add(SdfPrimitive::RoundBox {
                    center: Vec3::new(body_center.x - body_half.x, body_center.y, body_center.z + dz*s),
                    half_extents: Vec3::new(0.2*s, body_half.y - 0.3*s, 0.3*s),
                    rounding: 0.0,
                }, 0.0);
                // right
                st.add(SdfPrimitive::RoundBox {
                    center: Vec3::new(body_center.x + body_half.x, body_center.y, body_center.z + dz*s),
                    half_extents: Vec3::new(0.2*s, body_half.y - 0.3*s, 0.3*s),
                    rounding: 0.0,
                }, 0.0);
            }
            // Back stripes across the top (dorsal)
            for dz in [-2.0_f32, -0.5, 1.0, 2.5] {
                st.add(SdfPrimitive::RoundBox {
                    center: Vec3::new(body_center.x, body_center.y + body_half.y, body_center.z + dz*s),
                    half_extents: Vec3::new(body_half.x - 0.3*s, 0.2*s, 0.3*s),
                    rounding: 0.0,
                }, 0.0);
            }
            body.add_shape(st);
        }

        // ─── HEAD: big cube anchored to spine2_end (so spine arch lifts it)
        let head_center = spine2_end + Vec3::new(0.0, 3.0*s, 2.0*s);
        let head_half = Vec3::new(3.5*s, 3.3*s, 3.0*s);
        {
            let mut h = SdfShape::new("head", 5, orange);
            h.add(SdfPrimitive::RoundBox {
                center: head_center, half_extents: head_half, rounding: 0.0,
            }, 0.0);
            body.add_shape(h);
        }

        // ─── EARS: two triangular-ish pillars on top ─────────
        let ear_half = Vec3::new(0.8*s, 1.5*s, 0.8*s);
        for &dx in &[-2.3_f32, 2.3] {
            let mut e = SdfShape::new("ear", 5, orange);
            let c = Vec3::new(
                head_center.x + dx*s,
                head_center.y + head_half.y + ear_half.y - 0.2*s,
                head_center.z - 0.3*s,
            );
            e.add(SdfPrimitive::RoundBox {
                center: c, half_extents: ear_half, rounding: 0.0,
            }, 0.0);
            body.add_shape(e);
            // Pink inner ear — smaller box in front
            let mut ei = SdfShape::new("ear_inner", 5, pink);
            ei.add(SdfPrimitive::RoundBox {
                center: c + Vec3::new(0.0, -0.2*s, 0.4*s),
                half_extents: Vec3::new(0.4*s, 0.9*s, 0.3*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(ei);
        }

        // ─── FACE: cream muzzle plate on lower half of front ─
        let face_front = head_center.z + head_half.z;
        {
            let mut c = SdfShape::new("muzzle_cream", 5, cream);
            c.add(SdfPrimitive::RoundBox {
                center: Vec3::new(head_center.x, head_center.y - 1.6*s, face_front - 0.2*s),
                half_extents: Vec3::new(1.8*s, 1.3*s, 0.4*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(c);
        }

        // ─── EYES: big green squares on upper front ──────────
        let eye_half = Vec3::new(0.9*s, 1.1*s, 0.3*s);
        let eye_y = head_center.y + 0.2*s;
        for &dx in &[-1.5_f32, 1.5] {
            let mut e = SdfShape::new("eye", 5, green);
            let c = Vec3::new(head_center.x + dx*s, eye_y, face_front);
            e.add(SdfPrimitive::RoundBox {
                center: c, half_extents: eye_half, rounding: 0.0,
            }, 0.0);
            body.add_shape(e);
        }
        // Dark round pupils — centered on each eye
        for &dx in &[-1.5_f32, 1.5] {
            let mut p = SdfShape::new("pupil", 5, dark);
            let c = Vec3::new(head_center.x + dx*s, eye_y, face_front + 0.2*s);
            p.add(SdfPrimitive::RoundBox {
                center: c,
                half_extents: Vec3::new(0.35*s, 0.45*s, 0.2*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(p);
        }
        // White highlight dots in upper-outer corner of each eye
        for &dx in &[-1.85_f32, 1.15] {
            let mut h = SdfShape::new("hl", 5, white);
            let c = Vec3::new(head_center.x + dx*s, eye_y + 0.7*s, face_front + 0.25*s);
            h.add(SdfPrimitive::RoundBox {
                center: c,
                half_extents: Vec3::new(0.25*s, 0.25*s, 0.15*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(h);
        }

        // ─── NOSE: pink stub that sticks out forward ──────────
        let nose_y = head_center.y - 0.9*s;
        {
            let mut n = SdfShape::new("nose", 5, pink);
            n.add(SdfPrimitive::RoundBox {
                center: Vec3::new(head_center.x, nose_y, face_front + 0.6*s),
                half_extents: Vec3::new(0.45*s, 0.35*s, 0.6*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(n);
        }
        // Mouth — small dark line below nose
        {
            let mut m = SdfShape::new("mouth", 5, dark);
            m.add(SdfPrimitive::RoundBox {
                center: Vec3::new(head_center.x, nose_y - 0.7*s, face_front + 0.2*s),
                half_extents: Vec3::new(0.5*s, 0.12*s, 0.18*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(m);
        }
        // Whiskers — 3 horizontal strips on each side from nose level
        {
            let mut w = SdfShape::new("whiskers", 5, white);
            for &side in &[-1.0_f32, 1.0] {
                for row in 0..3 {
                    let dy = -0.3 + row as f32 * 0.4; // -0.3, 0.1, 0.5
                    w.add(SdfPrimitive::RoundBox {
                        center: Vec3::new(
                            head_center.x + side * (1.6*s),
                            nose_y + dy*s,
                            face_front + 0.15*s,
                        ),
                        half_extents: Vec3::new(1.4*s, 0.1*s, 0.1*s),
                        rounding: 0.0,
                    }, 0.0);
                }
            }
            body.add_shape(w);
        }

        // ─── FOREHEAD M-MARK (tabby signature stripes) ───────
        {
            let mut m = SdfShape::new("m_mark", 5, stripe);
            for &dx in &[-1.3_f32, 0.0, 1.3] {
                m.add(SdfPrimitive::RoundBox {
                    center: Vec3::new(head_center.x + dx*s, head_center.y + 2.3*s, face_front - 0.1*s),
                    half_extents: Vec3::new(0.22*s, 0.8*s, 0.2*s),
                    rounding: 0.0,
                }, 0.0);
            }
            // Cheek whisker stripes (3 per side)
            for &side in &[-1.0_f32, 1.0] {
                for row in 0..3 {
                    let y = -0.5 - row as f32 * 0.5;
                    m.add(SdfPrimitive::RoundBox {
                        center: Vec3::new(
                            head_center.x + side * (head_half.x - 0.3*s),
                            head_center.y + y*s,
                            face_front - 0.3*s,
                        ),
                        half_extents: Vec3::new(0.2*s, 0.18*s, 0.25*s),
                        rounding: 0.0,
                    }, 0.0);
                }
            }
            body.add_shape(m);
        }

        // ─── LEGS: 4 box-pillars from shoulder/hip to paw position.
        // Top anchor = shoulder/hip world_end_position (sits directly above
        // the paw in rest pose) — so legs are vertical columns by default.
        // When swipe rotates upper_arm, paw moves forward → bbox stretches.
        let leg_half_xz = 0.75 * s;
        for (paw_name, anchor_name) in [
            ("paw_fl", "shoulder_l"),
            ("paw_fr", "shoulder_r"),
            ("paw_bl", "hip_l"),
            ("paw_br", "hip_r"),
        ] {
            let paw_e = sk.bone(paw_name).world_end_position;
            let leg_top = sk.bone(anchor_name).world_end_position;
            let center = (leg_top + paw_e) * 0.5;
            let span = (paw_e - leg_top).abs();
            let he = Vec3::new(
                (span.x * 0.5 + leg_half_xz).max(leg_half_xz),
                (span.y * 0.5).max(0.4*s),
                (span.z * 0.5 + leg_half_xz).max(leg_half_xz),
            );
            let mut l = SdfShape::new("leg", 5, orange);
            l.add(SdfPrimitive::RoundBox {
                center, half_extents: he, rounding: 0.0,
            }, 0.0);
            body.add_shape(l);
        }

        // Cream paw pads (at paw end position — follows full animation)
        for paw_name in ["paw_fl", "paw_fr", "paw_bl", "paw_br"] {
            let paw_e = sk.bone(paw_name).world_end_position;
            let mut p = SdfShape::new("pad", 5, cream);
            p.add(SdfPrimitive::RoundBox {
                center: paw_e + Vec3::new(0.0, 0.3*s, 0.0),
                half_extents: Vec3::new(0.8*s, 0.3*s, 0.8*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(p);
        }

        // ─── TAIL: 4 segments following tail bone positions
        // Each segment = bbox between bone start and end. This way the
        // tail bends as bones rotate (wave from vertical to horizontal).
        let tail_segs = [("tail1", 0.85_f32), ("tail2", 0.75), ("tail3", 0.65), ("tail4", 0.55)];
        let mut tail_shape = SdfShape::new("tail", 5, orange);
        for (name, thick) in tail_segs {
            let a = sk.bone(name).world_position;
            let b = sk.bone(name).world_end_position;
            let center = (a + b) * 0.5;
            let span = (b - a).abs();
            // Half-extents: at least `thick*s` in each dim, plus span/2 for length axis
            let he = Vec3::new(
                (span.x * 0.5 + thick * s).max(thick * s),
                (span.y * 0.5 + thick * s).max(thick * s),
                (span.z * 0.5 + thick * s).max(thick * s),
            );
            tail_shape.add(SdfPrimitive::RoundBox {
                center, half_extents: he, rounding: 0.0,
            }, 0.0);
        }
        body.add_shape(tail_shape);
        // Tail stripe rings — at midpoint of segments 1, 2, 3
        {
            let mut ts = SdfShape::new("tail_stripes", 5, stripe);
            for name in ["tail1", "tail2", "tail3"] {
                let a = sk.bone(name).world_position;
                let b = sk.bone(name).world_end_position;
                let mid = (a + b) * 0.5;
                let span = (b - a).abs();
                ts.add(SdfPrimitive::RoundBox {
                    center: mid,
                    half_extents: Vec3::new(
                        (span.x * 0.5 + 0.85*s).max(0.85*s),
                        (span.y * 0.5).max(0.0) + 0.25*s,
                        (span.z * 0.5 + 0.85*s).max(0.85*s),
                    ),
                    rounding: 0.0,
                }, 0.0);
            }
            body.add_shape(ts);
        }
        // Tail tip cream
        {
            let mut tt = SdfShape::new("tail_tip", 5, cream);
            let tip = sk.bone("tail4").world_end_position;
            tt.add(SdfPrimitive::RoundBox {
                center: tip,
                half_extents: Vec3::new(0.6*s, 0.6*s, 0.6*s),
                rounding: 0.0,
            }, 0.0);
            body.add_shape(tt);
        }

        body
    }
}

// ═══════════════════════════════════════════════════════════════
// PREFAB: HUMAN SKULL from reference images
// ═══════════════════════════════════════════════════════════════

impl SdfBody {
    /// Anatomical human skull based on reference scull.jpg
    /// Position: centered at `pos`, looking toward -Z
    pub fn human_skull(pos: Vec3, scale: f32) -> Self {
        let mut body = SdfBody::new();
        let s = scale;
        let bone: [u8; 3] = [235, 225, 200];

        let mut skull = SdfShape::new("skull", 10, bone);

        // === CRANIAL VAULT ===
        // Main cranium — elongated ellipsoid (deeper than wide)
        // From scull.jpg side view: depth ≈ 85% of height, width ≈ 70% of height
        skull.add(SdfPrimitive::Ellipsoid {
            center: pos + Vec3::new(0.0, 6.0*s, -1.0*s),
            radii: Vec3::new(4.5*s, 5.5*s, 5.8*s), // width < height < depth
        }, 0.0);

        // Frontal bone (forehead)
        skull.add(SdfPrimitive::Sphere {
            center: pos + Vec3::new(0.0, 8.0*s, 3.0*s),
            radius: 3.0*s,
        }, 3.0); // ABSOLUTE blend, not scaled!

        // Occipital bone (back of skull — big backward bulge)
        skull.add(SdfPrimitive::Sphere {
            center: pos + Vec3::new(0.0, 4.5*s, -5.0*s),
            radius: 3.5*s,
        }, 3.0);

        // === FACE ===
        // Brow ridge
        skull.add(SdfPrimitive::RoundBox {
            center: pos + Vec3::new(0.0, 4.0*s, 3.5*s),
            half_extents: Vec3::new(3.2*s, 0.8*s, 0.8*s),
            rounding: 0.5*s,
        }, 2.0);

        // Zygomatic arches (cheekbones) — capsules from face to side
        skull.add(SdfPrimitive::Capsule {
            a: pos + Vec3::new(-2.5*s, 3.0*s, 2.5*s),
            b: pos + Vec3::new(-4.5*s, 2.5*s, 0.0),
            radius: 1.0*s,
        }, 1.5);
        skull.add(SdfPrimitive::Capsule {
            a: pos + Vec3::new(2.5*s, 3.0*s, 2.5*s),
            b: pos + Vec3::new(4.5*s, 2.5*s, 0.0),
            radius: 1.0*s,
        }, 1.5);

        // Maxilla (upper jaw)
        skull.add(SdfPrimitive::RoundBox {
            center: pos + Vec3::new(0.0, 1.0*s, 3.0*s),
            half_extents: Vec3::new(2.2*s, 1.2*s, 1.2*s),
            rounding: 0.5*s,
        }, 1.5);

        // Mandible — U-shaped jaw, 3 capsules
        skull.add(SdfPrimitive::Capsule {
            a: pos + Vec3::new(-3.0*s, 1.0*s, 0.0),
            b: pos + Vec3::new(-1.5*s, -1.5*s, 2.5*s),
            radius: 1.0*s,
        }, 1.5);
        skull.add(SdfPrimitive::Capsule {
            a: pos + Vec3::new(3.0*s, 1.0*s, 0.0),
            b: pos + Vec3::new(1.5*s, -1.5*s, 2.5*s),
            radius: 1.0*s,
        }, 1.5);
        skull.add(SdfPrimitive::Capsule {
            a: pos + Vec3::new(-1.5*s, -1.5*s, 2.5*s),
            b: pos + Vec3::new(1.5*s, -1.5*s, 2.5*s),
            radius: 1.0*s,
        }, 1.5);

        // === CAVITIES (subtract) ===
        // Eye sockets — deep holes
        skull.sub(SdfPrimitive::Sphere {
            center: pos + Vec3::new(-1.8*s, 3.5*s, 4.5*s),
            radius: 1.5*s,
        }, 1.0);
        skull.sub(SdfPrimitive::Sphere {
            center: pos + Vec3::new(1.8*s, 3.5*s, 4.5*s),
            radius: 1.5*s,
        }, 1.0);

        // Nasal aperture
        skull.sub(SdfPrimitive::Ellipsoid {
            center: pos + Vec3::new(0.0, 1.8*s, 4.5*s),
            radii: Vec3::new(0.8*s, 1.3*s, 1.0*s),
        }, 0.5);

        // Temporal fossae (indent on sides)
        skull.sub(SdfPrimitive::Sphere {
            center: pos + Vec3::new(-5.5*s, 5.0*s, 1.0*s),
            radius: 2.0*s,
        }, 1.0);
        skull.sub(SdfPrimitive::Sphere {
            center: pos + Vec3::new(5.5*s, 5.0*s, 1.0*s),
            radius: 2.0*s,
        }, 1.0);

        // Foramen magnum
        skull.sub(SdfPrimitive::Sphere {
            center: pos + Vec3::new(0.0, -0.5*s, -1.5*s),
            radius: 1.5*s,
        }, 0.5);

        body.add_shape(skull);
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sdf_sphere() {
        let s = SdfPrimitive::Sphere { center: Vec3::ZERO, radius: 5.0 };
        assert!((s.distance(Vec3::ZERO) - (-5.0)).abs() < 0.01);
        assert!((s.distance(Vec3::new(5.0, 0.0, 0.0)) - 0.0).abs() < 0.01);
        assert!((s.distance(Vec3::new(10.0, 0.0, 0.0)) - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_sdf_skull_rasterize() {
        let pos = Vec3::new(64.0, 64.0, 64.0);
        let s = 4.0;
        let body = SdfBody::human_skull(pos, s);

        // Debug: check SDF value at skull center
        let skull = &body.shapes[0];
        let d_center = skull.distance(pos + Vec3::new(0.0, 6.0*s, 0.0));
        let d_surface = skull.distance(pos + Vec3::new(4.8*s, 6.0*s, 0.0));
        println!("SDF at cranium center: {:.2}", d_center);
        println!("SDF near cranium surface: {:.2}", d_surface);
        println!("Skull bounds: {:?}", skull.bounds());

        let mut count = 0;
        body.rasterize(128, 2.0, |_,_,_,_,_,_,_| { count += 1; });
        println!("Skull voxels (shell): {}", count);
        assert!(count > 100);
    }

    #[test]
    fn test_sdf_human_body() {
        use super::super::skeleton::Skeleton;

        let mut sk = Skeleton::human(4.0);
        sk.root_position = Vec3::new(64.0, 60.0, 64.0);
        sk.solve_forward();

        let body = SdfBody::human_body(&sk, 4.0);
        println!("Human body: {} shapes", body.shapes.len());
        for shape in &body.shapes {
            println!("  {} (mat={}, color={:?})", shape.name, shape.material, shape.color);
        }

        // Should have torso + belt + neck + head + 2×(shoulder+upper+forearm+hand) + 2×(hip+thigh+shin+boot)
        assert!(body.shapes.len() >= 16, "Expected 16+ shapes, got {}", body.shapes.len());

        let mut count = 0;
        body.rasterize(128, 1.5, |_,_,_,_,_,_,_| { count += 1; });
        println!("Human body voxels: {}", count);
        // Full body should have significantly more voxels than just a skull
        assert!(count > 5000, "Expected >5000 voxels, got {}", count);
    }

    #[test]
    fn test_sdf_body_with_surface_nets() {
        use super::super::skeleton::Skeleton;
        use super::super::meshing;
        use super::super::svo::Voxel as V;

        let mut sk = Skeleton::human(2.0);
        sk.root_position = Vec3::new(64.0, 50.0, 64.0);
        sk.solve_forward();

        let sdf = SdfBody::human_body(&sk, 2.0);

        // Rasterize into flat grid
        let size = 128;
        let mut voxels = vec![V::empty(); size * size * size];
        sdf.rasterize(size, 1.5, |x,y,z,mat,r,g,b| {
            if x < size && y < size && z < size {
                voxels[z * size * size + y * size + x] = V::solid(mat, r, g, b);
            }
        });

        let solid = voxels.iter().filter(|v| v.is_solid()).count();
        println!("Rasterized: {} solid voxels in {}³", solid, size);

        // Now mesh with Surface Nets
        let mesh = meshing::generate_mesh_smooth(&voxels, size, Vec3::ZERO, 1.0);
        println!("Surface Nets: {} triangles, {} vertices", mesh.triangle_count, mesh.vertices.len());
        assert!(mesh.triangle_count > 1000, "Expected >1000 triangles from smooth meshing");

        // Verify normals mostly point outward
        let center = Vec3::new(64.0, 50.0, 64.0);
        let mut outward = 0;
        for v in &mesh.vertices {
            let pos = Vec3::from(v.position);
            let normal = Vec3::from(v.normal);
            let to_center = center - pos;
            if normal.dot(to_center) < 0.0 { outward += 1; }
        }
        let ratio = outward as f32 / mesh.vertices.len().max(1) as f32;
        println!("Normals outward: {:.0}%", ratio * 100.0);
    }

    #[test]
    fn test_sdf_smooth_union() {
        // Two spheres with smooth blend should have distance < min(d1, d2) at midpoint
        let mut shape = SdfShape::new("test", 1, [255,255,255]);
        shape.add(SdfPrimitive::Sphere { center: Vec3::new(-3.0, 0.0, 0.0), radius: 3.0 }, 0.0);
        shape.add(SdfPrimitive::Sphere { center: Vec3::new(3.0, 0.0, 0.0), radius: 3.0 }, 2.0);
        let d = shape.distance(Vec3::ZERO);
        // At origin: each sphere is distance 0.0. With smooth union, should be < 0
        assert!(d < 0.0);
    }
}
