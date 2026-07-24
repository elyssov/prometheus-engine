// PROMETHEUS ENGINE - canonical PURRGE cat.
//
// This is an authored block sculpture. Broad beveled masses carry the
// silhouette; small layered plates are reserved for expression and markings.

use glam::Vec3;

use super::brick::{Brick, BrickModel};
use super::skeleton::{BoneId, Skeleton};

mod palette {
    pub const ORANGE: [u8; 3] = [244, 139, 32];
    pub const ORANGE_LIGHT: [u8; 3] = [255, 169, 52];
    pub const CREAM: [u8; 3] = [255, 237, 204];
    pub const STRIPE: [u8; 3] = [180, 68, 20];
    pub const PINK: [u8; 3] = [247, 145, 159];
    pub const GREEN: [u8; 3] = [58, 210, 92];
    pub const GREEN_LIGHT: [u8; 3] = [124, 244, 119];
    pub const WHITE: [u8; 3] = [255, 255, 250];
    pub const DARK: [u8; 3] = [24, 24, 30];
    pub const OUTLINE: [u8; 3] = [64, 37, 20];
}

pub fn build_chibi_cat(sk: &Skeleton, scale: f32) -> BrickModel {
    let mut model = BrickModel::new("PurrgeCat");
    let pelvis = sk.bone("pelvis").id;
    let head = sk.bone("head").id;
    let head_center = Vec3::new(0.0, 1.30 * scale, 0.95 * scale);

    add_body(&mut model, pelvis, scale);
    add_head(&mut model, head, head_center, scale);
    add_ears(&mut model, head, head_center, scale);
    add_face(&mut model, head, head_center, scale);
    add_markings(&mut model, head, pelvis, head_center, scale);
    add_legs(&mut model, sk, scale);
    add_tail(&mut model, sk, scale);

    model
}

fn hero_brick(name: &str, half: Vec3, color: [u8; 3]) -> Brick {
    Brick::new(name, half, color).with_bevel(half.min_element() * 0.30)
}

fn add_part(
    model: &mut BrickModel,
    name: &str,
    bone: BoneId,
    position: Vec3,
    half: Vec3,
    color: [u8; 3],
) {
    model.add(
        hero_brick(name, half, color)
            .attached_to(bone)
            .with_position(position),
    );
}

fn add_body(model: &mut BrickModel, pelvis: BoneId, s: f32) {
    use palette::*;
    let center = Vec3::new(0.0, 0.30 * s, 1.55 * s);
    add_part(
        model,
        "torso",
        pelvis,
        center,
        Vec3::new(1.05 * s, 0.78 * s, 1.50 * s),
        ORANGE,
    );
    add_part(
        model,
        "shoulders",
        pelvis,
        center + Vec3::new(0.0, 0.08 * s, 1.20 * s),
        Vec3::new(1.18 * s, 0.70 * s, 0.62 * s),
        ORANGE_LIGHT,
    );
    add_part(
        model,
        "belly",
        pelvis,
        center + Vec3::new(0.0, -0.74 * s, 0.25 * s),
        Vec3::new(0.78 * s, 0.16 * s, 1.05 * s),
        CREAM,
    );
    add_part(
        model,
        "chest",
        pelvis,
        center + Vec3::new(0.0, -0.20 * s, 1.55 * s),
        Vec3::new(0.72 * s, 0.58 * s, 0.14 * s),
        CREAM,
    );
}

fn add_head(model: &mut BrickModel, head: BoneId, c: Vec3, s: f32) {
    use palette::*;
    add_part(
        model,
        "head_core",
        head,
        c,
        Vec3::new(1.62 * s, 1.28 * s, 1.22 * s),
        ORANGE,
    );
    add_part(
        model,
        "head_crown",
        head,
        c + Vec3::new(0.0, 1.16 * s, -0.05 * s),
        Vec3::new(1.32 * s, 0.48 * s, 1.06 * s),
        ORANGE_LIGHT,
    );
    add_part(
        model,
        "head_cheeks",
        head,
        c + Vec3::new(0.0, -0.82 * s, 0.18 * s),
        Vec3::new(1.48 * s, 0.55 * s, 1.12 * s),
        ORANGE,
    );
    add_part(
        model,
        "head_chin",
        head,
        c + Vec3::new(0.0, -1.28 * s, 0.30 * s),
        Vec3::new(1.08 * s, 0.28 * s, 0.88 * s),
        CREAM,
    );
}

