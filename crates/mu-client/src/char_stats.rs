//! The client-side character stat formulas (`CHARACTER_MACHINE`, ZzzInfomation.cpp of
//! Main 5.2): damage, magic and curse damage, attack rate, speed, defense and their PvP
//! versions. The server only sends the points (`CharacterInformation`); the client computes the
//! rest with `CalculateAll` (ZzzInfomation.cpp:3958-4046).
//!
//! The fields are `WORD` in the original (`CHARACTER_ATTRIBUTE`, _struct.h:346-406): here `u16`,
//! and sums wrap around the same way.
//!
//! **Equipment:** there are no items yet. What weapons, wings, rings, sets, jewels of harmony,
//! 380 options, sockets, etc. add comes in through [`EquipBonus`] already computed (per hand and
//! per field, at the point of the original formula where it is added); by default, unequipped,
//! everything is zero and the left hand damage is used (`r->Type == -1`, ZzzInventory.cpp:5980-5984).
//!
//! **No buffs:** scrolls, potions, `ABILITY_PLUS_DAMAGE`, etc. don't exist yet; the original
//! adds them at the same points.
//!
//! The Rage Fighter is inside `#ifdef PBG_ADD_NEWCHAR_MONK` in Main 5.2 (disabled there, as in
//! account.rs): its formulas are the ones in those blocks.

use crate::account::Class;

/// The character's points (`Character.Strength`...), as the server sends them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Points {
    pub level: u16,
    pub strength: u16,
    /// `Dexterity`.
    pub agility: u16,
    pub vitality: u16,
    pub energy: u16,
    /// `Charisma` (Dark Lord command).
    pub command: u16,
}

/// Which hand's damage `GetAttackDamage` shows (ZzzInventory.cpp:5950-6040).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[allow(dead_code)] // Without equipment only `Left` occurs yet.
pub enum AttackHand {
    /// No weapon on the right, a staff on the right or a bow: the left one.
    #[default]
    Left,
    /// A weapon on the right or a crossbow: the right one.
    Right,
    /// Two weapons: Dark Knight and Magic Gladiator 55 % of each, Rage Fighter 60 % and 65 %
    /// of the sum (5997-6026). Shown in blue (`Alpha`).
    TwoWeapons,
    /// Bow and arrows with +1 or more, which add damage (6006-6016): only the color changes.
    BowWithAmmo,
}

/// What equipment adds to each formula. Defaults to no equipment.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct EquipBonus {
    /// `Character.AddStrength`, `AddDexterity`, `AddVitality`, `AddEnergy`, `AddCharisma`
    /// (set options, sockets, Dark Lord wings...: `CalculateAll`, 3958-4000).
    pub add_strength: u16,
    pub add_agility: u16,
    pub add_vitality: u16,
    pub add_energy: u16,
    pub add_command: u16,
    /// Bow on the left or crossbow on the right with durability: the bow formula
    /// (`CalculateDamage`, 2683-2689).
    pub bow: bool,
    /// Damage (min, max) that wings, weapons, pendant, rings, set and sockets add to the right
    /// and left hand (2761-2943).
    pub damage_right: (u16, u16),
    pub damage_left: (u16, u16),
    pub hand: AttackHand,
    /// `CalculateMagicDamage` (2997-3175) and `CalculateCurseDamage` (3187-3235).
    pub magic_damage: (u16, u16),
    pub curse_damage: (u16, u16),
    /// `CalculateAttackRating`: set and sockets (3258-3260).
    pub attack_rating: u16,
    /// `CalculateAttackSpeed`: weapons, gloves, rings, pets, options (3340-3564).
    pub attack_speed: u16,
    pub magic_speed: u16,
    /// `CalculateSuccessfulBlocking`: shield and options (3631-3656).
    pub blocking: u16,
    /// Socket `m_fDefenceRateBonus` (3658); 1 without sockets.
    pub blocking_factor: Option<f32>,
    /// `CalculateDefense`: each piece, set, wings... (3697-3797).
    pub defense: u16,
    /// Full set in good condition (`bDexSuccess`, NewUICharacterInfoWindow.cpp:861-940): the
    /// defense shows the 10 % bonus.
    pub full_set: bool,
    /// What the window adds on top of `GetAttackDamage`: transformation rings, pets and
    /// harmony (`iMinIndex`/`iMaxIndex`, NewUICharacterInfoWindow.cpp:585-768).
    pub window_damage: (i32, i32),
    /// Harmony and 380 option of PvP attack rate and defense (the `(+%d)` of 2109 and 2110).
    pub pvp_attack_bonus: i32,
    pub pvp_defense_bonus: i32,
    /// Defense harmony (`maxdefense`, 964-980) plus what the transformation ring and pets add
    /// (`iChangeRingAddDefense`, 982-995).
    pub harmony_defense: i32,
    pub ring_defense: i32,
    /// What rings and pets (1329-1399) and harmony (`maxMg`, 1311-1327) add to magic damage.
    pub window_magic: (i32, i32),
    pub harmony_magic: i32,
    /// What rings and pets add to curse damage (1467-1515).
    pub window_curse: (i32, i32),
    /// A staff (or magic sword) on the right: its `MagicPower` after wear, as a fraction
    /// (the `(+%d)` of `GlobalText[215]`, 1409-1425).
    pub staff_magic: Option<f32>,
    /// A summoning book on the left (the `(+%d)` of `GlobalText[1693]`, 1517-1530).
    pub book_curse: Option<f32>,
}

