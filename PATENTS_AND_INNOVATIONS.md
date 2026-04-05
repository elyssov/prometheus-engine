# Prometheus Engine — Prior Art & Innovations
## Documented: April 5, 2026
## Author: Eugene Lyssovsky
## Developed with AI assistance (Claude/Anthropic)

This document establishes prior art for the following innovations.
All concepts described here were developed by the authors on the dates indicated.

---

## Innovation 1: Procedural Formula World (April 4-5, 2026)
**Concept:** Game worlds stored as text formulas (TOML/YAML), not voxel arrays. 
Entire cities described in kilobytes. Generated on-demand by CPU.
Only visible region materialized. Destruction stored as sparse diff to formulas.

## Innovation 2: Dual Representation Architecture (April 5, 2026)
**Concept:** Voxels for DATA (physics, generation, destruction), polygons for RENDER
(via Marching Cubes / Dual Contouring). Best of both worlds: full destructibility 
of voxels + full visual quality of polygon rendering.

## Innovation 3: Adaptive Multi-Tier Backend (April 4-5, 2026)
**Concept:** Same game automatically adapts to hardware:
Software (CPU-only) → Integrated GPU → Discrete GPU → RTX.
Same world formulas, different visual quality. Like Quake '96 software vs Voodoo.

## Innovation 4: Streaming Procedural Chunks (April 5, 2026)
**Concept:** 64³ voxel chunks generated on-demand from formulas, cached in RAM,
evicted when out of view. World is infinite. Memory is finite.
Predictive prefetch based on camera velocity extrapolation.

## Innovation 5: Voксельный Upscaling ("Voxel DLSS") (April 5, 2026)
**Concept:** Integer-based upscaling of voxel grids. No floating point.
Render at low resolution (128³), upscale to high (256³+) via trilinear
interpolation on integer grid. No temporal reprojection needed (grid is stable).
Works on any hardware including CPU-only.

## Innovation 6: Layer Stack Character System (April 4, 2026)
**Concept:** Characters built as stack of layers on skeleton:
Skeleton → Body (profiles) → Clothing → Armor → Gear → Weapons.
Each layer = separate asset. Combinatorial: M bodies × N clothes × K weapons.

## Innovation 7: Decal System for Face/Body Details (April 4-5, 2026)
**Concept:** Eyes, nose, whiskers, scars, visor placed as "decals" on skeleton bones.
Move with bone automatically. Rendered on top of body profiles.

## Innovation 8: Photo-to-Voxel Converter (April 4, 2026)
**Concept:** Upload photo → place joint markers → engine generates voxel character.
Colors sampled from photo pixels. Proportions from marker distances.

## Innovation 9: Hollow Shell Rendering (April 5, 2026)
**Concept:** Only render outer shell of voxel objects (2-3 voxels thick).
Interior empty. 94% voxel savings for solid objects.
Interior generated only when object is broken open.

## Innovation 10: Formula-Based Save/Load (April 5, 2026)
**Concept:** Save file = world seed + player state + destruction diff.
Kilobytes, not gigabytes. World regenerated from seed on load.

---

All innovations documented in public git repository:
https://github.com/elyssov/prometheus-engine

Commit history serves as timestamped proof of creation.

© 2026 Eugene Lyssovsky. All rights reserved.
Developed with AI assistance (Claude/Anthropic).
