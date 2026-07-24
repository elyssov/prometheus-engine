"""Build the canonical PURRGE cat as an authored Blender game asset."""

from pathlib import Path
import math

import bpy
from mathutils import Vector


ROOT = Path(__file__).resolve().parent
ASSET_DIR = ROOT / "assets" / "cat"
DEBUG_DIR = ROOT / "debug"
ASSET_DIR.mkdir(parents=True, exist_ok=True)
DEBUG_DIR.mkdir(parents=True, exist_ok=True)


def material(name, color, roughness=0.45, metallic=0.0):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*color, 1.0)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    return mat


MATS = {
    "orange": material("Cat Orange", (1.0, 0.25, 0.018), 0.38),
    "gold": material("Cat Gold", (1.0, 0.48, 0.035), 0.36),
    "cream": material("Warm Cream", (1.0, 0.84, 0.64), 0.48),
    "stripe": material("Tabby Stripe", (0.66, 0.075, 0.012), 0.42),
    "pink": material("Nose Pink", (1.0, 0.42, 0.51), 0.34),
    "green": material("Emerald Iris", (0.08, 0.72, 0.24), 0.25),
    "lime": material("Iris Light", (0.32, 1.0, 0.39), 0.20),
    "dark": material("Eye Black", (0.012, 0.016, 0.019), 0.24),
    "white": material("Eye Catchlight", (1.0, 1.0, 0.98), 0.18),
}


def put_in_collection(obj, collection):
    for current in list(obj.users_collection):
        current.objects.unlink(obj)
    collection.objects.link(obj)
    return obj


def rounded_cube(name, location, dimensions, mat, collection, bevel=0.08, rotation=(0, 0, 0)):
    bpy.ops.mesh.primitive_cube_add(location=location, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.dimensions = dimensions
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    modifier = obj.modifiers.new("Planar chamfer", "BEVEL")
    modifier.width = min(bevel, min(dimensions) * 0.22)
    modifier.segments = 1
    modifier.affect = "EDGES"
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=modifier.name)
    obj.data.materials.append(mat)
    return put_in_collection(obj, collection)


def pixel_tile(name, x, y, z, size, depth, mat, collection):
    return rounded_cube(
        name,
        (x, y, z),
        (size * 1.01, depth, size * 1.01),
        mat,
        collection,
        min(0.025, size * 0.08),
    )


def pixel_eye(name, center_x, front_y, center_z, mirror, collection):
    """Build the reference eye as a readable pixel mosaic, not a glass lens."""
    cell = 0.205
    rows = [
        "..CCC..",
        ".CDDDC.",
        "CDGGGDC",
        "CDGGGDC",
        "CDGGGDC",
        "CDGGGDC",
        ".CDDDC.",
        "..CCC..",
    ]
    palette = {"C": MATS["cream"], "D": MATS["dark"], "G": MATS["green"]}
    for row, pattern in enumerate(rows):
        for col, token in enumerate(pattern):
            if token == ".":
                continue
            x = center_x + (col - 3) * cell
            z = center_z + (3.5 - row) * cell
            pixel_tile(f"{name}_{row}_{col}", x, front_y, z, cell, 0.18, palette[token], collection)

    for row in (-1, 0, 1):
        pixel_tile(
            f"{name}_Pupil_{row}", center_x, front_y - 0.105,
            center_z + row * cell, cell, 0.12, MATS["dark"], collection,
        )

    # One square catchlight per eye keeps the gaze lively and directional.
    highlight_x = center_x - mirror * cell
    highlight_z = center_z + 1.5 * cell
    pixel_tile(f"{name}_Catchlight", highlight_x, front_y - 0.105, highlight_z, cell, 0.12, MATS["white"], collection)


def faceted_ellipsoid(name, location, dimensions, mat, collection):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=12, ring_count=6, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = Vector(dimensions) * 0.5
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    for polygon in obj.data.polygons:
        polygon.use_smooth = False
    obj.data.materials.append(mat)
    return put_in_collection(obj, collection)


