// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Apartment: П-44 typical 3-room, ~72 m²
//
// Layout (top-down, Y up, Z forward, X right):
//
//    z=800 ┌── Bedroom 1 ──┬── Corridor ──┬── Living ────┐
//          │  380×350 cm   │  140×800 cm  │  380×350 cm  │
//          │  13.3 m²      │  11.2 m²     │  13.3 m²     │
//    z=450 ├──D────────────┤              ├────────D─────┤
//          │  Bedroom 2    │              │  Kitchen     │
//          │  380×250 cm   │              │  380×250 cm  │
//          │  9.5 m²       │              │  9.5 m²      │
//    z=200 ├──D────────────┤              ├────────D─────┤
//          │  Bath + WC    │              │  Entry hall  │
//          │  380×200 cm   │              │  380×200 cm  │
//          │  7.6 m²       │              │  7.6 m²      │
//    z=0   └──D────────────┴──[ENTRY]─────┴──────────────┘
//          x=0           x=380          x=520          x=900
//
// Central corridor runs front → back; rooms hang off left and right.
// T-junction walls only — no central cross.  No ceiling (cutaway).
// ═══════════════════════════════════════════════════════════════

use glam::Vec3;
use super::brick::{Brick, BrickModel};
use super::damage::Durability;

mod palette {
    pub const FLOOR_WOOD:    [u8; 3] = [195, 150, 100];
    pub const FLOOR_TILE:    [u8; 3] = [220, 220, 225];
    pub const FLOOR_CARPET:  [u8; 3] = [125, 85, 80];
    pub const FLOOR_BATH:    [u8; 3] = [180, 200, 215];
    pub const WALL:          [u8; 3] = [236, 222, 200];
    pub const WALL_KITCHEN:  [u8; 3] = [215, 230, 220];
    pub const WALL_BEDROOM:  [u8; 3] = [220, 215, 235];
    pub const WALL_BATH:     [u8; 3] = [195, 215, 225];
    pub const DOOR_FRAME:    [u8; 3] = [135, 90, 55];

    pub const WOOD_LIGHT:    [u8; 3] = [175, 135, 90];
    pub const WOOD_DARK:     [u8; 3] = [105, 65, 35];
    pub const METAL:         [u8; 3] = [200, 200, 210];
    pub const FRIDGE:        [u8; 3] = [235, 245, 240];
    pub const STOVE:         [u8; 3] = [70, 72, 78];
    pub const BLACK:         [u8; 3] = [28, 28, 34];
    pub const TV_SCREEN:     [u8; 3] = [18, 25, 45];
    pub const LAMP_SHADE:    [u8; 3] = [250, 230, 175];
    pub const SOFA_BLUE:     [u8; 3] = [90, 125, 200];
    pub const SOFA_CUSH:     [u8; 3] = [115, 150, 225];
    pub const BED_LINEN:     [u8; 3] = [200, 215, 235];
    pub const BED_PILLOW:    [u8; 3] = [250, 245, 230];
    pub const BED_BLANKET_A: [u8; 3] = [95, 130, 205];
    pub const BED_BLANKET_B: [u8; 3] = [205, 115, 145];
    pub const PLANT_LEAF:    [u8; 3] = [80, 155, 85];
    pub const POT_TERRA:     [u8; 3] = [175, 90, 65];
    pub const RUG_RED:       [u8; 3] = [165, 70, 80];
    pub const BOOK_A:        [u8; 3] = [55, 75, 135];
    pub const BOOK_B:        [u8; 3] = [155, 48, 48];
    pub const BOOK_C:        [u8; 3] = [48, 125, 55];
    pub const BOOK_D:        [u8; 3] = [180, 160, 50];
    pub const CHAIR_RED:     [u8; 3] = [190, 85, 85];
    pub const CHAIR_GREEN:   [u8; 3] = [85, 175, 110];
    pub const CHAIR_YEL:     [u8; 3] = [220, 195, 80];
    pub const CHAIR_TEAL:    [u8; 3] = [85, 170, 185];
    pub const PORCELAIN:     [u8; 3] = [248, 248, 248];
    pub const TILE_LIGHT:    [u8; 3] = [215, 225, 230];
}

// ── Apartment geometry constants (cm) ────────────────────────
pub const X_MIN: f32 =   0.0;
pub const X_MAX: f32 = 900.0;
pub const Z_MIN: f32 =   0.0;
pub const Z_MAX: f32 = 800.0;
pub const CEILING_Y: f32 = 250.0;
pub const WALL_THICK: f32 = 10.0;
pub const DOOR_HEIGHT: f32 = 200.0;

// Corridor boundaries (interior walls on x=380 and x=520)
const WEST_CORR: f32 = 380.0;
const EAST_CORR: f32 = 520.0;
// Left-side horizontal room dividers
const LEFT_Z_1: f32 = 200.0;  // bath  | bed-2
const LEFT_Z_2: f32 = 450.0;  // bed-2 | bed-1
// Right-side horizontal room dividers
const RIGHT_Z_1: f32 = 200.0; // entry | kitchen
const RIGHT_Z_2: f32 = 450.0; // kitchen | living

/// Build the П-44 apartment as a single static BrickModel.
pub fn build_apartment() -> BrickModel {
    let mut m = BrickModel::new("Apartment_P44");

    add_floors(&mut m);
    add_outer_walls(&mut m);
    add_corridor_walls(&mut m);
    add_left_dividers(&mut m);
    add_right_dividers(&mut m);
    add_wall_tints(&mut m);

    add_bedroom_1(&mut m);
    add_bedroom_2(&mut m);
    add_bathroom(&mut m);
    add_living_room(&mut m);
    close_some_doors(&mut m);
    add_kitchen(&mut m);
    add_entry_hall(&mut m);
    add_corridor(&mut m);

    m
}

// Axis-aligned box helper
fn box_xyz(m: &mut BrickModel, name: &str, min: Vec3, max: Vec3, color: [u8; 3]) {
    let center = (min + max) * 0.5;
    let half = (max - min) * 0.5;
    m.add(Brick::new(name, half, color).with_position(center));
}

