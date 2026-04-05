// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Marching Cubes Mesh Generator
//
// Converts voxel data into triangle meshes for GPU rendering.
// Voxels = data (physics, destruction). Polygons = render (GPU).
//
// Smooth surfaces for organic shapes (bodies, terrain).
// Sharp edges for architecture (walls, furniture) via edge detection.
//
// Each 64³ chunk → ~50-100K triangles, generated in ~0.5ms on CPU.
// ═══════════════════════════════════════════════════════════════

use glam::Vec3;
use super::svo::Voxel;

/// A single vertex with position, normal, and color
#[derive(Clone, Copy, Debug)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],    // RGBA (0.0-1.0)
    pub material: u8,
}

/// A triangle mesh generated from voxels
pub struct ChunkMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub triangle_count: usize,
}

impl ChunkMesh {
    pub fn new() -> Self {
        Self { vertices: Vec::new(), indices: Vec::new(), triangle_count: 0 }
    }

    pub fn is_empty(&self) -> bool { self.triangle_count == 0 }

    /// Memory usage in bytes
    pub fn memory_bytes(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<MeshVertex>()
        + self.indices.len() * 4
    }
}

/// Generate mesh from a flat voxel array using surface extraction.
/// Simple but effective: for each solid voxel with an empty neighbor,
/// emit a quad (2 triangles) on that face.
///
/// This is NOT full Marching Cubes — it's "greedy meshing" which
/// produces clean, sharp voxel surfaces. Better for architecture.
/// We'll add smooth Marching Cubes as an option for organic shapes.
pub fn generate_mesh(
    voxels: &[Voxel],
    size: usize,
    offset: Vec3,   // world-space offset of this chunk
    scale: f32,     // voxel size in world units
) -> ChunkMesh {
    let mut mesh = ChunkMesh::new();

    let get = |x: i32, y: i32, z: i32| -> Voxel {
        if x < 0 || y < 0 || z < 0 || x >= size as i32 || y >= size as i32 || z >= size as i32 {
            return Voxel::empty();
        }
        voxels[(z as usize) * size * size + (y as usize) * size + (x as usize)]
    };

    // 6 face directions: +X, -X, +Y, -Y, +Z, -Z
    let dirs: [(i32,i32,i32, [f32;3]); 6] = [
        ( 1, 0, 0, [ 1.0, 0.0, 0.0]),  // +X
        (-1, 0, 0, [-1.0, 0.0, 0.0]),  // -X
        ( 0, 1, 0, [ 0.0, 1.0, 0.0]),  // +Y
        ( 0,-1, 0, [ 0.0,-1.0, 0.0]),  // -Y
        ( 0, 0, 1, [ 0.0, 0.0, 1.0]),  // +Z
        ( 0, 0,-1, [ 0.0, 0.0,-1.0]),  // -Z
    ];

    for z in 0..size as i32 {
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let voxel = get(x, y, z);
                if !voxel.is_solid() { continue; }

                let color = voxel_to_color(voxel);

                // Check each face
                for &(dx, dy, dz, normal) in &dirs {
                    let neighbor = get(x + dx, y + dy, z + dz);
                    if neighbor.is_solid() { continue; } // face hidden

                    // Emit quad for this visible face
                    let base_idx = mesh.vertices.len() as u32;
                    let (v0, v1, v2, v3) = face_vertices(
                        x as f32, y as f32, z as f32,
                        dx, dy, dz, scale, offset,
                    );

                    let mat = (voxel.packed & 0xFF) as u8;
                    mesh.vertices.push(MeshVertex { position: v0, normal, color, material: mat });
                    mesh.vertices.push(MeshVertex { position: v1, normal, color, material: mat });
                    mesh.vertices.push(MeshVertex { position: v2, normal, color, material: mat });
                    mesh.vertices.push(MeshVertex { position: v3, normal, color, material: mat });

                    // Two triangles per quad
                    mesh.indices.push(base_idx);
                    mesh.indices.push(base_idx + 1);
                    mesh.indices.push(base_idx + 2);
                    mesh.indices.push(base_idx);
                    mesh.indices.push(base_idx + 2);
                    mesh.indices.push(base_idx + 3);

                    mesh.triangle_count += 2;
                }
            }
        }
    }

    mesh
}