def triangular_prism(name, center_x, center_y, base_z, tip_z, width, depth, mat, collection):
    x0 = center_x - width / 2
    x1 = center_x + width / 2
    yf = center_y - depth / 2
    yb = center_y + depth / 2
    vertices = [
        (x0, yf, base_z), (x1, yf, base_z), (center_x, yf, tip_z),
        (x0, yb, base_z), (x1, yb, base_z), (center_x, yb, tip_z),
    ]
    faces = [
        (0, 2, 1), (3, 4, 5),
        (0, 1, 4, 3), (1, 2, 5, 4), (2, 0, 3, 5),
    ]
    mesh = bpy.data.meshes.new(f"{name}Mesh")
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    obj.data.materials.append(mat)
    collection.objects.link(obj)
    return obj


def curve_rod(name, points, radius, mat, collection):
    curve = bpy.data.curves.new(name, "CURVE")
    curve.dimensions = "3D"
    curve.resolution_u = 1
    curve.bevel_depth = radius
    curve.bevel_resolution = 0
    spline = curve.splines.new("POLY")
    spline.points.add(len(points) - 1)
    for point, coordinate in zip(spline.points, points):
        point.co = (*coordinate, 1.0)
    obj = bpy.data.objects.new(name, curve)
    obj.data.materials.append(mat)
    collection.objects.link(obj)
    return obj


def look_at(obj, target):
    obj.rotation_euler = (Vector(target) - obj.location).to_track_quat("-Z", "Y").to_euler()


def build_cat():
    cat = bpy.data.collections.new("PURRGE_CAT")
    bpy.context.scene.collection.children.link(cat)

    # The body remains small, but it must still read as a cat rather than a head on feet.
    rounded_cube("Body", (0, 0.42, 1.55), (2.35, 3.65, 2.05), MATS["orange"], cat, 0.12)
    rounded_cube("Chest", (0, -1.46, 1.65), (1.08, 0.16, 1.16), MATS["cream"], cat, 0.035)
    rounded_cube("Belly", (0, 0.18, 0.70), (1.55, 2.25, 0.14), MATS["cream"], cat, 0.025)

    # Four short legs, with feet physically seated on the studio floor.
    for side_x in (-0.72, 0.72):
        for y, tag in ((-0.75, "Front"), (1.10, "Back")):
            rounded_cube(f"{tag}Leg_{side_x:+.0f}", (side_x, y, 0.55), (0.58, 0.68, 1.22), MATS["orange"], cat, 0.12)
            rounded_cube(f"{tag}Paw_{side_x:+.0f}", (side_x, y - 0.12, 0.10), (0.76, 0.96, 0.38), MATS["cream"], cat, 0.10)

    # Layered square masses form the stepped skull from the painted reference.
    rounded_cube("HeadCore", (0, -1.24, 3.48), (3.72, 2.32, 2.52), MATS["orange"], cat, 0.09)
    rounded_cube("HeadCrown", (0, -1.18, 4.72), (3.06, 2.12, 0.44), MATS["gold"], cat, 0.06)
    rounded_cube("HeadTempleL", (-1.78, -1.26, 3.68), (0.42, 2.22, 1.70), MATS["orange"], cat, 0.05)
    rounded_cube("HeadTempleR", (1.78, -1.26, 3.68), (0.42, 2.22, 1.70), MATS["orange"], cat, 0.05)
    rounded_cube("HeadCheeks", (0, -1.40, 2.64), (3.42, 2.16, 0.66), MATS["orange"], cat, 0.08)
    rounded_cube("Chin", (0, -2.48, 2.24), (1.62, 0.22, 0.24), MATS["cream"], cat, 0.025)

    # Ears keep triangular silhouettes, with pixel steps layered over the inner slopes.
    for side in (-1, 1):
        x = side * 1.30
        triangular_prism(f"EarOuter_{side}", x, -1.18, 4.66, 6.04, 1.42, 1.14, MATS["orange"], cat)
        triangular_prism(f"EarInner_{side}", x, -1.77, 4.80, 5.72, 0.76, 0.07, MATS["cream"], cat)

    # Pixel mosaics carry the face. Their planar construction survives game-scale rendering.
    pixel_eye("EyeL", -0.88, -2.485, 3.60, -1, cat)
    pixel_eye("EyeR", 0.88, -2.485, 3.60, 1, cat)

    rounded_cube("MuzzleL", (-0.29, -2.68, 2.61), (0.66, 0.20, 0.43), MATS["cream"], cat, 0.04)
    rounded_cube("MuzzleR", (0.29, -2.68, 2.61), (0.66, 0.20, 0.43), MATS["cream"], cat, 0.04)
    rounded_cube("Nose", (0, -2.83, 2.70), (0.34, 0.18, 0.25), MATS["pink"], cat, 0.035)
    rounded_cube("MouthStem", (0, -2.80, 2.45), (0.08, 0.10, 0.18), MATS["stripe"], cat, 0.012)
    rounded_cube("MouthL", (-0.11, -2.81, 2.36), (0.24, 0.10, 0.08), MATS["stripe"], cat, 0.012)
    rounded_cube("MouthR", (0.11, -2.81, 2.36), (0.24, 0.10, 0.08), MATS["stripe"], cat, 0.012)

    # Sparse tabby marks keep the palette authored instead of noisy.
    for x in (-0.66, 0.0, 0.66):
        rounded_cube("ForeheadStripe", (x, -2.43, 4.62), (0.22, 0.12, 0.52), MATS["stripe"], cat, 0.025)
    for y in (-0.15, 0.62, 1.36):
        rounded_cube("BackStripe", (0, y, 2.52), (0.82, 0.36, 0.12), MATS["stripe"], cat, 0.05)

    # A segmented curl gives the tail a clear animation-friendly construction.
    tail = [
        ((1.30, 1.55, 1.65), (0.62, 0.72, 0.62), 0.20),
        ((1.65, 1.82, 2.15), (0.58, 0.65, 0.75), 0.45),
        ((1.88, 1.93, 2.78), (0.52, 0.58, 0.78), 0.72),
        ((1.78, 1.92, 3.42), (0.48, 0.54, 0.72), 1.02),
    ]
    for index, (location, dimensions, angle) in enumerate(tail):
        rounded_cube(
            f"Tail_{index}", location, dimensions,
            MATS["stripe"] if index % 2 else MATS["orange"], cat, 0.14,
            (angle, 0, 0.18),
        )
    rounded_cube("TailTip", (1.58, 1.78, 3.92), (0.50, 0.50, 0.58), MATS["cream"], cat, 0.14)
    return cat


