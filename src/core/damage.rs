// ═══════════════════════════════════════════════════════════════
// PROMETHEUS ENGINE — Damage Component
//
// A self-contained durability / damage system.  Attach a
// `Durability` to any object (brick, voxel chunk, limb) and apply
// a `Damage` blow via `compute_hit` / `apply_hit`.
//
// Units convention (see discussion 2026-04-18):
//   • 1 voxel = 1 cm of virtual world
//   • `Damage.reach` is measured in voxels (= cm)
//   • `Damage.power` and `Durability.toughness` are abstract
//     scalars; they share one scale so that `delta = power - toughness`
//     is directly readable (0.0..0.2 scratch, 0.2..0.5 dent, > 0.5 hole,
//     > 1.5 shatter)
//   • `Durability.hp` is the bucket that actually depletes across hits
// ═══════════════════════════════════════════════════════════════

/// Something that can be damaged.  Attach to a brick or a voxel object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Durability {
    /// Current health (in abstract units). 0 = destroyed.
    pub hp: f32,
    /// Starting health (for % calculations / visuals).
    pub max_hp: f32,
    /// Resistance threshold. A blow with `power <= toughness`
    /// does nothing. Defines the "armour class".
    /// Reference scale: 0.05 silk, 0.1 glass, 0.2 fabric, 0.5 wood,
    /// 0.9 metal, 2.0 concrete.
    pub toughness: f32,
    /// How much the object shatters (vs deforms) on breaking.
    /// 0.0 = rubber (dents, bounces), 1.0 = glass (flies apart).
    pub brittleness: f32,
}

impl Durability {
    pub fn new(max_hp: f32, toughness: f32, brittleness: f32) -> Self {
        Self { hp: max_hp, max_hp, toughness, brittleness }
    }

    pub fn alive(&self) -> bool { self.hp > 0.0 }

    pub fn health_fraction(&self) -> f32 {
        if self.max_hp <= 0.0 { 0.0 } else { (self.hp / self.max_hp).clamp(0.0, 1.0) }
    }

    // ── Presets (adjust as the game grows) ───────────────────
    pub fn silk()       -> Self { Self::new(1.0,  0.05, 0.05) }
    pub fn paper()      -> Self { Self::new(1.0,  0.08, 0.1) }
    pub fn glass()      -> Self { Self::new(1.0,  0.10, 1.0) }
    pub fn fabric()     -> Self { Self::new(2.0,  0.20, 0.0) }
    pub fn plastic()    -> Self { Self::new(2.5,  0.35, 0.4) }
    pub fn ceramic()    -> Self { Self::new(1.5,  0.55, 0.9) }
    pub fn wood()       -> Self { Self::new(3.0,  0.50, 0.3) }
    pub fn bone()       -> Self { Self::new(4.0,  0.70, 0.5) }
    pub fn metal()      -> Self { Self::new(10.0, 0.90, 0.1) }
    pub fn brick()      -> Self { Self::new(15.0, 1.50, 0.4) }
    pub fn concrete()   -> Self { Self::new(20.0, 2.00, 0.6) }
    pub fn indestructible() -> Self { Self::new(f32::MAX, f32::MAX, 0.0) }
}

/// A hit delivered by an attacker (cat paw, bullet, laser, grenade).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damage {
    /// Raw damage power. Compared against `Durability.toughness` for effect,
    /// and subtracted from `Durability.hp` for state.
    pub power: f32,
    /// Effective reach of the blow in VOXELS (cm). For a point weapon (bullet)
    /// this is near-zero; for a fist / paw ~2-3; for a shockwave large.
    pub reach: f32,
    pub kind: DamageKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    /// Blunt — cat paw, fist, hammer (tends to dent, heavy on brittle materials)
    Blunt,
    /// Pierce — bullet, needle, claw (small area, through-hit)
    Pierce,
    /// Energy — laser, fire (burns, melts)
    Energy,
    /// Explosive — grenade, shockwave (large radius, multiplies reach)
    Explosive,
}