// ═══════════════════════════════════════════════════════════════
// FLOORS — one slab per room, different materials
// ═══════════════════════════════════════════════════════════════

fn add_floors(m: &mut BrickModel) {
    use palette::*;
    // Left side
    box_xyz(m, "fl_br1",  Vec3::new(X_MIN, -WALL_THICK, LEFT_Z_2),
                          Vec3::new(WEST_CORR, 0.0, Z_MAX), FLOOR_CARPET);
    box_xyz(m, "fl_br2",  Vec3::new(X_MIN, -WALL_THICK, LEFT_Z_1),
                          Vec3::new(WEST_CORR, 0.0, LEFT_Z_2), FLOOR_CARPET);
    box_xyz(m, "fl_bath", Vec3::new(X_MIN, -WALL_THICK, Z_MIN),
                          Vec3::new(WEST_CORR, 0.0, LEFT_Z_1), FLOOR_BATH);
    // Corridor
    box_xyz(m, "fl_corr", Vec3::new(WEST_CORR, -WALL_THICK, Z_MIN),
                          Vec3::new(EAST_CORR, 0.0, Z_MAX), FLOOR_WOOD);
    // Right side
    box_xyz(m, "fl_liv",  Vec3::new(EAST_CORR, -WALL_THICK, RIGHT_Z_2),
                          Vec3::new(X_MAX, 0.0, Z_MAX), FLOOR_WOOD);
    box_xyz(m, "fl_kit",  Vec3::new(EAST_CORR, -WALL_THICK, RIGHT_Z_1),
                          Vec3::new(X_MAX, 0.0, RIGHT_Z_2), FLOOR_TILE);
    box_xyz(m, "fl_entry",Vec3::new(EAST_CORR, -WALL_THICK, Z_MIN),
                          Vec3::new(X_MAX, 0.0, RIGHT_Z_1), FLOOR_WOOD);
}

// ═══════════════════════════════════════════════════════════════
// OUTER WALLS — with entry door in the front wall
// ═══════════════════════════════════════════════════════════════

fn add_outer_walls(m: &mut BrickModel) {
    use palette::*;

    // West exterior (x = X_MIN)
    box_xyz(m, "ow_west",
        Vec3::new(X_MIN - WALL_THICK, 0.0, Z_MIN),
        Vec3::new(X_MIN, CEILING_Y, Z_MAX), WALL);
    // East exterior
    box_xyz(m, "ow_east",
        Vec3::new(X_MAX, 0.0, Z_MIN),
        Vec3::new(X_MAX + WALL_THICK, CEILING_Y, Z_MAX), WALL);
    // Back (z = Z_MAX)
    box_xyz(m, "ow_back",
        Vec3::new(X_MIN - WALL_THICK, 0.0, Z_MAX),
        Vec3::new(X_MAX + WALL_THICK, CEILING_Y, Z_MAX + WALL_THICK), WALL);
    // Front with entry door in the corridor (x = 420..480, z=0)
    let ex0 = 420.0_f32;
    let ex1 = 480.0;
    // Left segment
    box_xyz(m, "ow_front_l",
        Vec3::new(X_MIN - WALL_THICK, 0.0, Z_MIN - WALL_THICK),
        Vec3::new(ex0, CEILING_Y, Z_MIN), WALL);
    // Right segment
    box_xyz(m, "ow_front_r",
        Vec3::new(ex1, 0.0, Z_MIN - WALL_THICK),
        Vec3::new(X_MAX + WALL_THICK, CEILING_Y, Z_MIN), WALL);
    // Lintel
    box_xyz(m, "ow_front_lintel",
        Vec3::new(ex0, DOOR_HEIGHT, Z_MIN - WALL_THICK),
        Vec3::new(ex1, CEILING_Y, Z_MIN), WALL);
    // Entry door frame
    box_xyz(m, "entry_frame_l",
        Vec3::new(ex0 - 4.0, 0.0, Z_MIN - WALL_THICK - 1.0),
        Vec3::new(ex0, DOOR_HEIGHT + 4.0, Z_MIN + 1.0), DOOR_FRAME);
    box_xyz(m, "entry_frame_r",
        Vec3::new(ex1, 0.0, Z_MIN - WALL_THICK - 1.0),
        Vec3::new(ex1 + 4.0, DOOR_HEIGHT + 4.0, Z_MIN + 1.0), DOOR_FRAME);
    box_xyz(m, "entry_frame_t",
        Vec3::new(ex0 - 4.0, DOOR_HEIGHT, Z_MIN - WALL_THICK - 1.0),
        Vec3::new(ex1 + 4.0, DOOR_HEIGHT + 4.0, Z_MIN + 1.0), DOOR_FRAME);
}

// ═══════════════════════════════════════════════════════════════
// CORRIDOR WALLS — two long vertical walls, each with 3 doors
// ═══════════════════════════════════════════════════════════════

fn add_corridor_walls(m: &mut BrickModel) {
    // West corridor wall at x = WEST_CORR — doors to:
    //   bathroom  z = 80..160
    //   bedroom-2 z = 300..380
    //   bedroom-1 z = 600..680
    add_wall_along_z(m, WEST_CORR,
        &[(80.0, 160.0), (300.0, 380.0), (600.0, 680.0)],
        "west_corr");
    // East corridor wall at x = EAST_CORR — doors to:
    //   entry-hall z = 80..160   (short-cut from hall into corridor)
    //   kitchen    z = 300..380
    //   living     z = 600..680
    add_wall_along_z(m, EAST_CORR,
        &[(80.0, 160.0), (300.0, 380.0), (600.0, 680.0)],
        "east_corr");
}

