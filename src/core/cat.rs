// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Chibi Cat Mascot (QUADRUPED)
//
// Voxel-art chibi cat, four-legged, tuned to the Seedream 4.5
// reference sheet: oversized stepped-cube head, compact body,
// short stubby legs, curled tail. Pure Brick composition —
// every part is an oriented box that follows its skeleton bone.
// ═══════════════════════════════════════════════════════════════

use glam::{Quat, Vec3};
use super::skeleton::{Skeleton, BoneId};
use super::brick::{Brick, BrickModel};

mod palette {
    pub const ORANGE: [u8; 3] = [235, 130, 50];
    pub const CREAM:  [u8; 3] = [250, 230, 195];
    pub const STRIPE: [u8; 3] = [165, 80, 25];
    pub const PINK:   [u8; 3] = [245, 165, 175];
    pub const GREEN:  [u8; 3] = [70, 215, 95];
    pub const WHITE:  [u8; 3] = [252, 252, 252];
    pub const DARK:   [u8; 3] = [22, 18, 22];
    pub const OUTLINE:[u8; 3] = [60, 35, 15];
}

pub fn build_chibi_cat(sk: &Skeleton, scale: f32) -> BrickModel {
    let mut m = BrickModel::new("ChibiCat");
    let s = scale;

    let pelvis  = sk.bone("pelvis").id;
    let head    = sk.bone("head").id;

    // Local offset (in head-bone space) that positions the centre of the
    // head forward and slightly up of the head joint — same axis remap
    // the head bone direction uses in the skeleton definition.
    let head_y = 1.5 * s;
    let head_z = 1.0 * s;

    // ─── BODY: horizontal trunk from pelvis forward toward chest
    add_body(&mut m, pelvis, sk, s);

    // ─── HEAD: stepped rounded cube
    add_head(&mut m, head, head_y, head_z, s);

    // ─── EARS: flat stepped triangles (1-voxel deep)
    add_ears(&mut m, head, head_y, head_z, s);

    // ─── FACE: eyes, nose, mouth, whiskers
    add_face(&mut m, head, head_y, head_z, s);

    // ─── MARKINGS: M-mark on forehead, cheek stripes, body stripes
    add_markings(&mut m, head, head_y, head_z, s, pelvis);

    // ─── LEGS: 4 short stubby leg segments
    add_legs(&mut m, sk, s);

    // ─── TAIL: 4 curled segments
    add_tail(&mut m, sk, s);

    m
}

// ─── BODY ────────────────────────────────────────────────────

fn add_body(m: &mut BrickModel, pelvis: BoneId, sk: &Skeleton, s: f32) {
    use palette::*;
    // Torso sits along the spine (pelvis → spine2_end is +Z).
    // Length ≈ 3 units of skeleton = 3*s voxels.
    let trunk_len_half = 1.5 * s;      // half-length along Z (total 3*s)
    let trunk_w_half   = 1.3 * s;      // half-width along X
    let trunk_h_half   = 1.1 * s;      // half-height along Y

    // Placed so the back of the trunk is at the pelvis joint, front at spine2_e
    let trunk = Brick::new("trunk", Vec3::new(trunk_w_half, trunk_h_half, trunk_len_half), ORANGE)
        .attached_to(pelvis)
        .with_position(Vec3::new(0.0, 0.0, trunk_len_half));
    m.add(trunk);

    // Cream belly — flat slab on the underside
    let belly = Brick::new("belly", Vec3::new(trunk_w_half - 0.2*s, 0.25*s, trunk_len_half - 0.2*s), CREAM)
        .attached_to(pelvis)
        .with_position(Vec3::new(0.0, -trunk_h_half + 0.2*s, trunk_len_half));
    m.add(belly);

    // Chest cream — forward-facing slab on front of trunk
    let spine2_e = sk.bone("spine2").world_end_position - sk.bone("pelvis").world_position;
    let chest_z = spine2_e.z; // forward from pelvis
    let chest = Brick::new("chest", Vec3::new(trunk_w_half - 0.1*s, trunk_h_half - 0.1*s, 0.25*s), CREAM)
        .attached_to(pelvis)
        .with_position(Vec3::new(0.0, -0.3*s, chest_z));
    m.add(chest);
}

// ─── HEAD: stepped rounded cube ──────────────────────────────

