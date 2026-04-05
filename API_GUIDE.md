# Prometheus Engine — API Guide
## For AI coders and human developers
## Pre-alpha — API will evolve

---

## Quick Start

```rust
use prometheus_engine::*;
use glam::Vec3;

// 1. Create a soldier
let mut soldier = Entity::orpp_soldier(2.0);  // scale 2.0 for 256³ grid
soldier.set_position(Vec3::new(128.0, 92.0, 128.0));

// 2. Aim at something
soldier.aim_at(Vec3::new(200.0, 80.0, 150.0));

// 3. Update (solves FK, IK, attachments)
soldier.update();

// 4. Fire — bullet comes from muzzle automatically
if let Some(fire) = soldier.fire() {
    println!("Bullet from {:?} going {:?}", fire.origin, fire.direction);
}

// 5. Rasterize into voxel grid
let mut grid = vec![0u8; 256*256*256*8];
soldier.rasterize(256, |x, y, z, mat, r, g, b| {
    // set voxel at (x,y,z) with material mat and color (r,g,b)
});
```

---

## Skeleton

### Create

```rust
// Prefab skeletons
let human = Skeleton::human(1.0);  // 21 bones, standard proportions
let cat = Skeleton::cat(1.0);      // 26 bones, with tail and ears

// Custom skeleton
let mut sk = Skeleton::new("root");
sk.add_bone("spine", "root", 10.0, Vec3::Y, JointConstraint::BallSocket {
    cone_angle: 0.3, twist_min: -0.2, twist_max: 0.2
});
sk.add_bone("arm", "spine", 15.0, Vec3::NEG_X, JointConstraint::Hinge {
    axis: Vec3::X, min_angle: 0.0, max_angle: 2.6
});
```

### Pose

```rust
// Set bone rotation
sk.set_rotation("spine", Quat::from_rotation_y(0.5));

// Set hinge angle (for knees, elbows)
sk.set_hinge_angle("shin_l", 0.5);  // 0.5 radians ≈ 29°

// Solve forward kinematics (MUST call after changing rotations)
sk.solve_forward();

// Read world positions
let head_pos = sk.bone("head").world_position;
let hand_pos = sk.bone("hand_r").world_end_position;
```

### Joint Constraints

| Type | Use | Parameters |
|------|-----|-----------|
| `Fixed` | Fused joints (shoulder blade) | None |
| `Free` | Unrestricted (wrist, neck) | None |
| `Hinge` | Single axis (knee, elbow) | axis, min_angle, max_angle |
| `BallSocket` | Multi-axis (shoulder, hip) | cone_angle, twist_min, twist_max |

### Bone Names (Human)

```
pelvis, spine, chest, neck, head,
shoulder_l, upper_arm_l, forearm_l, hand_l,
shoulder_r, upper_arm_r, forearm_r, hand_r,
hip_l, thigh_l, shin_l, foot_l,
hip_r, thigh_r, shin_r, foot_r
```

### Bone Names (Cat)

```
pelvis, spine1, spine2, neck, head, jaw, ear_l, ear_r,
shoulder_l, upper_arm_l, forearm_l, paw_fl,
shoulder_r, upper_arm_r, forearm_r, paw_fr,
hip_l, thigh_l, shin_l, paw_bl,
hip_r, thigh_r, shin_r, paw_br,
tail1, tail2, tail3, tail4
```

---

## Body

### Profiles

```rust
// Simple cylinder (uniform radius along bone)
let arm = BoneProfile::cylinder(bone_id, 4.0, material_id, [r, g, b]);

// Tapered (thick at start, thin at end)
let thigh = BoneProfile::tapered(bone_id, 6.0, 4.0, mat, color);

// Elliptical (different X/Z radii — flat torso)
let chest = BoneProfile::elliptical(bone_id, vec![
    BodySection { t: 0.0, radius_x: 8.0, radius_z: 5.0, offset: Vec2::ZERO },
    BodySection { t: 0.5, radius_x: 10.0, radius_z: 5.5, offset: Vec2::ZERO },
    BodySection { t: 1.0, radius_x: 11.0, radius_z: 5.0, offset: Vec2::ZERO },
], mat, color);
```

### Decals (face details)

```rust
let mut body = BodyDefinition::new();
body.add(chest_profile);
body.add(arm_profile);
// ...

// Add eyes
body.add_decal(head_id, Vec3::new(-1.5, 1.0, -4.0), DecalShape::Sphere(1.2), 1, [50, 200, 50]);
body.add_decal(head_id, Vec3::new(1.5, 1.0, -4.0), DecalShape::Sphere(1.2), 1, [50, 200, 50]);

// Add whiskers
body.add_decal(head_id, Vec3::new(-3.0, 0.0, -4.0), DecalShape::LineH(4.0), 1, [240, 240, 240]);
```

### Decal Shapes