/// A wall segment running along Z at a given X, from Z_MIN to Z_MAX,
/// with multiple door gaps (each (z0, z1)).  Adds wall segments, lintels
/// and door frames in one go.
fn add_wall_along_z(m: &mut BrickModel, x_centre: f32,
                    gaps: &[(f32, f32)], label: &str)
{
    use palette::*;
    let x0 = x_centre - WALL_THICK * 0.5;
    let x1 = x_centre + WALL_THICK * 0.5;

    let mut cursor = Z_MIN;
    for (gi, (gz0, gz1)) in gaps.iter().enumerate() {
        // Wall segment from cursor to gap start
        if *gz0 > cursor {
            box_xyz(m, &format!("{}_seg{}", label, gi),
                Vec3::new(x0, 0.0, cursor),
                Vec3::new(x1, CEILING_Y, *gz0), WALL);
        }
        // Lintel above the door
        box_xyz(m, &format!("{}_lintel{}", label, gi),
            Vec3::new(x0, DOOR_HEIGHT, *gz0),
            Vec3::new(x1, CEILING_Y, *gz1), WALL);
        // Door frame
        box_xyz(m, &format!("{}_frame_l{}", label, gi),
            Vec3::new(x0 - 1.0, 0.0, *gz0 - 3.0),
            Vec3::new(x1 + 1.0, DOOR_HEIGHT + 3.0, *gz0), DOOR_FRAME);
        box_xyz(m, &format!("{}_frame_r{}", label, gi),
            Vec3::new(x0 - 1.0, 0.0, *gz1),
            Vec3::new(x1 + 1.0, DOOR_HEIGHT + 3.0, *gz1 + 3.0), DOOR_FRAME);
        box_xyz(m, &format!("{}_frame_t{}", label, gi),
            Vec3::new(x0 - 1.0, DOOR_HEIGHT, *gz0 - 3.0),
            Vec3::new(x1 + 1.0, DOOR_HEIGHT + 3.0, *gz1 + 3.0), DOOR_FRAME);
        cursor = *gz1;
    }
    // Final segment from last gap end to Z_MAX
    if cursor < Z_MAX {
        box_xyz(m, &format!("{}_tail", label),
            Vec3::new(x0, 0.0, cursor),
            Vec3::new(x1, CEILING_Y, Z_MAX), WALL);
    }
}

// ═══════════════════════════════════════════════════════════════
// CLOSED DOORS — three of the six corridor doorways are boarded
// shut.  Each door is a breakable wooden plank filling the gap.
// The cat has to smack it ~5 times to bust through.
//
// Closed: bath (west, z=80..160), kitchen (east, z=300..380),
//         bedroom-1 (west, z=600..680).
// Open:   entry (east, z=80..160), bedroom-2 (west, z=300..380),
//         living (east, z=600..680).
// ═══════════════════════════════════════════════════════════════

fn close_some_doors(m: &mut BrickModel) {
    use palette::WOOD_DARK;
    let dur = Durability::new(5.0, 0.6, 0.3);
    let door_half_thick = WALL_THICK * 0.5 + 1.0; // slightly proud of wall
    let door_half_height = DOOR_HEIGHT * 0.5;
    let door_half_z = 40.0;                        // gap is 80cm wide

    // Bath (west wall, z=80..160)
    m.add(Brick::new("door_bath",
        Vec3::new(door_half_thick, door_half_height, door_half_z), WOOD_DARK)
        .with_position(Vec3::new(WEST_CORR, door_half_height, 120.0))
        .with_durability(dur));

    // Kitchen (east wall, z=300..380)
    m.add(Brick::new("door_kitchen",
        Vec3::new(door_half_thick, door_half_height, door_half_z), WOOD_DARK)
        .with_position(Vec3::new(EAST_CORR, door_half_height, 340.0))
        .with_durability(dur));

    // Bedroom-1 (west wall, z=600..680)
    m.add(Brick::new("door_bedroom1",
        Vec3::new(door_half_thick, door_half_height, door_half_z), WOOD_DARK)
        .with_position(Vec3::new(WEST_CORR, door_half_height, 640.0))
        .with_durability(dur));
}

// ═══════════════════════════════════════════════════════════════
// LEFT / RIGHT room dividers (perpendicular to corridor, butting
// against the corridor wall — T-junctions, no crosses)
// ═══════════════════════════════════════════════════════════════

fn add_left_dividers(m: &mut BrickModel) {
    use palette::*;
    // Horizontal wall at z = LEFT_Z_1 (bath | bedroom-2), no door
    let z0 = LEFT_Z_1 - WALL_THICK * 0.5;
    let z1 = LEFT_Z_1 + WALL_THICK * 0.5;
    box_xyz(m, "div_left_1",
        Vec3::new(X_MIN, 0.0, z0),
        Vec3::new(WEST_CORR - WALL_THICK * 0.5, CEILING_Y, z1), WALL);
    // Horizontal wall at z = LEFT_Z_2 (bedroom-2 | bedroom-1)
    let z0 = LEFT_Z_2 - WALL_THICK * 0.5;
    let z1 = LEFT_Z_2 + WALL_THICK * 0.5;
    box_xyz(m, "div_left_2",
        Vec3::new(X_MIN, 0.0, z0),
        Vec3::new(WEST_CORR - WALL_THICK * 0.5, CEILING_Y, z1), WALL);
}

fn add_right_dividers(m: &mut BrickModel) {
    use palette::*;
    let z0 = RIGHT_Z_1 - WALL_THICK * 0.5;
    let z1 = RIGHT_Z_1 + WALL_THICK * 0.5;
    box_xyz(m, "div_right_1",
        Vec3::new(EAST_CORR + WALL_THICK * 0.5, 0.0, z0),
        Vec3::new(X_MAX, CEILING_Y, z1), WALL);
    let z0 = RIGHT_Z_2 - WALL_THICK * 0.5;
    let z1 = RIGHT_Z_2 + WALL_THICK * 0.5;
    box_xyz(m, "div_right_2",
        Vec3::new(EAST_CORR + WALL_THICK * 0.5, 0.0, z0),
        Vec3::new(X_MAX, CEILING_Y, z1), WALL);
}