fn add_head(m: &mut BrickModel, head: BoneId, head_y: f32, head_z: f32, s: f32) {
    use palette::*;
    // Stack of boxes, each upper layer narrower than the one below.
    // y_base = head center; head_z pushes the block forward of the neck joint.

    // Chin layer — narrower bottom
    let chin = Brick::new("head_chin", Vec3::new(1.2*s, 0.5*s, 1.0*s), ORANGE)
        .attached_to(head)
        .with_position(Vec3::new(0.0, head_y - 1.0*s, head_z + 0.2*s));
    m.add(chin);

    // Core — widest layer, eye level
    let core = Brick::new("head_core", Vec3::new(1.7*s, 1.0*s, 1.5*s), ORANGE)
        .attached_to(head)
        .with_position(Vec3::new(0.0, head_y, head_z + 0.3*s));
    m.add(core);

    // Step 1 — slightly narrower
    let step1 = Brick::new("head_step1", Vec3::new(1.4*s, 0.3*s, 1.2*s), ORANGE)
        .attached_to(head)
        .with_position(Vec3::new(0.0, head_y + 1.2*s, head_z + 0.2*s));
    m.add(step1);

    // Step 2 — narrowest, forehead plateau
    let step2 = Brick::new("head_step2", Vec3::new(1.1*s, 0.3*s, 1.0*s), ORANGE)
        .attached_to(head)
        .with_position(Vec3::new(0.0, head_y + 1.7*s, head_z + 0.1*s));
    m.add(step2);

    // Cream muzzle plate — face around nose
    let muzzle = Brick::new("muzzle_cream", Vec3::new(0.9*s, 0.55*s, 0.2*s), CREAM)
        .attached_to(head)
        .with_position(Vec3::new(0.0, head_y - 0.35*s, head_z + 1.7*s));
    m.add(muzzle);

    // Chin cream plate
    let chin_cream = Brick::new("chin_cream", Vec3::new(0.9*s, 0.2*s, 0.7*s), CREAM)
        .attached_to(head)
        .with_position(Vec3::new(0.0, head_y - 1.1*s, head_z + 1.0*s));
    m.add(chin_cream);
}

// ─── EARS: flat stepped triangles ────────────────────────────

fn add_ears(m: &mut BrickModel, head: BoneId, head_y: f32, head_z: f32, s: f32) {
    use palette::*;
    for &side in &[-1.0_f32, 1.0] {
        let x = side * 1.15*s;
        let base = Brick::new("ear_base", Vec3::new(0.4*s, 0.25*s, 0.15*s), ORANGE)
            .attached_to(head).with_position(Vec3::new(x, head_y + 2.2*s, head_z));
        m.add(base);
        let mid = Brick::new("ear_mid", Vec3::new(0.3*s, 0.25*s, 0.15*s), ORANGE)
            .attached_to(head).with_position(Vec3::new(x, head_y + 2.7*s, head_z));
        m.add(mid);
        let tip = Brick::new("ear_tip", Vec3::new(0.2*s, 0.25*s, 0.15*s), ORANGE)
            .attached_to(head).with_position(Vec3::new(x, head_y + 3.2*s, head_z));
        m.add(tip);
        // Pink interior — slightly forward
        let inner = Brick::new("ear_inner", Vec3::new(0.2*s, 0.4*s, 0.08*s), PINK)
            .attached_to(head).with_position(Vec3::new(x, head_y + 2.5*s, head_z + 0.1*s));
        m.add(inner);
    }
}

// ─── FACE: eyes + nose + mouth ───────────────────────────────