def build_voxel_cat():
    """Build the canonical cat as one crisp, reference-led voxel sculpture."""
    cat = bpy.data.collections.new("PURRGE_CAT")
    bpy.context.scene.collection.children.link(cat)
    voxels = {}

    def add(ix, iy, iz, material_name="orange"):
        voxels[(ix, iy, iz)] = material_name

    # Upright neutral body from the canonical front design. The rig will fold it
    # into quadruped locomotion later; this is one character, not two anatomies.
    torso_widths = [3, 4, 4, 4, 4, 4, 3, 3]
    for row, half_width in enumerate(torso_widths):
        iz = -8 + row
        for ix in range(-half_width, half_width + 1):
            for iy in range(-1, 6):
                add(ix, iy, iz)
    for iz in range(-7, -1):
        half_width = 2 if iz in (-6, -5, -4, -3) else 1
        for ix in range(-half_width, half_width + 1):
            add(ix, -2, iz, "cream")

    # Short legs and broad cream paws.
    for cx in (-3, 3):
        for ix in range(cx - 1, cx + 2):
            for iy in range(-1, 3):
                for iz in range(-12, -8):
                    add(ix, iy, iz, "cream" if iz == -12 else "orange")

    # Downward arms give the neutral model the same toy-like biped silhouette.
    for side in (-1, 1):
        arm_path = [
            (side * 5, 0, -2), (side * 5, 0, -3),
            (side * 6, 0, -4), (side * 6, 0, -5), (side * 7, 0, -6),
        ]
        for index, (cx, cy, cz) in enumerate(arm_path):
            if index == len(arm_path) - 1:
                xs = (cx - side, cx)
                ys = (cy - 1, cy)
                zs = (cz - 1, cz)
            else:
                xs = range(cx - 1, cx + 2)
                ys = range(cy - 1, cy + 2)
                zs = range(cz - 1, cz + 2)
            for ix in xs:
                for iy in ys:
                    for iz in zs:
                        add(ix, iy, iz, "cream" if index == len(arm_path) - 1 else ("stripe" if index in (2, 3) else "orange"))

    # Stepped round-square head silhouette. The depth is intentionally shallow.
    widths = [6, 7, 8, 8, 8, 8, 8, 8, 8, 7, 6]
    for iz, half_width in enumerate(widths):
        for ix in range(-half_width, half_width + 1):
            for iy in range(-4, 5):
                add(ix, iy, iz, "orange")

    # Triangular voxel ears with inset cream centers.
    for side in (-1, 1):
        center = side * 5
        ear_widths = (3, 2, 2, 1, 0)
        for level, half_width in enumerate(ear_widths):
            for ix in range(center - half_width, center + half_width + 1):
                for iy in range(-3, 4):
                    add(ix, iy, 11 + level, "orange")
            if 1 <= level <= 3 and half_width > 0:
                for ix in range(center - half_width + 1, center + half_width):
                    add(ix, -4, 11 + level, "pink")

    # Tabby markings on the forehead, back, and visible side.
    for ix in (-3, 0, 3):
        add(ix, -5, 10, "stripe")
        add(ix, -5, 9, "stripe")
    for iy in (2, 5, 8):
        for ix in range(-2, 3):
            add(ix, iy, 0, "stripe")
        for iz in range(-6, -2):
            add(6, iy, iz, "stripe")

    # Face mosaics. These sit one voxel in front of the orange skull.
    eye_rows = [
        "..CCC..",
        ".CDDDC.",
        "CDGGGDC",
        "CDGGGDC",
        "CDGGGDC",
        "CDGGGDC",
        ".CDDDC.",
        "..CCC..",
    ]
    for eye_center, mirror in ((-5, -1), (5, 1)):
        for row, pattern in enumerate(eye_rows):
            iz = 9 - row
            for col, token in enumerate(pattern):
                if token == ".":
                    continue
                add(eye_center + col - 3, -5, iz, {"C": "cream", "D": "dark", "G": "green"}[token])
        for iz in (4, 5, 6):
            add(eye_center, -6, iz, "dark")
        add(eye_center - mirror, -6, 7, "white")

    # Broad stepped mask is a defining feature of the designed face.
    muzzle_widths = {0: 5, 1: 6, 2: 4}
    for iz, half_width in muzzle_widths.items():
        for ix in range(-half_width, half_width + 1):
            add(ix, -5, iz, "cream")
    add(0, -6, 2, "pink")
    add(0, -6, 1, "stripe")
    add(-1, -6, 0, "stripe")
    add(1, -6, 0, "stripe")

    # A raised striped tail is assembled along a readable arc.
    tail_path = [
        (4, 5, -7), (5, 6, -6), (6, 7, -5), (7, 7, -3),
        (8, 7, -1), (8, 7, 1), (7, 7, 3), (6, 7, 4),
    ]
    for index, (cx, cy, cz) in enumerate(tail_path):
        for ix in range(cx - 1, cx + 2):
            for iy in range(cy - 1, cy + 2):
                for iz in range(cz - 1, cz + 2):
                    add(ix, iy, iz, "cream" if index == len(tail_path) - 1 else ("stripe" if index % 2 else "orange"))

    # The concept art reads at roughly 30-36 voxels across the head. Authoring
    # happens on a compact logical grid, then expands to the canonical 2x grid.
    resolution_scale = 2
    logical_voxels = voxels
    voxels = {}
    for (ix, iy, iz), material_name in logical_voxels.items():
        for dx in range(resolution_scale):
            for dy in range(resolution_scale):
                for dz in range(resolution_scale):
                    voxels[(
                        ix * resolution_scale + dx,
                        iy * resolution_scale + dy,
                        iz * resolution_scale + dz,
                    )] = material_name

    cell = 0.235 / resolution_scale
    origin = Vector((0.0, -1.30, 3.00)) - Vector((cell * 0.5, cell * 0.5, cell * 0.5))
    inset = cell * 0.965
    vertices = []
    faces = []
    face_materials = []
    corners = [
        (-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1),
        (-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1),
    ]
    cube_faces = [
        (0, 1, 2, 3), (4, 7, 6, 5), (0, 4, 5, 1),
        (1, 5, 6, 2), (2, 6, 7, 3), (4, 0, 3, 7),
    ]
    material_names = list(MATS.keys())
    material_indices = {name: index for index, name in enumerate(material_names)}
    for (ix, iy, iz), material_name in voxels.items():
        center = origin + Vector((ix * cell, iy * cell, iz * cell))
        base = len(vertices)
        for sx, sy, sz in corners:
            vertices.append((center.x + sx * inset * 0.5, center.y + sy * inset * 0.5, center.z + sz * inset * 0.5))
        for face in cube_faces:
            faces.append(tuple(base + corner for corner in face))
            face_materials.append(material_indices[material_name])

    mesh = bpy.data.meshes.new("PurrgeCatVoxelMesh")
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    obj = bpy.data.objects.new("PurrgeCat", mesh)
    cat.objects.link(obj)
    for name in material_names:
        obj.data.materials.append(MATS[name])
    for polygon, material_index in zip(obj.data.polygons, face_materials):
        polygon.material_index = material_index
    return cat