// Thin coloured tints on the inside of each room's walls — makes each
// zone feel different colour without drowning the palette.
fn add_wall_tints(m: &mut BrickModel) {
    use palette::*;
    // Kitchen (right middle) — mint tint
    let xw = EAST_CORR + WALL_THICK * 0.5 + 1.0;  // inner face
    box_xyz(m, "tint_kit_w",
        Vec3::new(xw, 0.0, RIGHT_Z_1 + 1.0),
        Vec3::new(xw + 1.0, CEILING_Y, RIGHT_Z_2 - 1.0), WALL_KITCHEN);
    // Bathroom — blueish tint
    let xw = WEST_CORR - WALL_THICK * 0.5 - 1.0;
    box_xyz(m, "tint_bath_e",
        Vec3::new(xw - 1.0, 0.0, 1.0),
        Vec3::new(xw, CEILING_Y, LEFT_Z_1 - 1.0), WALL_BATH);
    // Bedroom 1 — lavender tint
    box_xyz(m, "tint_br1_e",
        Vec3::new(xw - 1.0, 0.0, LEFT_Z_2 + 1.0),
        Vec3::new(xw, CEILING_Y, Z_MAX - 1.0), WALL_BEDROOM);
}

// ═══════════════════════════════════════════════════════════════
// CORRIDOR — runner rug, coat rack, lamp
// ═══════════════════════════════════════════════════════════════

fn add_corridor(m: &mut BrickModel) {
    use palette::*;
    let xc = (WEST_CORR + EAST_CORR) * 0.5;
    // Runner rug
    box_xyz(m, "corr_rug",
        Vec3::new(xc - 40.0, 0.0, 30.0),
        Vec3::new(xc + 40.0, 2.0, Z_MAX - 30.0), RUG_RED);
    // Ceiling lamp mid corridor
    box_xyz(m, "corr_lamp",
        Vec3::new(xc - 15.0, 220.0, 400.0),
        Vec3::new(xc + 15.0, CEILING_Y, 430.0), LAMP_SHADE);
}

// ═══════════════════════════════════════════════════════════════
// BEDROOM 1 (big) — x [0, 380], z [450, 800] — 13.3 m²
// ═══════════════════════════════════════════════════════════════

fn add_bedroom_1(m: &mut BrickModel) {
    use palette::*;
    const X0: f32 = 0.0;
    const X1: f32 = 380.0;
    const Z0: f32 = 450.0;
    const Z1: f32 = 800.0;

    // Double bed against back wall (Z1 side), width 160 cm
    let bx0 = 100.0; let bx1 = 260.0;            // 160 wide
    let bz1 = Z1 - 15.0; let bz0 = bz1 - 200.0;  // 200 long
    box_xyz(m, "br1_frame", Vec3::new(bx0, 0.0, bz0), Vec3::new(bx1, 25.0, bz1), WOOD_DARK);
    box_xyz(m, "br1_mat",   Vec3::new(bx0+5.0, 25.0, bz0+5.0),
                            Vec3::new(bx1-5.0, 50.0, bz1-5.0), BED_LINEN);
    box_xyz(m, "br1_pill_l", Vec3::new(bx0+10.0, 50.0, bz1-45.0),
                             Vec3::new(bx0+75.0, 65.0, bz1-10.0), BED_PILLOW);
    box_xyz(m, "br1_pill_r", Vec3::new(bx0+85.0, 50.0, bz1-45.0),
                             Vec3::new(bx1-10.0, 65.0, bz1-10.0), BED_PILLOW);
    box_xyz(m, "br1_blanket", Vec3::new(bx0+5.0, 50.0, bz0+5.0),
                              Vec3::new(bx1-5.0, 55.0, bz0+70.0), BED_BLANKET_B);
    box_xyz(m, "br1_headbrd", Vec3::new(bx0, 25.0, bz1-4.0),
                              Vec3::new(bx1, 100.0, bz1), WOOD_DARK);
    // Nightstand left of bed
    box_xyz(m, "br1_ns_l", Vec3::new(bx0-55.0, 0.0, bz1-45.0),
                           Vec3::new(bx0-10.0, 55.0, bz1), WOOD_LIGHT);
    box_xyz(m, "br1_lamp_pole", Vec3::new(bx0-35.0, 55.0, bz1-25.0),
                                Vec3::new(bx0-31.0, 105.0, bz1-21.0), METAL);
    box_xyz(m, "br1_lamp_shade", Vec3::new(bx0-45.0, 105.0, bz1-35.0),
                                 Vec3::new(bx0-20.0, 125.0, bz1-10.0), LAMP_SHADE);
    // Nightstand right of bed
    box_xyz(m, "br1_ns_r", Vec3::new(bx1+10.0, 0.0, bz1-45.0),
                           Vec3::new(bx1+55.0, 55.0, bz1), WOOD_LIGHT);
    // Wardrobe along west wall
    box_xyz(m, "br1_wd", Vec3::new(X0+10.0, 0.0, Z0+30.0),
                         Vec3::new(X0+65.0, 220.0, Z0+180.0), WOOD_LIGHT);
    box_xyz(m, "br1_wd_split", Vec3::new(X0+10.0, 5.0, Z0+103.0),
                               Vec3::new(X0+65.0, 215.0, Z0+107.0), WOOD_DARK);
    box_xyz(m, "br1_wd_h1", Vec3::new(X0+65.0, 95.0, Z0+90.0),
                            Vec3::new(X0+67.0, 115.0, Z0+98.0), METAL);
    box_xyz(m, "br1_wd_h2", Vec3::new(X0+65.0, 95.0, Z0+115.0),
                            Vec3::new(X0+67.0, 115.0, Z0+123.0), METAL);
    // Rug in front of bed
    box_xyz(m, "br1_rug", Vec3::new(120.0, 0.0, 470.0),
                          Vec3::new(240.0, 2.0, 570.0), CHAIR_TEAL);
    // Plant
    box_xyz(m, "br1_pot", Vec3::new(330.0, 0.0, 770.0),
                          Vec3::new(360.0, 25.0, Z1-5.0), POT_TERRA);
    box_xyz(m, "br1_plant", Vec3::new(325.0, 25.0, 760.0),
                            Vec3::new(365.0, 80.0, Z1-2.0), PLANT_LEAF);
    let _ = (X1, Z0);
}