fn add_face(m: &mut BrickModel, head: BoneId, head_y: f32, head_z: f32, s: f32) {
    use palette::*;
    let face_z = head_z + 1.8*s; // front face of core block
    let eye_y = head_y + 0.3*s;
    let eye_dx = 0.85*s;

    for (name, sx) in [("eye_l", -1.0_f32), ("eye_r", 1.0)] {
        // Dark outline
        let outline = Brick::new(&format!("{}_outline", name), Vec3::new(0.65*s, 0.75*s, 0.1*s), OUTLINE)
            .attached_to(head).with_position(Vec3::new(sx * eye_dx, eye_y, face_z + 0.05*s));
        m.add(outline);
        // Green iris
        let iris = Brick::new(&format!("{}_iris", name), Vec3::new(0.55*s, 0.65*s, 0.1*s), GREEN)
            .attached_to(head).with_position(Vec3::new(sx * eye_dx, eye_y, face_z + 0.12*s));
        m.add(iris);
        // Dark pupil
        let pupil = Brick::new(&format!("{}_pupil", name), Vec3::new(0.25*s, 0.35*s, 0.1*s), DARK)
            .attached_to(head).with_position(Vec3::new(sx * eye_dx, eye_y, face_z + 0.2*s));
        m.add(pupil);
        // White highlight (upper outer)
        let hl_dx = sx * (eye_dx - 0.2*s);
        let hl = Brick::new(&format!("{}_hl", name), Vec3::new(0.15*s, 0.15*s, 0.08*s), WHITE)
            .attached_to(head).with_position(Vec3::new(hl_dx, eye_y + 0.35*s, face_z + 0.25*s));
        m.add(hl);
    }

    // Pink nose — a small block sticking forward
    let nose = Brick::new("nose", Vec3::new(0.22*s, 0.2*s, 0.3*s), PINK)
        .attached_to(head).with_position(Vec3::new(0.0, head_y - 0.2*s, face_z + 0.25*s));
    m.add(nose);

    // Dark mouth line
    let mouth = Brick::new("mouth", Vec3::new(0.3*s, 0.06*s, 0.1*s), DARK)
        .attached_to(head).with_position(Vec3::new(0.0, head_y - 0.55*s, face_z + 0.1*s));
    m.add(mouth);

    // Whiskers — 3 thin horizontal rods sticking out each side of the muzzle.
    // Top whisker tilts up, middle straight, bottom tilts down (±Z via offset).
    let whisker_half = Vec3::new(0.55*s, 0.04*s, 0.04*s);
    for &side in &[-1.0_f32, 1.0] {
        // Anchor x so the inner end of the whisker starts at the muzzle edge
        let x_center = side * 1.55*s;
        let y_base = head_y - 0.35*s;
        let z_base = face_z - 0.05*s;
        // Row dy values (top → middle → bottom)
        for &dy in &[0.2_f32, 0.0, -0.2] {
            let w = Brick::new("whisker", whisker_half, DARK)
                .attached_to(head)
                .with_position(Vec3::new(x_center, y_base + dy*s, z_base));
            m.add(w);
        }
    }
}

// ─── MARKINGS: M-mark + cheek stripes + body stripes ─────────

fn add_markings(m: &mut BrickModel, head: BoneId, head_y: f32, head_z: f32, s: f32, pelvis: BoneId) {
    use palette::*;
    let face_z = head_z + 1.8*s;

    // M-mark on forehead — 3 short vertical stripes
    for &dx in &[-0.7_f32, 0.0, 0.7] {
        let stripe = Brick::new("m_mark", Vec3::new(0.1*s, 0.4*s, 0.08*s), STRIPE)
            .attached_to(head).with_position(Vec3::new(dx*s, head_y + 1.6*s, face_z + 0.02*s));
        m.add(stripe);
    }
    // Cheek stripes — small horizontal dashes from nose outward
    for &side in &[-1.0_f32, 1.0] {
        for i in 0..2 {
            let dy = -0.1 - i as f32 * 0.35;
            let stripe = Brick::new("cheek_stripe", Vec3::new(0.1*s, 0.08*s, 0.3*s), STRIPE)
                .attached_to(head)
                .with_position(Vec3::new(side * 1.55*s, head_y + dy*s, head_z + 1.0*s));
            m.add(stripe);
        }
    }
    // Side head stripes — 1-2 vertical stripes on the head sides (tabby)
    for &side in &[-1.0_f32, 1.0] {
        for i in 0..2 {
            let dz = 0.3 + i as f32 * 0.6;
            let stripe = Brick::new("head_side_stripe", Vec3::new(0.08*s, 0.4*s, 0.1*s), STRIPE)
                .attached_to(head)
                .with_position(Vec3::new(side * 1.72*s, head_y + 0.5*s, head_z + dz*s));
            m.add(stripe);
        }
    }

    // Body stripes — horizontal dashes along the back (cat is on all 4s,
    // back is +Y of the trunk).  Using pelvis as anchor since trunk is too.
    for i in 0..3 {
        let dz = 0.6 + i as f32 * 0.8;
        let stripe = Brick::new("back_stripe", Vec3::new(1.2*s, 0.1*s, 0.15*s), STRIPE)
            .attached_to(pelvis)
            .with_position(Vec3::new(0.0, 1.15*s, dz*s));
        m.add(stripe);
    }
    // Side body stripes
    for &side in &[-1.0_f32, 1.0] {
        for i in 0..3 {
            let dz = 0.5 + i as f32 * 0.7;
            let stripe = Brick::new("side_stripe", Vec3::new(0.1*s, 0.5*s, 0.2*s), STRIPE)
                .attached_to(pelvis)
                .with_position(Vec3::new(side * 1.32*s, 0.2*s, dz*s));
            m.add(stripe);
        }
    }
}

// ─── LEGS: four short pillars ────────────────────────────────