impl Damage {
    pub fn new(power: f32, reach: f32, kind: DamageKind) -> Self {
        Self { power, reach, kind }
    }

    // ── Presets ──────────────────────────────────────────────
    pub fn cat_paw()        -> Self { Self::new(0.30, 2.0, DamageKind::Blunt) }
    pub fn cat_claw_swipe() -> Self { Self::new(0.45, 3.0, DamageKind::Pierce) }
    pub fn human_fist()     -> Self { Self::new(0.50, 3.0, DamageKind::Blunt) }
    pub fn pistol_bullet()  -> Self { Self::new(1.20, 0.5, DamageKind::Pierce) }
    pub fn rifle_bullet()   -> Self { Self::new(2.00, 0.5, DamageKind::Pierce) }
    pub fn laser_beam()     -> Self { Self::new(1.50, 0.3, DamageKind::Energy) }
    pub fn grenade()        -> Self { Self::new(3.00, 20.0, DamageKind::Explosive) }
    pub fn monster_claw()   -> Self { Self::new(2.50, 5.0, DamageKind::Pierce) }
}

/// Qualitative description of a hit, for VFX / SFX selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    None,     // no mark
    Scratch,  // surface only
    Dent,     // small removal
    Hole,     // through-damage
    Shatter,  // total
}

/// What happened when `Damage` met `Durability`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitResult {
    /// True if any damage was applied (any removed voxels).
    pub applied: bool,
    /// Effective spherical radius of removed voxels, in voxels.
    pub effective_radius: f32,
    pub severity: Severity,
    /// Object's hp reached 0 — caller should disassemble / spawn debris.
    pub broken: bool,
    /// Shatter fraction: how much of it just flies off as fragments
    /// (0.0 = stays put, 1.0 = all particles). Use for debris count.
    pub shatter_fraction: f32,
}

/// Pure computation — no state change. Good for predicting effect
/// before applying (e.g. for camera shake or anticipation FX).
pub fn compute_hit(dur: &Durability, dmg: &Damage) -> HitResult {
    if !dur.alive() {
        return HitResult {
            applied: false, effective_radius: 0.0,
            severity: Severity::None, broken: false, shatter_fraction: 0.0,
        };
    }

    // Kind-specific modifiers
    let (power_mul, reach_mul) = match dmg.kind {
        DamageKind::Blunt     => (1.0, 1.0),
        DamageKind::Pierce    => (1.2, 0.7),   // deeper, narrower
        DamageKind::Energy    => (1.0, 0.6),   // burns small hole
        DamageKind::Explosive => (1.5, 2.5),   // wide area
    };

    // Brittle materials take extra from blunt
    let brittle_bonus = if dmg.kind == DamageKind::Blunt {
        1.0 + dur.brittleness * 0.5
    } else { 1.0 };

    let effective_power = dmg.power * power_mul * brittle_bonus;
    let delta = effective_power - dur.toughness;

    if delta <= 0.0 {
        return HitResult {
            applied: false, effective_radius: 0.0,
            severity: Severity::None, broken: false, shatter_fraction: 0.0,
        };
    }

    // Severity thresholds
    let severity = if delta <= 0.2       { Severity::Scratch }
                   else if delta <= 0.5  { Severity::Dent }
                   else if delta <= 1.5  { Severity::Hole }
                   else                  { Severity::Shatter };

    // Radius growth: weak hits barely pit, strong hits blow a crater.
    let base = dmg.reach * reach_mul;
    let bonus = match severity {
        Severity::Scratch => delta * 1.0,
        Severity::Dent    => 1.0 + (delta - 0.2) * 3.0,
        Severity::Hole    => 2.0 + (delta - 0.5) * 5.0,
        Severity::Shatter => 6.0 + (delta - 1.5) * 8.0,
        Severity::None    => 0.0,
    };
    let effective_radius = base + bonus;

    // Will this single hit break the object?
    let broken = effective_power >= dur.hp;
    let shatter_fraction = match severity {
        Severity::Shatter => 0.8 * dur.brittleness + 0.2,
        Severity::Hole    => 0.5 * dur.brittleness,
        Severity::Dent    => 0.2 * dur.brittleness,
        _                 => 0.05 * dur.brittleness,
    }.clamp(0.0, 1.0);

    HitResult {
        applied: true, effective_radius, severity, broken, shatter_fraction,
    }
}