// ═══════════════════════════════════════════════════════════════
// BEDROOM 2 (small) — x [0, 380], z [200, 450] — 9.5 m²
// Kid / guest room: single bed + desk + chair.
// ═══════════════════════════════════════════════════════════════

fn add_bedroom_2(m: &mut BrickModel) {
    use palette::*;
    const X0: f32 = 0.0;
    const Z0: f32 = 200.0;
    const Z1: f32 = 450.0;

    // Single bed along west wall
    let bx0 = X0 + 15.0; let bx1 = bx0 + 90.0;   // 90 wide
    let bz0 = Z0 + 30.0; let bz1 = bz0 + 190.0;
    box_xyz(m, "br2_frame", Vec3::new(bx0, 0.0, bz0), Vec3::new(bx1, 22.0, bz1), WOOD_LIGHT);
    box_xyz(m, "br2_mat",   Vec3::new(bx0+3.0, 22.0, bz0+3.0),
                            Vec3::new(bx1-3.0, 45.0, bz1-3.0), BED_LINEN);
    box_xyz(m, "br2_pillow", Vec3::new(bx0+8.0, 45.0, bz1-35.0),
                             Vec3::new(bx1-8.0, 60.0, bz1-8.0), BED_PILLOW);
    box_xyz(m, "br2_blanket", Vec3::new(bx0+3.0, 45.0, bz0+3.0),
                              Vec3::new(bx1-3.0, 50.0, bz0+55.0), BED_BLANKET_A);

    // Desk along back wall (z=Z1 side)
    let dx0 = 180.0; let dx1 = 330.0;   // 150 wide
    let dz1 = Z1 - 10.0; let dz0 = dz1 - 55.0;
    box_xyz(m, "br2_desk_top", Vec3::new(dx0, 70.0, dz0),
                               Vec3::new(dx1, 75.0, dz1), WOOD_DARK);
    // Desk sides (panels)
    box_xyz(m, "br2_desk_l", Vec3::new(dx0, 0.0, dz0), Vec3::new(dx0+4.0, 70.0, dz1), WOOD_DARK);
    box_xyz(m, "br2_desk_r", Vec3::new(dx1-4.0, 0.0, dz0),
                             Vec3::new(dx1, 70.0, dz1), WOOD_DARK);
    // Book + lamp on desk
    box_xyz(m, "br2_book1", Vec3::new(dx0+15.0, 75.0, dz0+10.0),
                            Vec3::new(dx0+45.0, 77.0, dz0+35.0), BOOK_A);
    box_xyz(m, "br2_book2", Vec3::new(dx0+50.0, 75.0, dz0+10.0),
                            Vec3::new(dx0+80.0, 77.0, dz0+30.0), BOOK_C);
    box_xyz(m, "br2_desklamp_pole", Vec3::new(dx1-30.0, 75.0, dz0+20.0),
                                    Vec3::new(dx1-27.0, 120.0, dz0+23.0), METAL);
    box_xyz(m, "br2_desklamp_shade", Vec3::new(dx1-40.0, 120.0, dz0+10.0),
                                     Vec3::new(dx1-15.0, 140.0, dz0+35.0), LAMP_SHADE);
    // Desk chair
    let cx = 240.0; let cz = dz0 - 60.0;
    box_xyz(m, "br2_chair_seat", Vec3::new(cx, 42.0, cz),
                                 Vec3::new(cx+45.0, 47.0, cz+45.0), CHAIR_YEL);
    for (lx, lz) in [(cx+2.0, cz+2.0), (cx+38.0, cz+2.0),
                     (cx+2.0, cz+38.0), (cx+38.0, cz+38.0)] {
        box_xyz(m, "br2_chair_leg",
            Vec3::new(lx, 0.0, lz), Vec3::new(lx+5.0, 42.0, lz+5.0), WOOD_DARK);
    }
    box_xyz(m, "br2_chair_back",
        Vec3::new(cx, 47.0, cz+40.0), Vec3::new(cx+45.0, 85.0, cz+45.0), CHAIR_YEL);

    // Rug
    box_xyz(m, "br2_rug", Vec3::new(150.0, 0.0, 260.0),
                          Vec3::new(300.0, 2.0, 360.0), CHAIR_GREEN);
}

// ═══════════════════════════════════════════════════════════════
// BATHROOM — x [0, 380], z [0, 200] — 7.6 m²
// Toilet + bathtub + sink
// ═══════════════════════════════════════════════════════════════

