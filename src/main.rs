// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE 2.0 — Demo: Mesh-Based Rendering
//
// Soldier in a room, polygon rendering via Marching Cubes pipeline.
// Dual Representation: voxels for data, polygons for eyes.
//
// Controls:
//   A     — toggle auto-rotate
//   Space — destroy sphere at center
//   R     — rebuild
//   Esc   — quit
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

use core::svo::Voxel;
use core::meshing;
use core::render_mesh::{self, MeshUniforms, GpuMesh};
use core::entity::Entity;
use core::material::MaterialRegistry;
use core::sdf_body::SdfBody;

const GRID: usize = 1024;

// ─── Sparse Voxel Grid — only stores filled voxels ──────────
// 1024³ flat = 8 GB impossible. HashMap = only what's filled.
// For meshing, we export a tight bounding box as flat array.
struct Grid {
    voxels: std::collections::HashMap<(u16, u16, u16), Voxel>,
    // Bounding box of filled voxels
    min: [u16; 3],
    max: [u16; 3],
}

impl Grid {
    fn new() -> Self {
        Self {
            voxels: std::collections::HashMap::with_capacity(500_000),
            min: [u16::MAX; 3],
            max: [0; 3],
        }
    }
    fn clear(&mut self) {
        self.voxels.clear();
        self.min = [u16::MAX; 3];
        self.max = [0; 3];
    }
    fn set(&mut self, x: usize, y: usize, z: usize, v: Voxel) {
        if x < GRID && y < GRID && z < GRID {
            self.voxels.insert((x as u16, y as u16, z as u16), v);
            self.min[0] = self.min[0].min(x as u16);
            self.min[1] = self.min[1].min(y as u16);
            self.min[2] = self.min[2].min(z as u16);
            self.max[0] = self.max[0].max(x as u16);
            self.max[1] = self.max[1].max(y as u16);
            self.max[2] = self.max[2].max(z as u16);
        }
    }

    /// Export bounding box region as flat array for meshing.
    /// Uses rectangular grid (padded to cube of max dimension).
    /// For very tall/narrow figures, pads X and Z to reduce waste.
    fn export_for_meshing(&self) -> (Vec<Voxel>, usize, Vec3) {
        if self.voxels.is_empty() {
            return (vec![Voxel::empty(); 8], 2, Vec3::ZERO);
        }
        let margin = 3u16;
        let x0 = self.min[0].saturating_sub(margin) as usize;
        let y0 = self.min[1].saturating_sub(margin) as usize;
        let z0 = self.min[2].saturating_sub(margin) as usize;
        let x1 = (self.max[0] + margin).min(GRID as u16 - 1) as usize + 1;
        let y1 = (self.max[1] + margin).min(GRID as u16 - 1) as usize + 1;
        let z1 = (self.max[2] + margin).min(GRID as u16 - 1) as usize + 1;

        let sx = x1 - x0;
        let sy = y1 - y0;
        let sz = z1 - z0;
        // Meshing needs uniform size — use max dimension
        let size = sx.max(sy).max(sz);

        // Cap at 512 to keep memory reasonable (~1GB)
        let size = size.min(512);

        let total = size * size * size;
        let mb = (total * 8) as f64 / 1_048_576.0;
        println!("  Sparse: {} voxels, bbox {}×{}×{}, export {}³ ({:.0} MB)",
            self.voxels.len(), sx, sy, sz, size, mb);

        let mut flat = vec![Voxel::empty(); total];
        for (&(x, y, z), &v) in &self.voxels {
            let lx = x as usize - x0;
            let ly = y as usize - y0;
            let lz = z as usize - z0;
            if lx < size && ly < size && lz < size {
                flat[lz * size * size + ly * size + lx] = v;
            }
        }

        (flat, size, Vec3::new(x0 as f32, y0 as f32, z0 as f32))
    }
}

// ─── Camera ──────────────────────────────────────────────────
struct Camera { angle: f32, pitch: f32, dist: f32, center: Vec3, fov: f32 }
impl Camera {
    fn new() -> Self {
        let c = GRID as f32 / 2.0;
        // Camera focuses on center of grid where skull is placed
        Self { angle: 0.3, pitch: 0.2, dist: 200.0, center: Vec3::new(c, c, c), fov: 60.0 }
    }
    fn eye(&self) -> Vec3 {
        Vec3::new(
            self.center.x + self.dist * self.pitch.cos() * self.angle.sin(),
            self.center.y + self.dist * self.pitch.sin(),
            self.center.z + self.dist * self.pitch.cos() * self.angle.cos(),
        )
    }
    fn forward(&self) -> Vec3 { (self.center - self.eye()).normalize() }
    fn right(&self) -> Vec3 { self.forward().cross(Vec3::Y).normalize() }
    fn view(&self) -> Mat4 { Mat4::look_at_rh(self.eye(), self.center, Vec3::Y) }
    fn proj(&self, a: f32) -> Mat4 { Mat4::perspective_rh(self.fov.to_radians(), a, 0.1, 2000.0) }
    fn zoom(&mut self, delta: f32) { self.dist = (self.dist - delta * 20.0).clamp(20.0, 3000.0); }
    fn move_dir(&mut self, fwd: f32, right: f32, up: f32) {
        let f = self.forward();
        let r = self.right();
        let speed = GRID as f32 * 0.02;
        self.center += f * fwd * speed + r * right * speed + Vec3::Y * up * speed;
    }
}

