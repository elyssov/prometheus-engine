// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — VoxObject
//
// A polygonal Brick that has been *promoted* to a voxel chunk after
// taking its first hit.  Stores an SVO (sparse octree) of solid
// voxels filling the brick's volume; each subsequent hit carves a
// sphere out of the SVO and re-meshes.  This gives partial damage:
// the cat scratches, doesn't merely vapourise.
//
// Design choices for V1:
//   • Axis-aligned only (apartment bricks have no rotation).
//     Rotation field is stored but not yet applied to the mesh.
//   • Fixed grid_dim per object (default 32).  vox_size derived
//     so the brick fits inside the grid.
//   • Each carve returns the world-space positions of removed
//     voxels so the caller can spawn gibs from them 1:1.
// ═══════════════════════════════════════════════════════════════

use glam::{Quat, Vec3};
use super::svo::{SVO, Voxel};
use super::meshing::{self, ChunkMesh};
use super::damage::Durability;

pub struct VoxObject {
    /// World-space centre of the original brick.
    pub origin: Vec3,
    /// World-space rotation (currently unused — bricks are axis-aligned).
    pub rotation: Quat,
    /// World units per voxel cell.
    pub vox_size: f32,
    /// Power-of-two grid dimension.
    pub grid_dim: usize,
    /// Half-extents of the original brick in world units — used for collision
    /// AABB so the box doesn't bloat to the SVO's grid size.
    pub original_he: Vec3,
    /// Sparse octree of voxels.
    pub svo: SVO,
    /// Base RGB colour of the source brick (used to spawn gib colour).
    pub color: [u8; 3],
    /// Durability — initialised from the source brick.  Carving deducts hp.
    pub durability: Durability,
    /// Whether the GPU mesh needs rebuilding.
    pub dirty: bool,
    /// Cached number of solid voxels (for fully-destroyed checks).
    pub solid_count: u32,
    /// Original solid voxel count at creation.
    pub initial_count: u32,
}

