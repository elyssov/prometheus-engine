// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Public API
//
// This is the entry point for anyone using the engine.
// Import one crate, build worlds.
//
// Quick start:
//   let mut world = World::new(256);
//   let soldier = Entity::orpp_soldier(2.0);
//   world.spawn(soldier, Vec3::new(128.0, 92.0, 128.0));
//   world.generate_room(RoomType::LivingRoom, 50.0, 0.5, 42);
//   loop {
//       world.entity_mut(0).aim_at(target);
//       world.update(dt);
//       world.rasterize(); // fills internal grid
//       render(world.grid());
//   }
// ═══════════════════════════════════════════════════════════════

pub mod core;

// Re-export main types for convenience
pub use core::skeleton::{Skeleton, BoneId, JointConstraint};
pub use core::body::{BodyDefinition, BoneProfile, BodySection, DecalShape};
pub use core::ik;
pub use core::attachment::{AttachedObject, weapon_ak, weapon_vikhr};
pub use core::entity::Entity;
pub use core::procgen::{self, RoomType, RoomSpec, FurnitureItem, Seed};