| Shape | Parameters | Use |
|-------|-----------|-----|
| `Point` | None | Mole, freckle |
| `Sphere(r)` | Radius | Eye, nose |
| `LineH(w)` | Width | Whisker, scar, visor |
| `LineV(h)` | Height | Stripe |
| `Ellipse(rx, rz)` | X/Z radii | Patch, marking |

### Rasterize

```rust
body.rasterize(&skeleton, grid_size, |x, y, z, mat, r, g, b| {
    set_voxel(x, y, z, mat, r, g, b);
});
```

---

## IK (Inverse Kinematics)

### Two-Bone IK (legs, arms)

```rust
// Place left foot at target position, knee points forward
ik::apply_leg_ik(&mut skeleton, "thigh_l", "shin_l",
    foot_target,   // Vec3: where foot should be
    pole_target,   // Vec3: direction knee should point
);
skeleton.solve_forward();  // update after IK
```

### Aim IK

```rust
// Make head look at target
ik::aim_bone_at(&mut skeleton, "head", target_pos);

// Turn entire body to face direction (smooth)
ik::turn_skeleton_to(&mut skeleton, target_pos, 0.05); // 0.05 = turn speed
```

---

## Attachments (Weapons, Gear)

### Create

```rust
// Prefab weapons
let ak = weapon_ak(hand_bone_id, 2.0);      // AK-12, scale 2.0
let vikhr = weapon_vikhr(hand_bone_id, 2.0); // Vikhr SMG

// Custom attachment
let mut obj = AttachedObject::new("Knife", hand_bone_id, offset, rotation);
obj.add_segment(start, end, radius, material, color);
obj.add_point("tip", Vec3::new(0.0, 0.0, 10.0), Vec3::Z);
```

### Use

```rust
// Attach to entity
entity.attach(ak);

// After entity.update(), get world positions:
let (muzzle_pos, muzzle_dir) = entity.attachments[0].muzzle().unwrap();
```

### Weapon Points

| Point | Meaning |
|-------|---------|
| `muzzle` | Where bullet exits |
| `grip` | Where right hand holds |
| `support_hand` | Where left hand supports |
| `stock` | Pressed against shoulder |
| `scope` | Where eye looks through |

---

## Entity (High-Level)

### Create

```rust
let mut soldier = Entity::orpp_soldier(2.0);
let mut cat = Entity::cat(2.0);
```

### Actions

```rust
entity.set_position(Vec3::new(128.0, 92.0, 128.0));
entity.look_at(target);        // smooth turn toward target
entity.aim_at(target);         // turn + aim weapon
entity.plant_feet(ground_y);   // IK feet on ground
entity.update();               // solve everything
let bullet = entity.fire();    // shoot from muzzle
entity.rasterize(grid_size, |x,y,z,m,r,g,b| { ... });
```

---

## Procedural Generation

### Generate Room

```rust
let spec = procgen::generate_room(
    RoomType::LivingRoom,  // type
    50.0,                   // size
    0.5,                    // clutter (0.0 = empty, 1.0 = packed)
    12345,                  // seed (same seed = same room)
);

// Inspect furniture
for item in &spec.furniture {
    println!("{}: ${:.0} at {:?}", item.name, item.value, item.position);
}

// Rasterize into grid
procgen::rasterize_room(&spec, 256, offset, scale, |x,y,z,m,r,g,b| { ... });
```

### Room Types

| Type | Required Furniture | Optional |
|------|-------------------|----------|
| `LivingRoom` | Sofa, TV | Coffee table, bookshelf, lamp, vases, books |
| `Kitchen` | Counter, fridge, table | Chairs, appliances |
| `Bedroom` | Bed, nightstand | Wardrobe, desk, lamp |
| `Bathroom` | — | (TODO) |
| `Hallway` | — | (TODO) |
| `Office` | — | (TODO) |

### Seed System

Same seed + same parameters = identical result. Always.

```rust
let room1 = generate_room(RoomType::Kitchen, 40.0, 0.3, 42);
let room2 = generate_room(RoomType::Kitchen, 40.0, 0.3, 42);
assert_eq!(room1.furniture.len(), room2.furniture.len()); // always true
```

---

## Materials

| ID | Name | Typical Use |
|----|------|------------|
| 0 | Empty | Air |
| 1 | Wood | Furniture, floors, stocks |
| 2 | Glass | Windows, visor, eyes |
| 3 | Ceramic | Vases, tiles |
| 4 | Metal | Weapons, handles |
| 5 | Fabric | Clothing, upholstery |
| 6 | Plastic | Electronics, toys |
| 7 | Stone | Walls, floors |
| 8 | Water | Liquids |
| 9 | Paper | Books, photos |
| 10 | Leather/Skin | Skin, boots, belts |
| 13 | Fur | Cat, dog |

---

## Rendering

The engine provides voxel data. You bring your own renderer.
Included: WGSL shader for GPU raymarching (DDA through flat grid).

```rust
// Upload grid to GPU, render fullscreen quad, shader does raymarching
// See src/shader.wgsl and src/main.rs for reference implementation
```

---

*"Code creates worlds. Not me — code."*
*— Lara*
