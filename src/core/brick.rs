// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Brick primitive
//
// An oriented box. Free position, free rotation, free size.
// Not bound to a voxel grid. Builds triangle mesh directly,
// skipping the rasterize → cube-mesh pipeline that destroys
// orientation and detail.
//
// Bricks can attach to skeleton bones — when the bone rotates,
// the brick rotates with it. This is what gives us a real walk
// cycle, swiping paws and a curling tail.
// ═══════════════════════════════════════════════════════════════

use glam::{Mat4, Quat, Vec3};
use super::skeleton::{Skeleton, BoneId};
use super::meshing::{ChunkMesh, MeshVertex};
use super::damage::{self, Damage, Durability, HitResult};

/// One oriented box. Authoring is done in local space; the world
/// transform is computed each frame from the bone (if attached).
#[derive(Clone, Debug)]
pub struct Brick {
    pub name: String,

    // ── Authoring (local) ──────────────────────────────────
    /// Half-extents of the box (so width = half.x * 2)
    pub half_extents: Vec3,
    /// RGB color, 0-255
    pub color: [u8; 3],
    /// Engine material id (passed through to mesh)
    pub material: u8,

    // ── Skeleton attachment ─────────────────────────────────
    /// Bone this brick rides on (None = world-space static)
    pub parent: Option<BoneId>,
    /// Offset from the bone's joint (in bone-local space)
    pub local_offset: Vec3,
    /// Rotation relative to the bone (or to world if no parent)
    pub local_rotation: Quat,

    // ── World transform (recomputed each frame) ─────────────
    pub world_position: Vec3,
    pub world_rotation: Quat,
    /// Optional per-frame extra scale (idle breathing, etc.)
    pub scale: Vec3,

    // ── Damage component (None = indestructible) ────────────
    pub durability: Option<Durability>,
    /// False → excluded from mesh (e.g. shattered).
    pub visible: bool,
    /// Counts down to 0 after a hit; tints the brick red while > 0.
    pub flash_t: f32,
}

impl Brick {
    pub fn new(name: &str, half_extents: Vec3, color: [u8; 3]) -> Self {
        Self {
            name: name.to_string(),
            half_extents, color, material: 5,
            parent: None,
            local_offset: Vec3::ZERO,
            local_rotation: Quat::IDENTITY,
            world_position: Vec3::ZERO,
            world_rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            durability: None,
            visible: true,
            flash_t: 0.0,
        }
    }

    pub fn with_durability(mut self, d: Durability) -> Self {
        self.durability = Some(d); self
    }

    /// Tinted color used for rendering (flash after damage).
    fn render_color(&self) -> [u8; 3] {
        if self.flash_t <= 0.0 { return self.color; }
        let f = (self.flash_t / 0.35).clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| -> u8 {
            (a as f32 * (1.0 - f) + b as f32 * f) as u8
        };
        [
            lerp(self.color[0], 255),
            lerp(self.color[1],  60),
            lerp(self.color[2],  55),
        ]
    }

    pub fn with_position(mut self, p: Vec3) -> Self {
        self.local_offset = p;
        self
    }

    pub fn with_rotation(mut self, r: Quat) -> Self {
        self.local_rotation = r;
        self
    }

    pub fn attached_to(mut self, bone: BoneId) -> Self {
        self.parent = Some(bone);
        self
    }

    /// World-space transform matrix (for vertex transformation)
    pub fn world_transform(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.world_rotation, self.world_position)
    }
}

/// Collection of bricks forming one model (a character, a prop, etc.)
pub struct BrickModel {
    pub name: String,
    pub bricks: Vec<Brick>,
    /// Root position in world (added to every brick's world position)
    pub root_position: Vec3,
    /// Root rotation (rotates the whole model)
    pub root_rotation: Quat,
}

