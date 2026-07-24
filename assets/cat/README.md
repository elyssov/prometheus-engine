# PURRGE Cat Art Contract

Status: `PARTIAL` - first authored voxel sculpture, not the final playable cat.

## Canonical files

- `purrge_cat.blend` is the editable source asset.
- `purrge_cat.glb` is the current engine interchange asset.
- `../../build_purrge_cat.py` rebuilds both files and the beauty render.
- `../../debug/cat_beauty_blender.png` is the current visual proof.

## Visual rules

- Follow the orange tabby concepts in `../../Cat`, not the legacy screenshots.
- Preserve the stepped head silhouette, large green pixel eyes, small torso, four readable paws, and raised striped tail.
- The cat must remain readable at gameplay size and from a three-quarter camera.
- Voxel construction must support animation and selective damage; it is not permission to use one large box per body part.
- Engine-generated brick cats are debug proxies until the GLB asset is imported and rigged.

## Next acceptance gate

- Add a deformation-safe skeleton for head, torso, four legs, and tail.
- Import the GLB into the Rust renderer without changing its silhouette or palette.
- Produce idle, walk, paw-swipe, jump, and grabbed-by-scruff proof poses.
- Approve the cat in a lit apartment shot before expanding destruction gameplay.