impl VoxObject {
    /// Promote a polygonal brick into a voxel chunk.  Fills the box
    /// extents with solid voxels of the brick's colour.
    pub fn from_brick(
        world_pos: Vec3,
        world_rot: Quat,
        half_extents: Vec3,
        color: [u8; 3],
        material: u8,
        durability: Durability,
        grid_dim: usize,
    ) -> Self {
        assert!(grid_dim.is_power_of_two());
        let max_side = (half_extents.max_element() * 2.0).max(0.01);
        let vox_size = max_side / grid_dim as f32;

        let mut svo = SVO::new(grid_dim);

        let nx = ((half_extents.x * 2.0) / vox_size).round().max(1.0) as usize;
        let ny = ((half_extents.y * 2.0) / vox_size).round().max(1.0) as usize;
        let nz = ((half_extents.z * 2.0) / vox_size).round().max(1.0) as usize;
        let nx = nx.min(grid_dim);
        let ny = ny.min(grid_dim);
        let nz = nz.min(grid_dim);
        let ox = (grid_dim - nx) / 2;
        let oy = (grid_dim - ny) / 2;
        let oz = (grid_dim - nz) / 2;

        let v = Voxel::solid(material, color[0], color[1], color[2]);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    svo.set(ox + x, oy + y, oz + z, v);
                }
            }
        }
        let initial_count = (nx * ny * nz) as u32;

        Self {
            origin: world_pos,
            rotation: world_rot,
            vox_size,
            grid_dim,
            original_he: half_extents,
            svo,
            color,
            durability,
            dirty: true,
            solid_count: initial_count,
            initial_count,
        }
    }

    /// Carve a sphere out of the chunk.  Returns the world-space positions
    /// of each removed voxel so the caller can spawn gibs.  Also marks
    /// the mesh dirty if anything was removed.
    pub fn carve_sphere(&mut self, world_impact: Vec3, radius_world: f32,
                        gib_positions: &mut Vec<Vec3>)
    {
        let local = self.rotation.inverse() * (world_impact - self.origin);
        let half_grid = self.grid_dim as f32 * self.vox_size * 0.5;
        let cx = (local.x + half_grid) / self.vox_size;
        let cy = (local.y + half_grid) / self.vox_size;
        let cz = (local.z + half_grid) / self.vox_size;
        let r_grid = radius_world / self.vox_size;
        let r2 = r_grid * r_grid;

        let g = self.grid_dim as f32 - 1.0;
        let xmin = (cx - r_grid).floor().max(0.0) as usize;
        let xmax = (cx + r_grid).ceil().min(g) as usize;
        let ymin = (cy - r_grid).floor().max(0.0) as usize;
        let ymax = (cy + r_grid).ceil().min(g) as usize;
        let zmin = (cz - r_grid).floor().max(0.0) as usize;
        let zmax = (cz + r_grid).ceil().min(g) as usize;

        let mut removed = 0u32;
        for z in zmin..=zmax {
            for y in ymin..=ymax {
                for x in xmin..=xmax {
                    let dx = x as f32 + 0.5 - cx;
                    let dy = y as f32 + 0.5 - cy;
                    let dz = z as f32 + 0.5 - cz;
                    if dx*dx + dy*dy + dz*dz <= r2 {
                        let v = self.svo.get(x, y, z);
                        if v.is_solid() {
                            self.svo.remove(x, y, z);
                            // World position of voxel centre
                            let wp = self.origin + self.rotation * Vec3::new(
                                (x as f32 + 0.5) * self.vox_size - half_grid,
                                (y as f32 + 0.5) * self.vox_size - half_grid,
                                (z as f32 + 0.5) * self.vox_size - half_grid,
                            );
                            gib_positions.push(wp);
                            removed += 1;
                        }
                    }
                }
            }
        }
        if removed > 0 {
            self.dirty = true;
            self.solid_count = self.solid_count.saturating_sub(removed);
        }
    }

    /// Annihilate everything left — used when a hit's effective power
    /// is overwhelmingly larger than the object's hp.  Salfetka case.
    pub fn annihilate(&mut self, gib_positions: &mut Vec<Vec3>) {
        let half_grid = self.grid_dim as f32 * self.vox_size * 0.5;
        for z in 0..self.grid_dim {
            for y in 0..self.grid_dim {
                for x in 0..self.grid_dim {
                    if self.svo.get(x, y, z).is_solid() {
                        self.svo.remove(x, y, z);
                        let wp = self.origin + self.rotation * Vec3::new(
                            (x as f32 + 0.5) * self.vox_size - half_grid,
                            (y as f32 + 0.5) * self.vox_size - half_grid,
                            (z as f32 + 0.5) * self.vox_size - half_grid,
                        );
                        gib_positions.push(wp);
                    }
                }
            }
        }
        self.solid_count = 0;
        self.dirty = true;
    }

    pub fn is_empty(&self) -> bool { self.solid_count == 0 }

    /// Build a triangle mesh of the current voxel state in world space.
    /// V1 ignores rotation — the brick's local frame must be axis-aligned
    /// (true for everything in apartment.rs).
    pub fn build_mesh(&self) -> ChunkMesh {
        let half_grid = self.grid_dim as f32 * self.vox_size * 0.5;
        let offset = self.origin - Vec3::splat(half_grid);
        let flat = self.svo.export_flat(self.grid_dim);
        meshing::generate_mesh(&flat, self.grid_dim, offset, self.vox_size)
    }

    /// Axis-aligned world bounding box (for raycasting and collision).
    /// Uses the *original brick's* half-extents — not the SVO grid envelope —
    /// so a flat rug doesn't become a column-tall obstacle after first hit.
    pub fn world_aabb(&self) -> (Vec3, Vec3) {
        (self.origin - self.original_he, self.origin + self.original_he)
    }

    /// True if any voxel in the vertical column at `world_xz` between
    /// `slab_lo` and `slab_hi` (world-Y) is solid.  Used by collision so
    /// the cat can walk through carved-out holes instead of being blocked
    /// by the original AABB.
    pub fn column_solid(&self, world_xz: glam::Vec2, slab_lo: f32, slab_hi: f32) -> bool {
        let p = Vec3::new(world_xz.x, 0.0, world_xz.y);
        let local = self.rotation.inverse() * (p - self.origin);
        let half_grid = self.grid_dim as f32 * self.vox_size * 0.5;
        let lx_f = (local.x + half_grid) / self.vox_size;
        let lz_f = (local.z + half_grid) / self.vox_size;
        if lx_f < 0.0 || lz_f < 0.0
            || lx_f >= self.grid_dim as f32 || lz_f >= self.grid_dim as f32 {
            return false;
        }
        let lx = lx_f as usize;
        let lz = lz_f as usize;
        // Convert slab Y range to local grid Y indices.
        let ly_lo_f = (slab_lo - self.origin.y + half_grid) / self.vox_size;
        let ly_hi_f = (slab_hi - self.origin.y + half_grid) / self.vox_size;
        let ly_lo = ly_lo_f.floor().max(0.0) as usize;
        let ly_hi = ly_hi_f.ceil().min(self.grid_dim as f32 - 1.0) as usize;
        for ly in ly_lo..=ly_hi {
            if self.svo.get(lx, ly, lz).is_solid() { return true; }
        }
        false
    }
}