/// What `CalculateAll` stores in `Character`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Derived {
    /// Strength, agility, vitality, energy and command including equipment (`Strength + AddStrength`).
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub energy: u16,
    pub command: u16,
    pub damage_min_right: u16,
    pub damage_max_right: u16,
    pub damage_min_left: u16,
    pub damage_max_left: u16,
    pub magic_min: u16,
    pub magic_max: u16,
    pub curse_min: u16,
    pub curse_max: u16,
    pub attack_rating: u16,
    pub attack_rating_pk: u16,
    pub attack_speed: u16,
    pub magic_speed: u16,
    pub blocking: u16,
    pub blocking_pk: u16,
    pub defense: u16,
}

/// `(WORD)tmpf` of the formulas with decimals (`float` in the original).
fn word(f: f32) -> u16 {
    f as i64 as u16
}

/// `CalculateAll` without buffs (ZzzInfomation.cpp:3958-4014).
pub fn calculate(class: Class, p: &Points, eq: &EquipBonus) -> Derived {
    let s = p.strength.wrapping_add(eq.add_strength);
    let d = p.agility.wrapping_add(eq.add_agility);
    let v = p.vitality.wrapping_add(eq.add_vitality);
    let e = p.energy.wrapping_add(eq.add_energy);
    let c = p.command.wrapping_add(eq.add_command);
    let (s32, d32, v32, e32, c32, l32) = (s as u32, d as u32, v as u32, e as u32, c as u32, p.level as u32);
    let level = p.level as f32;
    let mut out = Derived { strength: s, agility: d, vitality: v, energy: e, command: c, ..Default::default() };

    // `CalculateDamage` (2662-2759).
    let (min, max) = if eq.bow {
        (d32 / 7 + s32 / 14, d32 / 4 + s32 / 8)
    } else {
        match class {
            Class::Elf | Class::Summoner => ((s32 + d32) / 7, (s32 + d32) / 4),
            // `minValue = 6; maxValue = 4` (2706).
            Class::Knight => (s32 / 6, s32 / 4),
            // `minValue[0] = 6; minValue[1] = 12; maxValue[0] = 4; maxValue[1] = 9` (2720-2721).
            Class::Gladiator => (s32 / 6 + e32 / 12, s32 / 4 + e32 / 9),
            Class::Lord => (s32 / 7 + e32 / 14, s32 / 5 + e32 / 10),
            Class::Fighter => (s32 / 7 + v32 / 15, s32 / 5 + v32 / 12),
            Class::Wizard => (s32 / 8, s32 / 4),
        }
    };
    out.damage_min_right = (min as u16).wrapping_add(eq.damage_right.0);
    out.damage_max_right = (max as u16).wrapping_add(eq.damage_right.1);
    out.damage_min_left = (min as u16).wrapping_add(eq.damage_left.0);
    out.damage_max_left = (max as u16).wrapping_add(eq.damage_left.1);

    // `CalculateMagicDamage` (2980-2985) and `CalculateCurseDamage` (3178-3185, Summoner only).
    out.magic_min = ((e32 / 9) as u16).wrapping_add(eq.magic_damage.0);
    out.magic_max = ((e32 / 4) as u16).wrapping_add(eq.magic_damage.1);
    if class == Class::Summoner {
        out.curse_min = ((e32 / 9) as u16).wrapping_add(eq.curse_damage.0);
        out.curse_max = ((e32 / 4) as u16).wrapping_add(eq.curse_damage.1);
    }

    // `CalculateAttackRating` (3239-3261).
    let rating = match class {
        Class::Lord => l32 * 5 + d32 * 5 / 2 + s32 / 6 + c32 / 10,
        Class::Fighter => l32 * 3 + d32 * 5 / 4 + s32 / 6,
        _ => l32 * 5 + d32 * 3 / 2 + s32 / 4,
    };
    out.attack_rating = (rating as u16).wrapping_add(eq.attack_rating);

    // `CalculateAttackSpeed` (3298-3338).
    let (attack, magic) = match class {
        Class::Elf => (d32 / 50, d32 / 50),
        Class::Knight | Class::Gladiator => (d32 / 15, d32 / 20),
        Class::Lord => (d32 / 10, d32 / 10),
        Class::Summoner => (d32 / 20, d32 / 20),
        Class::Fighter => (d32 / 9, d32 / 9),
        Class::Wizard => (d32 / 20, d32 / 10),
    };
    out.attack_speed = (attack as u16).wrapping_add(eq.attack_speed);
    out.magic_speed = (magic as u16).wrapping_add(eq.magic_speed);

    // `CalculateSuccessfulBlocking` (3604-3659).
    let blocking = match class {
        Class::Elf | Class::Summoner => d32 / 4,
        Class::Lord => d32 / 7,
        Class::Fighter => d32 / 10,
        _ => d32 / 3,
    };
    let blocking = (blocking as u16).wrapping_add(eq.blocking);
    out.blocking = match eq.blocking_factor {
        Some(f) => word(blocking as f32 * f),
        None => blocking,
    };

    // `CalculateDefense` (3661-3695).
    let defense = match class {
        Class::Elf => d32 / 10,
        Class::Knight | Class::Summoner => d32 / 3,
        Class::Wizard => d32 / 4,
        Class::Lord => d32 / 7,
        Class::Fighter => d32 / 8,
        Class::Gladiator => d32 / 5,
    };
    out.defense = (defense as u16).wrapping_add(eq.defense);

    // `CalculateSuccessfulBlockingPK` (3568-3602) and `CalculateAttackRatingPK` (3263-3296), in `float`.
    let df = d as f32;
    out.blocking_pk = word(match class {
        Class::Knight | Class::Lord | Class::Summoner => level * 2.0 + df * 0.5,
        Class::Elf => level * 2.0 + df * 0.1,
        Class::Gladiator | Class::Wizard => level * 2.0 + df * 0.25,
        Class::Fighter => level * 1.5 + df * 0.2,
    });
    out.attack_rating_pk = word(match class {
        Class::Knight => level * 3.0 + df * 4.5,
        Class::Lord | Class::Wizard => level * 3.0 + df * 4.0,
        Class::Elf => level * 3.0 + df * 0.6,
        Class::Gladiator | Class::Summoner => level * 3.0 + df * 3.5,
        Class::Fighter => level * 2.6 + df * 3.6,
    });
    out
}

