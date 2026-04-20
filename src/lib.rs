// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Public API
//
// Voxel + mesh hybrid engine. Skeleton-driven characters,
// SDF-based body composition, GPU-meshed rendering.
//
// Stable surface (post-purge):
//   - Skeleton — bone hierarchy + forward kinematics
//   - SdfBody — SDF primitive composition into voxel grid
//   - meshing — voxel grid → triangle mesh (cube / surface_nets variants)
//   - render_mesh — wgpu pipeline for the mesh
// ═══════════════════════════════════════════════════════════════

pub mod core;

pub use core::skeleton::{Skeleton, BoneId, JointConstraint};
pub use core::svo::Voxel;
pub use core::sdf_body::{SdfBody, SdfShape, SdfPrimitive, SdfOp};
pub use core::meshing::{ChunkMesh, MeshVertex, generate_mesh, generate_mesh_with_ao};
pub use core::material::MaterialRegistry;