impl BrickModel {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(), bricks: Vec::new(),
            root_position: Vec3::ZERO,
            root_rotation: Quat::IDENTITY,
        }
    }

    pub fn add(&mut self, brick: Brick) -> usize {
        let id = self.bricks.len();
        self.bricks.push(brick);
        id
    }

    /// Recompute every brick's world transform from the skeleton
    /// (or from the model root if a brick has no parent).
    pub fn update(&mut self, skeleton: &Skeleton) {
        let root_p = self.root_position;
        let root_r = self.root_rotation;
        for b in self.bricks.iter_mut() {
            match b.parent {
                Some(bone_id) => {
                    let bone = skeleton.bone_by_id(bone_id);
                    let bone_p = bone.world_position;
                    let bone_r = bone.world_rotation;
                    b.world_rotation = bone_r * b.local_rotation;
                    b.world_position = bone_p + bone_r * b.local_offset;
                }
                None => {
                    b.world_rotation = root_r * b.local_rotation;
                    b.world_position = root_p + root_r * b.local_offset;
                }
            }
        }
    }

    /// Update transforms for a model that has no skeleton — every brick is
    /// static (parent = None). Panics if any brick has a parent bone set.
    pub fn update_static(&mut self) {
        let root_p = self.root_position;
        let root_r = self.root_rotation;
        for b in self.bricks.iter_mut() {
            assert!(b.parent.is_none(),
                "update_static called on model containing bone-attached brick '{}'", b.name);
            b.world_rotation = root_r * b.local_rotation;
            b.world_position = root_p + root_r * b.local_offset;
        }
    }

    /// Build a triangle mesh — every brick contributes 12 tris (6 faces × 2),
    /// 24 vertices (4 per face — separate so flat shading works).
    pub fn to_mesh(&self) -> ChunkMesh {
        let mut mesh = ChunkMesh::new();
        for b in &self.bricks {
            if !b.visible { continue; }
            append_brick(&mut mesh, b);
        }
        mesh
    }

    pub fn brick_count(&self) -> usize { self.bricks.len() }

    /// Decrement flash timers; call once per frame.
    pub fn tick_flash(&mut self, dt: f32) {
        for b in self.bricks.iter_mut() {
            if b.flash_t > 0.0 {
                b.flash_t = (b.flash_t - dt).max(0.0);
            }
        }
    }

    /// Raycast against all VISIBLE, BREAKABLE bricks' world-space AABBs.
    /// Returns (brick_index, distance_along_ray) of the nearest hit within `max_dist`.
    pub fn raycast_breakable(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(usize, f32)> {
        let dir = dir.normalize_or_zero();
        if dir.length_squared() < 1e-6 { return None; }
        let mut best: Option<(usize, f32)> = None;
        for (i, b) in self.bricks.iter().enumerate() {
            if !b.visible || b.durability.is_none() { continue; }
            let he = Vec3::new(
                b.half_extents.x * b.scale.x,
                b.half_extents.y * b.scale.y,
                b.half_extents.z * b.scale.z,
            );
            let min = b.world_position - he;
            let max = b.world_position + he;
            if let Some(t) = ray_aabb(origin, dir, min, max, max_dist) {
                if best.map(|(_, bt)| t < bt).unwrap_or(true) {
                    best = Some((i, t));
                }
            }
        }
        best
    }

    /// Apply a hit to a specific brick.  Returns the HitResult for caller
    /// (VFX / sfx dispatch).  `None` if the brick is not breakable.
    pub fn hit_brick(&mut self, idx: usize, dmg: &Damage) -> Option<HitResult> {
        let b = &mut self.bricks[idx];
        let dur = b.durability.as_mut()?;
        let hit = damage::apply_hit(dur, dmg);
        if hit.applied { b.flash_t = 0.35; }
        if hit.broken  { b.visible = false; }
        Some(hit)
    }
}