// ─── App ─────────────────────────────────────────────────────
struct App {
    win: Option<Arc<Window>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    pipeline: Option<wgpu::RenderPipeline>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    uniform_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    depth_view: Option<wgpu::TextureView>,
    gpu_mesh: Option<GpuMesh>,
    grid: Grid,
    cam: Camera,
    materials: MaterialRegistry,
    soldier: Entity,
    time: f32,
    rotate: bool,
    frame: u32,
    needs_remesh: bool,
    mouse_dragging: bool,
    last_mouse: (f64, f64),
}

impl App {
    fn new() -> Self {
        let mut grid = Grid::new();

        let s = GRID as f32 / 128.0; // scale=8 at 1024
        let center = Vec3::new(GRID as f32 / 2.0, GRID as f32 / 2.0, GRID as f32 / 2.0);

        // Full SDF human body from skeleton
        let mut soldier = Entity::skeleton_preview(s);
        soldier.set_position(center);
        soldier.update(); // solve_forward() inside

        let sdf_body = SdfBody::human_body(&soldier.skeleton, s);
        sdf_body.rasterize(GRID, 1.5, |x,y,z,mat,r,g,b| {
            grid.set(x, y, z, Voxel::solid(mat, r, g, b));
        });
        println!("  SDF body: {} voxels at scale {:.1}", grid.voxels.len(), s);

        Self {
            win: None, device: None, queue: None, surface: None, config: None,
            pipeline: None, bind_group_layout: None, uniform_buffer: None,
            bind_group: None, depth_view: None, gpu_mesh: None,
            grid, cam: Camera::new(), materials: MaterialRegistry::default(),
            soldier, time: 0.0, rotate: false, frame: 0, needs_remesh: true,
            mouse_dragging: false, last_mouse: (0.0, 0.0),
        }
    }

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
                label: Some("Prometheus"), required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(), memory_hints: wgpu::MemoryHints::default(),
            }, None,
        )).unwrap();

        let size = window.inner_size();
        let config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).unwrap();
        surface.configure(&device, &config);

        // Create mesh pipeline
        let (pipeline, bgl) = render_mesh::create_mesh_pipeline(&device, config.format);

        // Uniform buffer
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniforms"),
            contents: bytemuck::bytes_of(&MeshUniforms::new(
                Mat4::IDENTITY, Mat4::IDENTITY, Vec3::ZERO, Vec3::new(0.3, -0.8, 0.5).normalize(),
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BG"), layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }],
        });

        // Depth buffer
        let (_, depth_view) = render_mesh::create_depth_texture(&device, config.width, config.height);

        self.win = Some(window);
        self.pipeline = Some(pipeline);
        self.bind_group_layout = Some(bgl);
        self.uniform_buffer = Some(uniform_buffer);
        self.bind_group = Some(bind_group);
        self.depth_view = Some(depth_view);
        self.device = Some(device);
        self.queue = Some(queue);
        self.surface = Some(surface);
        self.config = Some(config);
        self.needs_remesh = true;
    }

    fn rebuild_mesh(&mut self) {
        if let Some(device) = &self.device {
            let start = std::time::Instant::now();
            // Export sparse grid to tight flat array for meshing
            let (flat, size, offset) = self.grid.export_for_meshing();
            let export_time = start.elapsed();

            let mesh_start = std::time::Instant::now();
            let mesh = meshing::generate_mesh_smooth_with_ao(
                &flat, size, offset, 1.0,
            );
            let mesh_time = mesh_start.elapsed();

            println!("  Mesh: {} tri, {} vert, {:.1} KB | export {:.0}ms + mesh {:.0}ms",
                mesh.triangle_count, mesh.vertices.len(),
                mesh.memory_bytes() as f64 / 1024.0,
                export_time.as_secs_f64() * 1000.0,
                mesh_time.as_secs_f64() * 1000.0);
            self.gpu_mesh = GpuMesh::from_chunk_mesh(device, &mesh);
            self.needs_remesh = false;
        }
    }

    fn render(&mut self) {
        if self.needs_remesh { self.rebuild_mesh(); }

        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let surface = self.surface.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        if self.rotate { self.cam.angle += 0.008; }
        self.time += 1.0 / 60.0;

        // Update uniforms
        let aspect = config.width as f32 / config.height as f32;
        let uniforms = MeshUniforms::new(
            self.cam.view(), self.cam.proj(aspect), self.cam.eye(),
            Vec3::new(0.3, -0.8, -0.5).normalize(),
        );
        queue.write_buffer(self.uniform_buffer.as_ref().unwrap(), 0, bytemuck::bytes_of(&uniforms));

        // Get frame
        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => { surface.configure(device, config); return; }
        };
        let view = frame.texture.create_view(&Default::default());

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mesh Render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.03, a: 1.0 }),
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

            if let Some(mesh) = &self.gpu_mesh {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        self.win.as_ref().unwrap().request_redraw();
        self.frame += 1;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if self.win.is_some() { return; }
        let w = Arc::new(el.create_window(
            Window::default_attributes()
                .with_title("PROMETHEUS ENGINE 2.0 — Surface Nets")
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
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                match event.physical_key {
                    // WASD camera movement
                    PhysicalKey::Code(KeyCode::KeyW) => self.cam.move_dir(1.0, 0.0, 0.0),
                    PhysicalKey::Code(KeyCode::KeyS) => self.cam.move_dir(-1.0, 0.0, 0.0),
                    PhysicalKey::Code(KeyCode::KeyD) => self.cam.move_dir(0.0, 1.0, 0.0),
                    // FOV control
                    PhysicalKey::Code(KeyCode::Equal) | PhysicalKey::Code(KeyCode::NumpadAdd) => {
                        self.cam.fov = (self.cam.fov + 5.0).min(170.0);
                        println!("  FOV: {:.0}°", self.cam.fov);
                    }
                    PhysicalKey::Code(KeyCode::Minus) | PhysicalKey::Code(KeyCode::NumpadSubtract) => {
                        self.cam.fov = (self.cam.fov - 5.0).max(20.0);
                        println!("  FOV: {:.0}°", self.cam.fov);
                    }
                    // Q/E for up/down
                    PhysicalKey::Code(KeyCode::KeyQ) => self.cam.move_dir(0.0, 0.0, -1.0),
                    PhysicalKey::Code(KeyCode::KeyE) => self.cam.move_dir(0.0, 0.0, 1.0),
                    PhysicalKey::Code(KeyCode::Space) => {
                        // Destruction disabled in sparse mode (needs flat grid)
                        println!("  Destruction not available in sparse mode");
                    }
                    PhysicalKey::Code(KeyCode::KeyR) => {
                        self.grid.clear();
                        let s = GRID as f32 / 128.0;
                        let center = Vec3::new(GRID as f32/2.0, GRID as f32/2.0, GRID as f32/2.0);
                        self.soldier.set_position(center);
                        self.soldier.update();
                        let sdf_body = SdfBody::human_body(&self.soldier.skeleton, s);
                        sdf_body.rasterize(GRID, 1.5, |x,y,z,mat,r,g,b| {
                            self.grid.set(x, y, z, Voxel::solid(mat, r, g, b));
                        });
                        self.needs_remesh = true;
                        println!("  Rebuilt: {} voxels", self.grid.voxels.len());
                    }
                    PhysicalKey::Code(KeyCode::KeyA) => self.cam.move_dir(0.0, -1.0, 0.0),
                    PhysicalKey::Code(KeyCode::KeyT) => { self.rotate = !self.rotate; }
                    PhysicalKey::Code(KeyCode::Escape) => el.exit(),
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    self.mouse_dragging = state.is_pressed();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.mouse_dragging {
                    let dx = position.x - self.last_mouse.0;
                    let dy = position.y - self.last_mouse.1;
                    self.cam.angle -= dx as f32 * 0.005;
                    self.cam.pitch = (self.cam.pitch + dy as f32 * 0.003).clamp(0.05, 1.2);
                }
                self.last_mouse = (position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.cam.zoom(scroll);
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    println!();
    println!("  ═══════════════════════════════════════");
    println!("  🔥 PROMETHEUS ENGINE 2.0");
    println!("     Dual Representation: Voxels → Smooth Polygons");
    println!("     FOV 120° | Surface Nets | AO | Laplacian Relaxation");
    println!("  ═══════════════════════════════════════");
    println!();
    println!("  WASD = move  QE = up/down  +- = FOV  A = rotate  Esc = quit");
    println!();

    let el = EventLoop::new().unwrap();
    let mut app = App::new();
    el.run_app(&mut app).unwrap();
}