def build_stage():
    stage = bpy.data.collections.new("BEAUTY_STAGE")
    bpy.context.scene.collection.children.link(stage)
    floor_mat = material("Studio Floor", (0.72, 0.78, 0.83), 0.72)
    rounded_cube("StudioFloor", (0, 0, -0.18), (18, 18, 0.30), floor_mat, stage, 0.04)

    world = bpy.context.scene.world
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.18, 0.23, 0.30, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.32

    def area(name, location, energy, size, color):
        data = bpy.data.lights.new(name, "AREA")
        data.energy = energy
        data.shape = "DISK"
        data.size = size
        data.color = color
        obj = bpy.data.objects.new(name, data)
        stage.objects.link(obj)
        obj.location = location
        look_at(obj, (0, -0.8, 2.5))

    area("Key", (-5.5, -6.0, 8.5), 950, 5.0, (1.0, 0.90, 0.80))
    area("Fill", (5.5, -3.0, 5.0), 620, 4.0, (0.68, 0.80, 1.0))
    area("Rim", (2.5, 5.0, 7.0), 760, 3.5, (1.0, 0.62, 0.32))

    camera_data = bpy.data.cameras.new("BeautyCamera")
    camera = bpy.data.objects.new("BeautyCamera", camera_data)
    stage.objects.link(camera)
    camera.location = (2.4, -18.5, 5.2)
    camera_data.lens = 64
    look_at(camera, (0, -0.55, 2.62))
    bpy.context.scene.camera = camera


def configure_and_export(cat_collection):
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x = 1280
    scene.render.resolution_y = 1280
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = str(DEBUG_DIR / "cat_beauty_blender.png")
    scene.render.film_transparent = False
    scene.view_settings.look = "AgX - Medium High Contrast"
    scene.render.image_settings.color_mode = "RGBA"

    bpy.ops.wm.save_as_mainfile(filepath=str(ASSET_DIR / "purrge_cat.blend"))
    bpy.ops.object.select_all(action="DESELECT")
    for obj in cat_collection.objects:
        obj.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=str(ASSET_DIR / "purrge_cat.glb"),
        export_format="GLB",
        use_selection=True,
        export_apply=True,
    )
    bpy.ops.render.render(write_still=True)


def main():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for collection in list(bpy.data.collections):
        bpy.data.collections.remove(collection)
    cat = build_voxel_cat()
    build_stage()
    configure_and_export(cat)
    print(f"PURRGE cat saved to {ASSET_DIR}")


main()