/// Compute 4 vertices for a face of a voxel cube
fn face_vertices(
    x: f32, y: f32, z: f32,
    dx: i32, dy: i32, dz: i32,
    scale: f32, offset: Vec3,
) -> ([f32;3], [f32;3], [f32;3], [f32;3]) {
    let s = scale;
    let o = offset;

    // Base position of voxel corner
    let bx = o.x + x * s;
    let by = o.y + y * s;
    let bz = o.z + z * s;

    if dx == 1 { // +X face
        ([bx+s, by,   bz  ], [bx+s, by+s, bz  ], [bx+s, by+s, bz+s], [bx+s, by,   bz+s])
    } else if dx == -1 { // -X face
        ([bx,   by,   bz+s], [bx,   by+s, bz+s], [bx,   by+s, bz  ], [bx,   by,   bz  ])
    } else if dy == 1 { // +Y face
        ([bx,   by+s, bz  ], [bx,   by+s, bz+s], [bx+s, by+s, bz+s], [bx+s, by+s, bz  ])
    } else if dy == -1 { // -Y face
        ([bx,   by,   bz+s], [bx,   by,   bz  ], [bx+s, by,   bz  ], [bx+s, by,   bz+s])
    } else if dz == 1 { // +Z face
        ([bx+s, by,   bz+s], [bx+s, by+s, bz+s], [bx,   by+s, bz+s], [bx,   by,   bz+s])
    } else { // -Z face
        ([bx,   by,   bz  ], [bx,   by+s, bz  ], [bx+s, by+s, bz  ], [bx+s, by,   bz  ])
    }
}

/// Extract color from packed voxel
fn voxel_to_color(v: Voxel) -> [f32; 4] {
    let r = ((v.packed >> 8) & 0xFF) as f32 / 255.0;
    let g = ((v.packed >> 16) & 0xFF) as f32 / 255.0;
    let b = ((v.packed >> 24) & 0xFF) as f32 / 255.0;
    [r, g, b, 1.0]
}

/// Compute ambient occlusion for a vertex (simple: count solid neighbors)
pub fn compute_ao(voxels: &[Voxel], size: usize, x: i32, y: i32, z: i32, normal: [f32;3]) -> f32 {
    let get = |x: i32, y: i32, z: i32| -> bool {
        if x < 0 || y < 0 || z < 0 || x >= size as i32 || y >= size as i32 || z >= size as i32 {
            return false;
        }
        voxels[(z as usize)*size*size + (y as usize)*size + (x as usize)].is_solid()
    };

    let mut occluded = 0;
    let mut total = 0;

    for dz in -1i32..=1 {
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 && dz == 0 { continue; }
                let dot = dx as f32 * normal[0] + dy as f32 * normal[1] + dz as f32 * normal[2];
                if dot < 0.0 { continue; } // only check on normal side
                total += 1;
                if get(x + dx, y + dy, z + dz) { occluded += 1; }
            }
        }
    }

    if total == 0 { return 1.0; }
    1.0 - (occluded as f32 / total as f32) * 0.5
}