fn add_bathroom(m: &mut BrickModel) {
    use palette::*;
    const X1: f32 = 380.0;

    // Bathtub along west wall
    let tx0 = 10.0; let tx1 = 80.0;    // 70 wide (shower tub oriented N-S)
    let tz0 = 20.0; let tz1 = 180.0;   // 160 long
    box_xyz(m, "bath_tub_outer", Vec3::new(tx0, 0.0, tz0), Vec3::new(tx1, 55.0, tz1), PORCELAIN);
    // Inner cavity
    box_xyz(m, "bath_tub_inner", Vec3::new(tx0+6.0, 10.0, tz0+6.0),
                                 Vec3::new(tx1-6.0, 55.0, tz1-6.0), TILE_LIGHT);
    // Faucet end
    box_xyz(m, "bath_faucet", Vec3::new(tx0+30.0, 55.0, tz1-10.0),
                              Vec3::new(tx0+45.0, 85.0, tz1-2.0), METAL);

    // Toilet (against east wall near the door)
    let tox = 280.0;
    box_xyz(m, "bath_toilet_base", Vec3::new(tox, 0.0, 50.0),
                                   Vec3::new(tox+40.0, 15.0, 90.0), PORCELAIN);
    box_xyz(m, "bath_toilet_bowl", Vec3::new(tox, 15.0, 50.0),
                                   Vec3::new(tox+40.0, 40.0, 90.0), PORCELAIN);
    box_xyz(m, "bath_toilet_seat", Vec3::new(tox-1.0, 40.0, 50.0),
                                   Vec3::new(tox+41.0, 42.0, 90.0), BLACK);
    box_xyz(m, "bath_toilet_tank", Vec3::new(tox, 40.0, 85.0),
                                   Vec3::new(tox+40.0, 80.0, 100.0), PORCELAIN);

    // Sink (between bathtub and toilet)
    let sx = 150.0;
    box_xyz(m, "bath_sink_ped", Vec3::new(sx, 0.0, 10.0),
                                Vec3::new(sx+25.0, 70.0, 35.0), PORCELAIN);
    box_xyz(m, "bath_sink_basin", Vec3::new(sx-15.0, 70.0, 0.0),
                                  Vec3::new(sx+40.0, 88.0, 45.0), PORCELAIN);
    box_xyz(m, "bath_sink_inner", Vec3::new(sx-10.0, 75.0, 5.0),
                                  Vec3::new(sx+35.0, 88.0, 40.0), TILE_LIGHT);
    box_xyz(m, "bath_sink_fau", Vec3::new(sx+10.0, 88.0, 2.0),
                                Vec3::new(sx+18.0, 95.0, 10.0), METAL);

    // Towel on a rack
    box_xyz(m, "bath_towel", Vec3::new(220.0, 100.0, 5.0),
                             Vec3::new(270.0, 135.0, 10.0), CHAIR_TEAL);

    // Ceiling lamp
    box_xyz(m, "bath_lamp", Vec3::new(170.0, 235.0, 90.0),
                            Vec3::new(200.0, CEILING_Y, 120.0), LAMP_SHADE);
    let _ = X1;
}

// ═══════════════════════════════════════════════════════════════
// LIVING ROOM — x [520, 900], z [450, 800] — 13.3 m²
// Sofa + coffee table + TV + lamp + bookshelf + plant
// ═══════════════════════════════════════════════════════════════

fn add_living_room(m: &mut BrickModel) {
    use palette::*;
    const X0: f32 = 520.0;
    const X1: f32 = 900.0;
    const Z0: f32 = 450.0;
    const Z1: f32 = 800.0;

    // Sofa (east wall, facing west into room)
    let sx1 = X1 - 15.0; let sx0 = sx1 - 90.0;
    let sz0 = Z0 + 80.0; let sz1 = sz0 + 200.0;
    box_xyz(m, "liv_sofa_base", Vec3::new(sx0, 0.0, sz0),
                                Vec3::new(sx1, 40.0, sz1), SOFA_BLUE);
    box_xyz(m, "liv_sofa_seat", Vec3::new(sx0+6.0, 40.0, sz0+6.0),
                                Vec3::new(sx1-6.0, 50.0, sz1-6.0), SOFA_CUSH);
    box_xyz(m, "liv_sofa_back", Vec3::new(sx1-20.0, 40.0, sz0),
                                Vec3::new(sx1, 75.0, sz1), SOFA_BLUE);
    box_xyz(m, "liv_sofa_arm_n", Vec3::new(sx0, 40.0, sz0),
                                 Vec3::new(sx1, 60.0, sz0+15.0), SOFA_BLUE);
    box_xyz(m, "liv_sofa_arm_s", Vec3::new(sx0, 40.0, sz1-15.0),
                                 Vec3::new(sx1, 60.0, sz1), SOFA_BLUE);
    box_xyz(m, "liv_cush_1", Vec3::new(sx0+20.0, 50.0, sz0+25.0),
                             Vec3::new(sx0+55.0, 72.0, sz0+55.0), SOFA_CUSH);
    box_xyz(m, "liv_cush_2", Vec3::new(sx0+20.0, 50.0, sz1-55.0),
                             Vec3::new(sx0+55.0, 72.0, sz1-25.0), SOFA_CUSH);

    // Coffee table
    let tx0 = sx0 - 120.0; let tx1 = tx0 + 100.0;
    let tz0 = sz0 + 60.0;  let tz1 = sz1 - 60.0;
    box_xyz(m, "liv_ct_top", Vec3::new(tx0, 40.0, tz0),
                             Vec3::new(tx1, 45.0, tz1), WOOD_DARK);
    for (lx, lz) in [(tx0+3.0, tz0+3.0), (tx1-8.0, tz0+3.0),
                     (tx0+3.0, tz1-8.0), (tx1-8.0, tz1-8.0)] {
        box_xyz(m, "liv_ct_leg",
            Vec3::new(lx, 0.0, lz), Vec3::new(lx+5.0, 40.0, lz+5.0), WOOD_DARK);
    }
    box_xyz(m, "liv_book", Vec3::new(tx0+15.0, 45.0, tz0+15.0),
                           Vec3::new(tx0+45.0, 47.0, tz0+35.0), BOOK_B);
    box_xyz(m, "liv_teapot", Vec3::new(tx1-30.0, 45.0, tz1-25.0),
                             Vec3::new(tx1-10.0, 60.0, tz1-10.0), CHAIR_TEAL);

    // Rug under coffee table
    box_xyz(m, "liv_rug", Vec3::new(tx0-20.0, 0.0, tz0-30.0),
                          Vec3::new(sx0-5.0, 2.0, tz1+30.0), RUG_RED);

    // TV stand against the back wall (z=Z1 side)
    let vsz1 = Z1 - 15.0; let vsz0 = vsz1 - 25.0;
    let vsx0 = X0 + 100.0; let vsx1 = vsx0 + 140.0;
    box_xyz(m, "liv_tv_stand", Vec3::new(vsx0, 0.0, vsz0),
                               Vec3::new(vsx1, 50.0, vsz1), WOOD_DARK);
    box_xyz(m, "liv_tv_bezel", Vec3::new(vsx0+20.0, 55.0, vsz1),
                               Vec3::new(vsx1-20.0, 120.0, vsz1+8.0), BLACK);
    box_xyz(m, "liv_tv_screen", Vec3::new(vsx0+25.0, 60.0, vsz1+8.0),
                                Vec3::new(vsx1-25.0, 115.0, vsz1+9.0), TV_SCREEN);

    // Bookshelf (west wall, near the door to corridor)
    let bx0 = X0 + 25.0; let bx1 = bx0 + 25.0;
    let bz0 = Z0 + 20.0; let bz1 = bz0 + 100.0;
    box_xyz(m, "liv_shelf", Vec3::new(bx0, 0.0, bz0),
                            Vec3::new(bx1, 220.0, bz1), WOOD_LIGHT);
    let colors = [BOOK_A, BOOK_B, BOOK_C, BOOK_D, BOOK_A, BOOK_C, BOOK_B, BOOK_D];
    for row_y in [40.0_f32, 100.0, 160.0] {
        let mut bz = bz0 + 4.0;
        for (i, c) in colors.iter().enumerate() {
            let w = 7.0 + (i as f32 * 2.0) % 6.0;
            if bz + w > bz1 - 4.0 { break; }
            box_xyz(m, "liv_book_shelf",
                Vec3::new(bx0+3.0, row_y, bz),
                Vec3::new(bx1-3.0, row_y+43.0, bz+w), *c);
            bz += w + 1.0;
        }
    }

    // Floor lamp corner
    box_xyz(m, "liv_lamp_base", Vec3::new(X1-40.0, 0.0, Z1-40.0),
                                Vec3::new(X1-20.0, 5.0, Z1-20.0), METAL);
    box_xyz(m, "liv_lamp_pole", Vec3::new(X1-32.0, 5.0, Z1-32.0),
                                Vec3::new(X1-28.0, 150.0, Z1-28.0), METAL);
    box_xyz(m, "liv_lamp_shade", Vec3::new(X1-45.0, 150.0, Z1-45.0),
                                 Vec3::new(X1-15.0, 180.0, Z1-15.0), LAMP_SHADE);

    // Plant
    box_xyz(m, "liv_pot", Vec3::new(X0+80.0, 0.0, Z0+30.0),
                          Vec3::new(X0+110.0, 25.0, Z0+60.0), POT_TERRA);
    box_xyz(m, "liv_plant", Vec3::new(X0+75.0, 25.0, Z0+25.0),
                            Vec3::new(X0+115.0, 90.0, Z0+65.0), PLANT_LEAF);
}

