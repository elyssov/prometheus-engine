# Prometheus Engine 🔥

**Voxel engine with full destructibility, GPU raymarching, and procedural generation.**

*"Code creates worlds."*

---

## What is this

Prometheus Engine is a cross-platform voxel engine built from scratch in Rust. Every object in the world is made of voxels. Every voxel can be destroyed. Every destruction triggers physics, particles, sound, and chain reactions.

Built by a human architect and an AI engineer. This is the first commercial voxel engine created in such a configuration.

## Core Features

- **Sparse Voxel Octree (SVO)** — efficient storage, O(log N) access
- **GPU Raymarching** — direct voxel rendering via wgpu (Vulkan / Metal / DX12 / WebGPU)
- **Full Destructibility** — every object shatters, crumbles, or snaps based on material properties
- **Chain Reactions** — objects fall, hit other objects, which fall, hit others...
- **Procedural Generation** — rooms, buildings, cities, furniture placement
- **Skeletal Animation** — voxel characters with bone hierarchies
- **Material System** — 14+ materials with unique physics, sounds, and particles
- **Multi-Camera** — ThirdPerson, SideScroller, Isometric, TopDown, FirstPerson, Cinematic
- **AI / Behavior Trees** — pathfinding on voxel grids, perception (sight, hearing)
- **Physics** — gravity, collisions, ragdoll, liquids (simplified)
- **ECS Architecture** — Entity Component System for clean, modular game logic

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust |
| Graphics | wgpu (Vulkan / Metal / DX12 / WebGPU) |
| Math | glam |
| Audio | kira |
| Windowing | winit |
| Platforms | Windows, macOS, Linux, Web (WASM), Android, iOS, Switch |

## Roadmap

| Version | Focus | Target |
|---------|-------|--------|
| **v0.1** | First voxel on screen, raymarching, basic destruction | Now |
| **v0.5** | Physics, chain reactions, skeletal animation | Q2 2026 |
| **v1.0** | PURRGE ships | Q3 2026 |
| **v2.0** | Extended materials, city generation, tactical camera | 2027 |

## Games on Prometheus

- **[PURRGE](https://github.com/elyssov/purrge)** — Kawaii roguelike about a cat destroying a voxel apartment (showcase game)
- **ORPP: Apocalypse** — Tactical survival horror in the AEGIS universe (future)

## License

Source-available. Free to use. 5% royalty on revenue exceeding $100,000.

See [LICENSE](LICENSE) for details.

---

*Built with Rust, stubbornness, and the belief that code creates worlds.*
*Prometheus stole fire from the gods. We ARE the fire.*