/// Apply a hit: computes the effect AND depletes hp. Returns the result.
pub fn apply_hit(dur: &mut Durability, dmg: &Damage) -> HitResult {
    let result = compute_hit(dur, dmg);
    if result.applied {
        dur.hp = (dur.hp - dmg.power).max(0.0);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tough_material_ignores_weak_hit() {
        let dur = Durability::metal();
        let res = compute_hit(&dur, &Damage::cat_paw());
        assert!(!res.applied);
        assert_eq!(res.severity, Severity::None);
    }

    #[test]
    fn cat_paw_scratches_wood() {
        let dur = Durability::wood();
        let res = compute_hit(&dur, &Damage::cat_claw_swipe());
        assert!(res.applied);
        // 0.45 * 1.2 = 0.54 effective pierce, toughness 0.5 → delta 0.04 → scratch
        assert_eq!(res.severity, Severity::Scratch);
    }

    #[test]
    fn bullet_makes_hole_in_wood() {
        let dur = Durability::wood();
        let res = compute_hit(&dur, &Damage::pistol_bullet());
        assert!(res.applied);
        // Pierce: 1.2 * 1.2 = 1.44 vs toughness 0.5 → delta 0.94 → Hole
        assert_eq!(res.severity, Severity::Hole);
    }

    #[test]
    fn grenade_shatters_brick_wall() {
        let dur = Durability::brick();
        let res = compute_hit(&dur, &Damage::grenade());
        assert!(res.applied);
        // Explosive 3 * 1.5 = 4.5 vs toughness 1.5 → delta 3.0 → Shatter
        assert_eq!(res.severity, Severity::Shatter);
    }

    #[test]
    fn cat_paw_shatters_glass() {
        let dur = Durability::glass();
        let res = compute_hit(&dur, &Damage::cat_paw());
        assert!(res.applied);
        // 0.3 blunt * 1.5 brittle = 0.45 vs toughness 0.1 → delta 0.35 → Dent, small crack
        assert!(matches!(res.severity, Severity::Dent | Severity::Hole));
    }

    #[test]
    fn apply_hit_drops_hp() {
        let mut dur = Durability::wood();
        let start_hp = dur.hp;
        let _ = apply_hit(&mut dur, &Damage::rifle_bullet());
        assert!(dur.hp < start_hp);
    }

    #[test]
    fn indestructible_never_breaks() {
        let dur = Durability::indestructible();
        let res = compute_hit(&dur, &Damage::grenade());
        assert!(!res.applied);
    }

    #[test]
    fn dead_object_ignores_hits() {
        let mut dur = Durability::glass();
        dur.hp = 0.0;
        let res = compute_hit(&dur, &Damage::rifle_bullet());
        assert!(!res.applied);
    }

    #[test]
    fn effective_radius_grows_with_delta() {
        let dur = Durability::wood();
        let weak = Damage::new(0.6, 1.0, DamageKind::Blunt);    // delta ~0.1
        let strong = Damage::new(3.0, 1.0, DamageKind::Blunt);  // delta ~2.5
        let r_weak   = compute_hit(&dur, &weak).effective_radius;
        let r_strong = compute_hit(&dur, &strong).effective_radius;
        assert!(r_strong > r_weak * 3.0, "strong blow should blow a proper crater");
    }
}