fn add_ears(model: &mut BrickModel, head: BoneId, c: Vec3, s: f32) {
    use palette::*;
    for side in [-1.0_f32, 1.0] {
        add_part(
            model,
            "ear_base",
            head,
            c + Vec3::new(side * 1.03 * s, 1.70 * s, -0.10 * s),
            Vec3::new(0.62 * s, 0.48 * s, 0.52 * s),
            ORANGE,
        );
        add_part(
            model,
            "ear_mid",
            head,
            c + Vec3::new(side * 1.24 * s, 2.35 * s, -0.12 * s),
            Vec3::new(0.43 * s, 0.42 * s, 0.42 * s),
            ORANGE,
        );
        add_part(
            model,
            "ear_tip",
            head,
            c + Vec3::new(side * 1.43 * s, 2.92 * s, -0.14 * s),
            Vec3::new(0.24 * s, 0.30 * s, 0.30 * s),
            ORANGE_LIGHT,
        );
        add_part(
            model,
            "ear_inner",
            head,
            c + Vec3::new(side * 1.12 * s, 2.03 * s, 0.43 * s),
            Vec3::new(0.28 * s, 0.43 * s, 0.08 * s),
            PINK,
        );
    }
}

fn add_face(model: &mut BrickModel, head: BoneId, c: Vec3, s: f32) {
    use palette::*;
    let z = c.z + 1.28 * s;
    let y = c.y + 0.08 * s;
    for (name, side) in [("eye_l", -1.0_f32), ("eye_r", 1.0)] {
        let x = c.x + side * 0.78 * s;
        add_part(
            model,
            &format!("{name}_cream"),
            head,
            Vec3::new(x, y, z + 0.04 * s),
            Vec3::new(0.66 * s, 0.80 * s, 0.08 * s),
            CREAM,
        );
        add_part(
            model,
            &format!("{name}_outline"),
            head,
            Vec3::new(x, y, z + 0.13 * s),
            Vec3::new(0.56 * s, 0.70 * s, 0.07 * s),
            OUTLINE,
        );
        add_part(
            model,
            &format!("{name}_iris"),
            head,
            Vec3::new(x, y, z + 0.22 * s),
            Vec3::new(0.47 * s, 0.60 * s, 0.06 * s),
            GREEN,
        );
        add_part(
            model,
            &format!("{name}_iris_light"),
            head,
            Vec3::new(x + side * 0.18 * s, y, z + 0.29 * s),
            Vec3::new(0.17 * s, 0.46 * s, 0.05 * s),
            GREEN_LIGHT,
        );
        add_part(
            model,
            &format!("{name}_pupil"),
            head,
            Vec3::new(x - side * 0.06 * s, y - 0.02 * s, z + 0.36 * s),
            Vec3::new(0.22 * s, 0.42 * s, 0.05 * s),
            DARK,
        );
        add_part(
            model,
            &format!("{name}_highlight"),
            head,
            Vec3::new(x - side * 0.22 * s, y + 0.34 * s, z + 0.43 * s),
            Vec3::new(0.13 * s, 0.15 * s, 0.04 * s),
            WHITE,
        );
    }

    add_part(
        model,
        "muzzle",
        head,
        c + Vec3::new(0.0, -0.70 * s, 1.36 * s),
        Vec3::new(0.82 * s, 0.42 * s, 0.18 * s),
        CREAM,
    );
    add_part(
        model,
        "nose",
        head,
        c + Vec3::new(0.0, -0.58 * s, 1.62 * s),
        Vec3::new(0.22 * s, 0.17 * s, 0.18 * s),
        PINK,
    );
    for side in [-1.0_f32, 1.0] {
        add_part(
            model,
            "mouth",
            head,
            c + Vec3::new(side * 0.20 * s, -0.90 * s, 1.58 * s),
            Vec3::new(0.22 * s, 0.05 * s, 0.05 * s),
            DARK,
        );
        for row in [-1.0_f32, 0.0, 1.0] {
            add_part(
                model,
                "whisker",
                head,
                c + Vec3::new(side * 1.73 * s, (-0.72 + row * 0.20) * s, 1.42 * s),
                Vec3::new(0.42 * s, 0.025 * s, 0.025 * s),
                DARK,
            );
        }
    }
}