/// Slab-method ray vs axis-aligned box.
fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3, max_dist: f32) -> Option<f32> {
    // Guard against div-by-zero on axis-aligned rays
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

// ─── Mesh assembly ────────────────────────────────────────

/// 6 face definitions: (normal in local space, 4 corner offsets)
/// Corner offsets are unit cube corners relative to face — multiplied by half_extents.
const FACES: [(Vec3, [Vec3; 4]); 6] = [
    // +X
    (Vec3::X, [
        Vec3::new( 1.0, -1.0, -1.0), Vec3::new( 1.0,  1.0, -1.0),
        Vec3::new( 1.0,  1.0,  1.0), Vec3::new( 1.0, -1.0,  1.0),
    ]),
    // -X
    (Vec3::NEG_X, [
        Vec3::new(-1.0, -1.0,  1.0), Vec3::new(-1.0,  1.0,  1.0),
        Vec3::new(-1.0,  1.0, -1.0), Vec3::new(-1.0, -1.0, -1.0),
    ]),
    // +Y
    (Vec3::Y, [
        Vec3::new(-1.0,  1.0, -1.0), Vec3::new(-1.0,  1.0,  1.0),
        Vec3::new( 1.0,  1.0,  1.0), Vec3::new( 1.0,  1.0, -1.0),
    ]),
    // -Y
    (Vec3::NEG_Y, [
        Vec3::new(-1.0, -1.0,  1.0), Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new( 1.0, -1.0, -1.0), Vec3::new( 1.0, -1.0,  1.0),
    ]),
    // +Z
    (Vec3::Z, [
        Vec3::new( 1.0, -1.0,  1.0), Vec3::new( 1.0,  1.0,  1.0),
        Vec3::new(-1.0,  1.0,  1.0), Vec3::new(-1.0, -1.0,  1.0),
    ]),
    // -Z
    (Vec3::NEG_Z, [
        Vec3::new(-1.0, -1.0, -1.0), Vec3::new(-1.0,  1.0, -1.0),
        Vec3::new( 1.0,  1.0, -1.0), Vec3::new( 1.0, -1.0, -1.0),
    ]),
];

fn append_brick(mesh: &mut ChunkMesh, b: &Brick) {
    let xform = b.world_transform();
    let normal_xform = b.world_rotation; // rotate normals only (no scale/translation)
    let rc = b.render_color();
    let color = [
        rc[0] as f32 / 255.0,
        rc[1] as f32 / 255.0,
        rc[2] as f32 / 255.0,
        1.0,
    ];

    for (local_normal, corners) in FACES {
        let world_normal_v = normal_xform * local_normal;
        let world_normal = [world_normal_v.x, world_normal_v.y, world_normal_v.z];
        let base = mesh.vertices.len() as u32;
        for corner in corners {
            let local_p = Vec3::new(
                corner.x * b.half_extents.x,
                corner.y * b.half_extents.y,
                corner.z * b.half_extents.z,
            );
            let world_p = xform.transform_point3(local_p);
            mesh.vertices.push(MeshVertex {
                position: [world_p.x, world_p.y, world_p.z],
                normal: world_normal,
                color,
                material: b.material,
            });
        }
        // 2 tris per face (quad)
        mesh.indices.push(base);
        mesh.indices.push(base + 1);
        mesh.indices.push(base + 2);
        mesh.indices.push(base);
        mesh.indices.push(base + 2);
        mesh.indices.push(base + 3);
        mesh.triangle_count += 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_brick_produces_12_tris() {
        let mut m = BrickModel::new("test");
        m.add(Brick::new("cube", Vec3::splat(1.0), [255, 0, 0]));
        let sk = Skeleton::new("root");
        m.update(&sk);
        let mesh = m.to_mesh();
        assert_eq!(mesh.triangle_count, 12);
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn rotated_brick_changes_normal() {
        let mut m = BrickModel::new("test");
        let mut b = Brick::new("cube", Vec3::splat(1.0), [255, 0, 0]);
        b.local_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        m.add(b);
        let sk = Skeleton::new("root");
        m.update(&sk);
        let mesh = m.to_mesh();
        // After 90° Z rotation, the +X face's normal should point +Y
        let first_normal = Vec3::from(mesh.vertices[0].normal);
        assert!((first_normal - Vec3::Y).length() < 0.01,
            "Expected +Y normal after Z-rotation, got {:?}", first_normal);
    }

    #[test]
    fn brick_attached_to_bone_follows_it() {
        let mut sk = Skeleton::new("root");
        sk.add_bone("arm", "root", 5.0, Vec3::X,
            crate::core::skeleton::JointConstraint::Free);
        sk.root_position = Vec3::new(10.0, 20.0, 30.0);
        sk.solve_forward();
        let arm_id = sk.bone("arm").id;

        let mut m = BrickModel::new("test");
        let b = Brick::new("attached", Vec3::splat(1.0), [0, 255, 0])
            .attached_to(arm_id);
        m.add(b);
        m.update(&sk);
        // Brick should be at the arm bone's joint (which sits at root)
        let p = m.bricks[0].world_position;
        assert!((p - Vec3::new(10.0, 20.0, 30.0)).length() < 0.01,
            "Brick should be at arm's joint position, got {:?}", p);
    }
}