fn add_legs(m: &mut BrickModel, sk: &Skeleton, s: f32) {
    use palette::*;
    let leg_half = 0.35 * s;

    // Front legs (upper_arm + forearm + paw)
    for (upper_name, fore_name, paw_name) in [
        ("upper_arm_l", "forearm_l", "paw_fl"),
        ("upper_arm_r", "forearm_r", "paw_fr"),
    ] {
        let upper_id = sk.bone(upper_name).id;
        let fore_id  = sk.bone(fore_name).id;
        let paw_id   = sk.bone(paw_name).id;
        let upper_len = sk.bone(upper_name).rest_length;
        let fore_len  = sk.bone(fore_name).rest_length;

        let upper = Brick::new("arm_upper", Vec3::new(leg_half, upper_len * 0.5, leg_half), ORANGE)
            .attached_to(upper_id).with_position(Vec3::new(0.0, -upper_len * 0.5, 0.0));
        m.add(upper);
        let fore = Brick::new("arm_fore", Vec3::new(leg_half, fore_len * 0.5, leg_half), ORANGE)
            .attached_to(fore_id).with_position(Vec3::new(0.0, -fore_len * 0.5, 0.0));
        m.add(fore);
        // Cream paw
        let paw = Brick::new("paw_front", Vec3::new(leg_half + 0.1*s, 0.3*s, leg_half + 0.15*s), CREAM)
            .attached_to(paw_id).with_position(Vec3::new(0.0, -0.15*s, 0.1*s));
        m.add(paw);
    }

    // Hind legs
    for (thigh_name, shin_name, paw_name) in [
        ("thigh_l", "shin_l", "paw_bl"),
        ("thigh_r", "shin_r", "paw_br"),
    ] {
        let thigh_id = sk.bone(thigh_name).id;
        let shin_id  = sk.bone(shin_name).id;
        let paw_id   = sk.bone(paw_name).id;
        let thigh_len = sk.bone(thigh_name).rest_length;
        let shin_len  = sk.bone(shin_name).rest_length;

        let thigh = Brick::new("leg_thigh", Vec3::new(leg_half, thigh_len * 0.5, leg_half), ORANGE)
            .attached_to(thigh_id).with_position(Vec3::new(0.0, -thigh_len * 0.5, 0.0));
        m.add(thigh);
        // Stripe ring
        let stripe = Brick::new("leg_stripe", Vec3::new(leg_half + 0.02*s, 0.15*s, leg_half + 0.02*s), STRIPE)
            .attached_to(thigh_id).with_position(Vec3::new(0.0, -thigh_len * 0.65, 0.0));
        m.add(stripe);
        let shin = Brick::new("leg_shin", Vec3::new(leg_half, shin_len * 0.5, leg_half), ORANGE)
            .attached_to(shin_id).with_position(Vec3::new(0.0, -shin_len * 0.5, 0.0));
        m.add(shin);
        // Cream paw
        let paw = Brick::new("paw_back", Vec3::new(leg_half + 0.1*s, 0.3*s, leg_half + 0.15*s), CREAM)
            .attached_to(paw_id).with_position(Vec3::new(0.0, -0.15*s, 0.1*s));
        m.add(paw);
    }
}

// ─── TAIL: 4 segments following bones ────────────────────────

fn add_tail(m: &mut BrickModel, sk: &Skeleton, s: f32) {
    use palette::*;
    let segs = [("tail1", 0.32_f32), ("tail2", 0.28), ("tail3", 0.25), ("tail4", 0.22)];
    for (i, (name, thick)) in segs.iter().enumerate() {
        let bone = sk.bone(name);
        let len = bone.rest_length;
        let body = Brick::new("tail_seg", Vec3::new(*thick * s, len * 0.5, *thick * s), ORANGE)
            .attached_to(bone.id)
            .with_position(Vec3::new(0.0, len * 0.5, 0.0));
        m.add(body);
        if i < 3 {
            let stripe = Brick::new("tail_stripe", Vec3::new(*thick * s + 0.02*s, 0.12*s, *thick * s + 0.02*s), STRIPE)
                .attached_to(bone.id)
                .with_position(Vec3::new(0.0, len * 0.6, 0.0));
            m.add(stripe);
        }
    }
    // Cream tail tip
    let tip_bone = sk.bone("tail4");
    let tip = Brick::new("tail_tip", Vec3::new(0.3*s, 0.3*s, 0.3*s), CREAM)
        .attached_to(tip_bone.id)
        .with_position(Vec3::new(0.0, tip_bone.rest_length, 0.0));
    m.add(tip);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_produces_expected_bricks() {
        let mut sk = Skeleton::chibi_cat(1.0);
        sk.solve_forward();
        let m = build_chibi_cat(&sk, 1.0);
        assert!(m.brick_count() >= 40, "Expected 40+ bricks, got {}", m.brick_count());
    }
}