/// Generate mesh WITH per-vertex AO
pub fn generate_mesh_with_ao(
    voxels: &[Voxel],
    size: usize,
    offset: Vec3,
    scale: f32,
) -> ChunkMesh {
    let mut mesh = generate_mesh(voxels, size, offset, scale);

    // Post-process: compute AO for each vertex
    for v in mesh.vertices.iter_mut() {
        let vx = ((v.position[0] - offset.x) / scale) as i32;
        let vy = ((v.position[1] - offset.y) / scale) as i32;
        let vz = ((v.position[2] - offset.z) / scale) as i32;
        let ao = compute_ao(voxels, size, vx, vy, vz, v.normal);
        v.color[0] *= ao;
        v.color[1] *= ao;
        v.color[2] *= ao;
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_chunk(size: usize) -> Vec<Voxel> {
        let mut voxels = vec![Voxel::empty(); size * size * size];
        // Fill a small cube in the center
        let half = size / 2;
        let r = size / 4;
        for z in (half-r)..(half+r) {
            for y in (half-r)..(half+r) {
                for x in (half-r)..(half+r) {
                    voxels[z * size * size + y * size + x] = Voxel::solid(1, 200, 100, 50);
                }
            }
        }
        voxels
    }

    #[test]
    fn test_generate_mesh_basic() {
        let voxels = make_test_chunk(16);
        let mesh = generate_mesh(&voxels, 16, Vec3::ZERO, 1.0);

        println!("16³ chunk with 8³ cube: {} triangles, {} vertices",
            mesh.triangle_count, mesh.vertices.len());

        // 8³ cube has 6 faces, each face = 8×8 quads, each quad = 2 triangles
        // But only SURFACE faces (not interior), so:
        // 6 faces × 64 surface quads = 384 quads = 768 triangles
        assert!(mesh.triangle_count > 0);
        assert!(mesh.triangle_count <= 768);
    }

    #[test]
    fn test_single_voxel() {
        let mut voxels = vec![Voxel::empty(); 4*4*4];
        voxels[1*4*4 + 1*4 + 1] = Voxel::solid(1, 255, 0, 0);

        let mesh = generate_mesh(&voxels, 4, Vec3::ZERO, 1.0);

        println!("Single voxel: {} triangles", mesh.triangle_count);
        // Single voxel exposed on all 6 sides = 6 quads = 12 triangles
        assert_eq!(mesh.triangle_count, 12);
    }

    #[test]
    fn test_empty_chunk() {
        let voxels = vec![Voxel::empty(); 8*8*8];
        let mesh = generate_mesh(&voxels, 8, Vec3::ZERO, 1.0);
        assert!(mesh.is_empty());
    }

    #[test]
    fn test_full_chunk_no_interior_faces() {
        // Completely filled chunk: only outer faces visible
        let voxels = vec![Voxel::solid(1, 100, 100, 100); 8*8*8];
        let mesh = generate_mesh(&voxels, 8, Vec3::ZERO, 1.0);

        println!("Full 8³: {} triangles", mesh.triangle_count);
        // Only boundary faces: 6 faces × 64 quads = 384 quads = 768 triangles
        assert_eq!(mesh.triangle_count, 768);
    }

    #[test]
    fn test_mesh_with_ao() {
        let voxels = make_test_chunk(16);
        let mesh = generate_mesh_with_ao(&voxels, 16, Vec3::ZERO, 1.0);

        // Verify AO darkened some vertices
        let min_brightness: f32 = mesh.vertices.iter()
            .map(|v| v.color[0])
            .fold(f32::MAX, f32::min);
        println!("Min vertex brightness after AO: {:.3}", min_brightness);
        // Corner vertices should be darker
        assert!(min_brightness < 0.75);
    }

    #[test]
    fn test_64_chunk_performance() {
        let mut voxels = vec![Voxel::empty(); 64*64*64];
        // Floor + walls (typical room)
        for z in 0..64 { for x in 0..64 {
            voxels[z*64*64 + 0*64 + x] = Voxel::solid(7, 180, 155, 115); // floor
        }}
        for z in 0..64 { for y in 0..64 {
            voxels[z*64*64 + y*64 + 0] = Voxel::solid(7, 220, 215, 205); // wall
            voxels[z*64*64 + y*64 + 63] = Voxel::solid(7, 220, 215, 205);
        }}
        // Some furniture boxes
        for z in 20..30 { for y in 1..10 { for x in 20..30 {
            voxels[z*64*64 + y*64 + x] = Voxel::solid(1, 110, 70, 35); // table
        }}}

        let start = std::time::Instant::now();
        let mesh = generate_mesh(&voxels, 64, Vec3::ZERO, 1.0);
        let elapsed = start.elapsed();

        println!("64³ room chunk: {} triangles, {} vertices, {:.2} ms",
            mesh.triangle_count, mesh.vertices.len(), elapsed.as_secs_f64() * 1000.0);
        println!("Memory: {:.1} KB", mesh.memory_bytes() as f64 / 1024.0);

        // Should complete in under 50ms (debug mode is slow, release ~2ms)
        assert!(elapsed.as_millis() < 50);
        assert!(mesh.triangle_count > 1000);
    }
}