/// `GetAttackDamage` (ZzzInventory.cpp:5950-6040): the displayed damage and whether it is blue.
pub fn attack_damage(class: Class, d: &Derived, hand: AttackHand) -> (i32, i32, bool) {
    let (rmin, rmax, lmin, lmax) =
        (d.damage_min_right as i32, d.damage_max_right as i32, d.damage_min_left as i32, d.damage_max_left as i32);
    match (hand, class) {
        (AttackHand::TwoWeapons, Class::Knight | Class::Gladiator) => (rmin * 55 / 100 + lmin * 55 / 100, rmax * 55 / 100 + lmax * 55 / 100, true),
        (AttackHand::TwoWeapons, Class::Fighter) => ((rmin + lmin) * 60 / 100, (rmax + lmax) * 65 / 100, true),
        (AttackHand::BowWithAmmo, Class::Elf) => (lmin, lmax, true),
        (AttackHand::Right | AttackHand::TwoWeapons, _) => (rmin, rmax, false),
        _ => (lmin, lmax, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(level: u16, s: u16, a: u16, v: u16, e: u16, c: u16) -> Points {
        Points { level, strength: s, agility: a, vitality: v, energy: e, command: c }
    }

    fn calc(class: Class, p: Points) -> Derived {
        calculate(class, &p, &EquipBonus::default())
    }

    #[test]
    fn dark_knight_recien_creado() {
        // 28/20/25/10 (account.rs).
        let d = calc(Class::Knight, pts(1, 28, 20, 25, 10, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (28 / 6, 28 / 4));
        assert_eq!(d.attack_rating, 5 + 30 + 7);
        assert_eq!(d.attack_rating_pk, 93, "1·3 + 20·4.5");
        assert_eq!((d.defense, d.blocking, d.blocking_pk), (6, 6, 12));
        assert_eq!((d.attack_speed, d.magic_speed), (1, 1));
        assert_eq!(attack_damage(Class::Knight, &d, AttackHand::Left), (4, 7, false));
    }

    #[test]
    fn dark_knight_nivel_100() {
        let d = calc(Class::Knight, pts(100, 400, 200, 200, 50, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (66, 100));
        assert_eq!(d.attack_rating, 500 + 300 + 100);
        assert_eq!(d.attack_rating_pk, 300 + 900);
        assert_eq!((d.defense, d.blocking, d.blocking_pk), (66, 66, 300));
        assert_eq!((d.attack_speed, d.magic_speed), (13, 10));
    }

    #[test]
    fn dark_wizard_y_summoner() {
        let d = calc(Class::Wizard, pts(1, 18, 18, 15, 30, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (2, 4));
        assert_eq!((d.magic_min, d.magic_max), (3, 7));
        assert_eq!((d.curse_min, d.curse_max), (0, 0), "solo la Summoner");
        assert_eq!((d.attack_rating, d.attack_rating_pk), (5 + 27 + 4, 3 + 72));
        assert_eq!((d.defense, d.blocking, d.blocking_pk), (4, 6, 6));
        assert_eq!((d.attack_speed, d.magic_speed), (0, 1));

        let d = calc(Class::Summoner, pts(1, 21, 21, 18, 23, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (6, 10));
        assert_eq!((d.magic_min, d.magic_max, d.curse_min, d.curse_max), (2, 5, 2, 5));
        assert_eq!((d.defense, d.blocking, d.blocking_pk), (7, 5, 12));
        assert_eq!((d.attack_speed, d.magic_speed), (1, 1));
    }

    #[test]
    fn elfa_gladiador_y_lord() {
        let d = calc(Class::Elf, pts(1, 22, 25, 20, 15, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (47 / 7, 47 / 4));
        assert_eq!((d.attack_rating, d.attack_rating_pk), (5 + 37 + 5, 18));
        assert_eq!((d.defense, d.blocking, d.blocking_pk, d.attack_speed), (2, 6, 4, 0));

        let d = calc(Class::Gladiator, pts(1, 26, 26, 26, 26, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (4 + 2, 6 + 2));
        assert_eq!((d.attack_rating, d.attack_rating_pk), (5 + 39 + 6, 94));
        assert_eq!((d.defense, d.blocking, d.blocking_pk, d.attack_speed), (5, 8, 8, 1));

        let d = calc(Class::Lord, pts(1, 26, 20, 20, 15, 25));
        assert_eq!((d.damage_min_left, d.damage_max_left), (3 + 1, 5 + 1));
        assert_eq!((d.attack_rating, d.attack_rating_pk), (5 + 50 + 4 + 2, 83));
        assert_eq!((d.defense, d.blocking, d.blocking_pk, d.attack_speed), (2, 2, 12, 2));
    }

    #[test]
    fn rage_fighter() {
        let d = calc(Class::Fighter, pts(1, 32, 27, 25, 20, 0));
        assert_eq!((d.damage_min_left, d.damage_max_left), (4 + 1, 6 + 2));
        assert_eq!(d.attack_rating, 3 + 33 + 5);
        assert_eq!(d.attack_rating_pk, 99, "2.6 + 97.2 en float");
        assert_eq!((d.defense, d.blocking, d.blocking_pk, d.attack_speed), (3, 2, 6, 3));
    }

    #[test]
    fn sumas_en_word() {
        // `& 0xFFFF` of the Dark Lord's attack rate with high agility.
        let d = calc(Class::Lord, pts(400, 32767, 32767, 100, 100, 100));
        let full = 400 * 5 + 32767 * 5 / 2 + 32767 / 6 + 100 / 10;
        assert_eq!(d.attack_rating as u32, full & 0xFFFF);
        // Equipment points are added to the server's.
        let eq = EquipBonus { add_strength: 12, damage_right: (10, 20), hand: AttackHand::Right, ..Default::default() };
        let d = calculate(Class::Knight, &pts(1, 28, 20, 25, 10, 0), &eq);
        assert_eq!(d.strength, 40);
        assert_eq!(attack_damage(Class::Knight, &d, eq.hand), (40 / 6 + 10, 40 / 4 + 20, false));
    }

    #[test]
    fn dos_armas() {
        let eq = EquipBonus { damage_right: (20, 30), damage_left: (10, 20), hand: AttackHand::TwoWeapons, ..Default::default() };
        let d = calculate(Class::Knight, &pts(1, 28, 20, 25, 10, 0), &eq);
        // (24·55/100 + 14·55/100, 37·55/100 + 27·55/100).
        assert_eq!(attack_damage(Class::Knight, &d, eq.hand), (13 + 7, 20 + 14, true));
        let d = calculate(Class::Fighter, &pts(1, 32, 27, 25, 20, 0), &eq);
        assert_eq!(attack_damage(Class::Fighter, &d, eq.hand), ((25 + 15) * 60 / 100, (38 + 28) * 65 / 100, true));
    }
}