// ═══════════════════════════════════════════════════════════════
// KITCHEN — x [520, 900], z [200, 450] — 9.5 m²
// Fridge + counter with stove & sink + small table with chairs
// ═══════════════════════════════════════════════════════════════

fn add_kitchen(m: &mut BrickModel) {
    use palette::*;
    const X0: f32 = 520.0;
    const X1: f32 = 900.0;
    const Z0: f32 = 200.0;
    const Z1: f32 = 450.0;

    // Fridge near corridor door (west side, back corner)
    box_xyz(m, "kit_fridge", Vec3::new(X0+15.0, 0.0, Z1-70.0),
                             Vec3::new(X0+75.0, 180.0, Z1-10.0), FRIDGE);
    box_xyz(m, "kit_fridge_h", Vec3::new(X0+75.0, 60.0, Z1-45.0),
                               Vec3::new(X0+77.0, 130.0, Z1-40.0), METAL);

    // Counter along back wall from fridge to east wall
    box_xyz(m, "kit_counter", Vec3::new(X0+75.0, 0.0, Z1-70.0),
                              Vec3::new(X1-15.0, 85.0, Z1-10.0), WALL);
    box_xyz(m, "kit_counter_top", Vec3::new(X0+70.0, 85.0, Z1-75.0),
                                  Vec3::new(X1-10.0, 92.0, Z1-5.0), WOOD_DARK);

    // Stove
    box_xyz(m, "kit_stove", Vec3::new(X0+150.0, 85.0, Z1-65.0),
                            Vec3::new(X0+210.0, 95.0, Z1-15.0), STOVE);
    for (rx, rz_off) in [(165.0_f32, -50.0), (195.0, -50.0),
                         (165.0, -25.0), (195.0, -25.0)] {
        box_xyz(m, "kit_ring",
            Vec3::new(X0+rx, 95.0, Z1+rz_off),
            Vec3::new(X0+rx+8.0, 97.0, Z1+rz_off+8.0), METAL);
    }

    // Sink
    box_xyz(m, "kit_sink", Vec3::new(X0+270.0, 85.0, Z1-65.0),
                           Vec3::new(X0+330.0, 90.0, Z1-15.0), METAL);
    box_xyz(m, "kit_faucet", Vec3::new(X0+295.0, 90.0, Z1-25.0),
                             Vec3::new(X0+305.0, 115.0, Z1-20.0), METAL);

    // Kitchen table (centre of room, compact P-44 kitchen)
    let ttx0 = X0+140.0; let ttx1 = ttx0+130.0;
    let ttz0 = Z0+40.0;  let ttz1 = ttz0+85.0;
    box_xyz(m, "kit_tbl_top", Vec3::new(ttx0, 70.0, ttz0),
                              Vec3::new(ttx1, 75.0, ttz1), WOOD_LIGHT);
    for (lx, lz) in [(ttx0+4.0, ttz0+4.0), (ttx1-10.0, ttz0+4.0),
                     (ttx0+4.0, ttz1-10.0), (ttx1-10.0, ttz1-10.0)] {
        box_xyz(m, "kit_tbl_leg",
            Vec3::new(lx, 0.0, lz), Vec3::new(lx+6.0, 70.0, lz+6.0), WOOD_LIGHT);
    }
    // 3 chairs (small kitchen)
    let chairs = [
        (ttx0-55.0, ttz0+20.0, CHAIR_RED),
        (ttx1+10.0, ttz0+20.0, CHAIR_GREEN),
        (ttx0+45.0, ttz1+5.0,  CHAIR_YEL),
    ];
    for (cx, cz, col) in chairs {
        box_xyz(m, "kit_ch_seat", Vec3::new(cx, 45.0, cz),
                                  Vec3::new(cx+45.0, 50.0, cz+45.0), col);
        for (lx, lz) in [(cx+2.0, cz+2.0), (cx+38.0, cz+2.0),
                         (cx+2.0, cz+38.0), (cx+38.0, cz+38.0)] {
            box_xyz(m, "kit_ch_leg",
                Vec3::new(lx, 0.0, lz), Vec3::new(lx+5.0, 45.0, lz+5.0), WOOD_DARK);
        }
        // Backrest oriented toward closest table edge
        if cx < ttx0 {
            box_xyz(m, "kit_ch_back", Vec3::new(cx, 50.0, cz),
                                      Vec3::new(cx+5.0, 85.0, cz+45.0), col);
        } else if cx > ttx1 {
            box_xyz(m, "kit_ch_back", Vec3::new(cx+40.0, 50.0, cz),
                                      Vec3::new(cx+45.0, 85.0, cz+45.0), col);
        } else {
            box_xyz(m, "kit_ch_back", Vec3::new(cx, 50.0, cz+40.0),
                                      Vec3::new(cx+45.0, 85.0, cz+45.0), col);
        }
    }
    // Fruit bowl
    box_xyz(m, "kit_bowl", Vec3::new(ttx0+40.0, 75.0, ttz0+25.0),
                           Vec3::new(ttx0+80.0, 88.0, ttz0+65.0), CHAIR_RED);
    box_xyz(m, "kit_fruit1", Vec3::new(ttx0+48.0, 85.0, ttz0+35.0),
                             Vec3::new(ttx0+62.0, 98.0, ttz0+49.0), CHAIR_RED);
    box_xyz(m, "kit_fruit2", Vec3::new(ttx0+60.0, 85.0, ttz0+45.0),
                             Vec3::new(ttx0+74.0, 98.0, ttz0+59.0), CHAIR_YEL);
    // Ceiling lamp above table
    let lx = (ttx0 + ttx1) * 0.5 - 20.0;
    let lz = (ttz0 + ttz1) * 0.5 - 20.0;
    box_xyz(m, "kit_lamp_shade", Vec3::new(lx, 200.0, lz),
                                 Vec3::new(lx+40.0, 220.0, lz+40.0), LAMP_SHADE);
    box_xyz(m, "kit_lamp_cord", Vec3::new(lx+18.0, 220.0, lz+18.0),
                                Vec3::new(lx+22.0, CEILING_Y, lz+22.0), METAL);
}