fn add_markings(model: &mut BrickModel, head: BoneId, pelvis: BoneId, c: Vec3, s: f32) {
    use palette::*;
    for x in [-0.58_f32, 0.0, 0.58] {
        add_part(
            model,
            "forehead_stripe",
            head,
            c + Vec3::new(x * s, 1.10 * s, 1.18 * s),
            Vec3::new(0.13 * s, 0.34 * s, 0.05 * s),
            STRIPE,
        );
    }
    for z in [0.35_f32, 1.25, 2.15] {
        add_part(
            model,
            "back_stripe",
            pelvis,
            Vec3::new(0.0, 1.10 * s, z * s),
            Vec3::new(0.76 * s, 0.08 * s, 0.16 * s),
            STRIPE,
        );
    }
}

fn add_legs(model: &mut BrickModel, sk: &Skeleton, s: f32) {
    use palette::*;
    for (upper, lower, paw) in [
        ("upper_arm_l", "forearm_l", "paw_fl"),
        ("upper_arm_r", "forearm_r", "paw_fr"),
        ("thigh_l", "shin_l", "paw_bl"),
        ("thigh_r", "shin_r", "paw_br"),
    ] {
        let upper_bone = sk.bone(upper);
        add_part(
            model,
            "leg_upper",
            upper_bone.id,
            Vec3::new(0.0, -upper_bone.rest_length * 0.5, 0.0),
            Vec3::new(0.34 * s, upper_bone.rest_length * 0.48, 0.36 * s),
            ORANGE,
        );
        let lower_bone = sk.bone(lower);
        add_part(
            model,
            "leg_lower",
            lower_bone.id,
            Vec3::new(0.0, -lower_bone.rest_length * 0.5, 0.0),
            Vec3::new(0.31 * s, lower_bone.rest_length * 0.48, 0.33 * s),
            ORANGE_LIGHT,
        );
        add_part(
            model,
            "paw",
            sk.bone(paw).id,
            Vec3::new(0.0, -0.10 * s, 0.12 * s),
            Vec3::new(0.43 * s, 0.25 * s, 0.52 * s),
            CREAM,
        );
    }
}

fn add_tail(model: &mut BrickModel, sk: &Skeleton, s: f32) {
    use palette::*;
    for (index, name) in ["tail1", "tail2", "tail3", "tail4"].into_iter().enumerate() {
        let bone = sk.bone(name);
        add_part(
            model,
            "tail_segment",
            bone.id,
            Vec3::new(0.0, -bone.rest_length * 0.5, 0.0),
            Vec3::new(0.28 * s, bone.rest_length * 0.48, 0.28 * s),
            if index % 2 == 1 { STRIPE } else { ORANGE },
        );
    }
    let tip = sk.bone("tail4");
    add_part(
        model,
        "tail_tip",
        tip.id,
        Vec3::new(0.0, -tip.rest_length, 0.0),
        Vec3::splat(0.30 * s),
        CREAM,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_cat_has_expression_rig_and_beveled_silhouette() {
        let mut skeleton = Skeleton::chibi_cat(4.0);
        skeleton.solve_forward();
        let model = build_chibi_cat(&skeleton, 4.0);
        assert!(model.brick_count() >= 50);
        assert!(model.bricks.iter().all(|brick| brick.bevel > 0.0));
        assert!(model
            .bricks
            .iter()
            .any(|brick| brick.name == "eye_l_highlight"));
        assert!(model.bricks.iter().any(|brick| brick.name == "tail_tip"));
    }
}