// ═══════════════════════════════════════════════════════════════
// ENTRY HALL — x [520, 900], z [0, 200] — 7.6 m²
// Coat rack, shoe mat, mirror, bench
// ═══════════════════════════════════════════════════════════════

fn add_entry_hall(m: &mut BrickModel) {
    use palette::*;
    const X0: f32 = 520.0;
    const X1: f32 = 900.0;

    // Coat rack against east wall
    box_xyz(m, "eh_rack_base", Vec3::new(X1-40.0, 0.0, 40.0),
                               Vec3::new(X1-15.0, 5.0, 70.0), METAL);
    box_xyz(m, "eh_rack_pole", Vec3::new(X1-29.0, 5.0, 52.0),
                               Vec3::new(X1-25.0, 180.0, 56.0), METAL);
    for (hx, hy) in [(X1-20.0, 165.0_f32), (X1-35.0, 150.0), (X1-20.0, 135.0)] {
        box_xyz(m, "eh_hook", Vec3::new(hx-1.0, hy, 50.0),
                              Vec3::new(hx+3.0, hy+8.0, 58.0), METAL);
    }
    // Jacket on a hook
    box_xyz(m, "eh_jacket", Vec3::new(X1-55.0, 90.0, 38.0),
                            Vec3::new(X1-15.0, 160.0, 68.0), CHAIR_GREEN);

    // Shoe mat by entry
    box_xyz(m, "eh_mat", Vec3::new(X0+30.0, 0.0, 5.0),
                         Vec3::new(X0+130.0, 2.0, 60.0), WOOD_DARK);
    box_xyz(m, "eh_shoe_l", Vec3::new(X0+50.0, 2.0, 20.0),
                            Vec3::new(X0+70.0, 10.0, 50.0), CHAIR_RED);
    box_xyz(m, "eh_shoe_r", Vec3::new(X0+75.0, 2.0, 20.0),
                            Vec3::new(X0+95.0, 10.0, 50.0), CHAIR_RED);

    // Wall mirror (tall narrow)
    box_xyz(m, "eh_mirror", Vec3::new(X1-2.0, 100.0, 100.0),
                            Vec3::new(X1, 180.0, 160.0), TILE_LIGHT);
    box_xyz(m, "eh_mirror_frame", Vec3::new(X1-4.0, 98.0, 98.0),
                                  Vec3::new(X1, 182.0, 162.0), WOOD_DARK);

    // Small bench for putting on shoes
    box_xyz(m, "eh_bench_seat", Vec3::new(X0+30.0, 40.0, 100.0),
                                Vec3::new(X0+130.0, 45.0, 140.0), WOOD_LIGHT);
    box_xyz(m, "eh_bench_l", Vec3::new(X0+32.0, 0.0, 102.0),
                             Vec3::new(X0+42.0, 40.0, 138.0), WOOD_DARK);
    box_xyz(m, "eh_bench_r", Vec3::new(X0+118.0, 0.0, 102.0),
                             Vec3::new(X0+128.0, 40.0, 138.0), WOOD_DARK);

    // Ceiling lamp
    box_xyz(m, "eh_lamp", Vec3::new(X0+200.0, 235.0, 80.0),
                          Vec3::new(X0+220.0, CEILING_Y, 100.0), LAMP_SHADE);
}
