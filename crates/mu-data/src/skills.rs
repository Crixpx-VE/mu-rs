//! The skill table (`SkillAttribute[]`): `Data/Local/Eng/skill_eng.bmd`, read like `OpenSkillScript`
//! (ZzzInfomation.cpp:252-290): `MAX_SKILLS` records, each one `BuxConvert`ed (XOR `FC CF AB`), and a
//! checksum `DWORD` at the end (not verified: `GenerateCheckSum2` is not in the source).
//!
//! Two layouts are understood, told apart by the file size:
//! - Main 5.2's `SKILL_ATTRIBUTE` (_struct.h:302-327): 600 records of 80 bytes (`Name[32]`, `Level` as a
//!   `BYTE`, `Distance` as a `BYTE`...).
//! - MuMain's `SKILL_ATTRIBUTE_FILE` (SkillStructs.h, SkillFieldDefs.h): 650 records of 108 bytes with a
//!   50-byte UTF-8 name, `Level` as a `WORD`, `Distance` and `SkillBrand` as `DWORD`s and four extra
//!   fields at the end (`SkillRank`, `ItemSkill`, `IsDamage`, `Effect`, unused here). The client data
//!   that ships with this project has this one.
//!
//! The record index is the skill number (`AT_SKILL_*`), the same one the server sends in the skill list.

use std::path::Path;

use crate::warps::bux_convert;
use crate::{Error, Result, fs};

/// `MAX_SKILLS` of Main 5.2 (_define.h:386).
pub const MAX_SKILLS_MAIN52: usize = 600;
/// `MAX_SKILLS` of MuMain.
pub const MAX_SKILLS_MUMAIN: usize = 650;

/// `MAX_CLASS` (7): the classes of `RequireClass` (DW, DK, Elf, MG, DL, Summoner, RF).
pub const MAX_CLASS: usize = 7;
/// `MAX_DUTY_CLASS` (3): the evolutions of `RequireDutyClass`.
pub const MAX_DUTY_CLASS: usize = 3;

/// `SkillUseType` (`SKILL_USE_TYPE_*`, _define.h).
pub mod use_type {
    pub const NONE: u8 = 0;
    pub const MASTER: u8 = 1;
    pub const BRAND: u8 = 2;
    pub const MASTERLEVEL: u8 = 3;
    pub const MASTERACTIVE: u8 = 4;
}

/// One record of the table (`SKILL_ATTRIBUTE`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillDef {
    pub name: String,
    /// Required level (and the factor of the required energy).
    pub level: u16,
    pub damage: u16,
    pub mana: u16,
    /// `AbilityGuage`: the AG it costs.
    pub ability: u16,
    /// In tiles.
    pub distance: u32,
    /// Cooldown in milliseconds (`SkillDelay`).
    pub delay: i32,
    /// The energy factor (see [`SkillDef::required_energy`]).
    pub energy: i32,
    /// Required leadership (`Charisma`).
    pub charisma: u16,
    pub mastery_type: u8,
    /// `SkillUseType`: see [`use_type`].
    pub use_type: u8,
    pub brand: u32,
    pub kill_count: u8,
    pub require_duty_class: [u8; MAX_DUTY_CLASS],
    pub require_class: [u8; MAX_CLASS],
    /// `Magic_Icon`: the icon of the master skills (`SkillUseType` 4).
    pub magic_icon: u16,
    /// `TypeSkill` (`eTypeSkill`).
    pub type_skill: u8,
    pub strength: i32,
    pub dexterity: i32,
}

impl SkillDef {
    /// `GetSkillInformation_Energy` (SkillManager.cpp:58-79): the energy the tooltip asks for;
    /// `knight` = the hero is a Dark Knight (`CLASS_KNIGHT`), `summon_curse` = the skill is
    /// `AT_SKILL_SUMMON_EXPLOSION` or `AT_SKILL_SUMMON_REQUIEM`.
    pub fn required_energy(&self, knight: bool, summon_curse: bool) -> i32 {
        if self.energy == 0 {
            return 0;
        }
        let l = self.level as i32;
        if knight {
            10 + self.energy * l * 4 / 100
        } else if summon_curse {
            20 + self.energy * l * 3 / 100
        } else {
            20 + self.energy * l * 4 / 100
        }
    }

    /// `DemendConditionCheckSkill` (SkillManager.cpp:415-458) for this (base) skill: whether the hero's
    /// stats are enough. Skills without an energy factor always pass. The original's `operator<=`
    /// (SkillManager.h:222-226) compares strength, agility, vitality (0 here), energy and
    /// leadership, not the level; the required energy is `WORD(20 + Energy·Level·0.04)`.
    pub fn demand_ok(&self, hero: &HeroStats) -> bool {
        if self.energy == 0 {
            return true;
        }
        let energy = (20.0 + (self.energy as f64 * self.level as f64) * 0.04) as u16;
        let s = self.strength as u16;
        let d = self.dexterity as u16;
        s <= hero.strength && d <= hero.dexterity && energy <= hero.energy && self.charisma <= hero.charisma
    }
}

/// What [`SkillDef::demand_ok`] compares: the hero's stats with their item bonuses (`Strength +
/// AddStrength`...).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeroStats {
    pub level: u16,
    pub strength: u16,
    pub dexterity: u16,
    pub vitality: u16,
    pub energy: u16,
    pub charisma: u16,
}

/// The whole table, indexed by skill number.
#[derive(Debug, Clone, Default)]
pub struct SkillTable(pub Vec<SkillDef>);

impl SkillTable {
    pub fn get(&self, skill: u16) -> Option<&SkillDef> {
        self.0.get(skill as usize)
    }
}

fn u16_at(r: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([r[i], r[i + 1]])
}

fn u32_at(r: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([r[i], r[i + 1], r[i + 2], r[i + 3]])
}

fn name_of(b: &[u8], utf8: bool) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    if utf8 {
        String::from_utf8_lossy(&b[..end]).into_owned()
    } else {
        // The original's ANSI code page (the English names are plain ASCII).
        b[..end].iter().map(|&c| c as char).collect()
    }
}

/// Main 5.2's 80-byte record (MSVC layout of `SKILL_ATTRIBUTE`, `MAX_DUTY_CLASS` 3, `MAX_CLASS` 7).
fn read_main52(r: &[u8]) -> SkillDef {
    SkillDef {
        name: name_of(&r[..32], false),
        level: r[32] as u16,
        damage: u16_at(r, 34),
        mana: u16_at(r, 36),
        ability: u16_at(r, 38),
        distance: r[40] as u32,
        delay: u32_at(r, 44) as i32,
        energy: u32_at(r, 48) as i32,
        charisma: u16_at(r, 52),
        mastery_type: r[54],
        use_type: r[55],
        brand: r[56] as u32,
        kill_count: r[57],
        require_duty_class: r[58..61].try_into().unwrap(),
        require_class: r[61..68].try_into().unwrap(),
        magic_icon: u16_at(r, 68),
        type_skill: r[70],
        strength: u32_at(r, 72) as i32,
        dexterity: u32_at(r, 76) as i32,
    }
}

/// MuMain's 108-byte record (`SKILL_ATTRIBUTE_FILE` with `MAX_SKILL_NAME` 50).
fn read_mumain(r: &[u8]) -> SkillDef {
    SkillDef {
        name: name_of(&r[..50], true),
        level: u16_at(r, 50),
        damage: u16_at(r, 52),
        mana: u16_at(r, 54),
        ability: u16_at(r, 56),
        distance: u32_at(r, 60),
        delay: u32_at(r, 64) as i32,
        energy: u32_at(r, 68) as i32,
        charisma: u16_at(r, 72),
        mastery_type: r[74],
        use_type: r[75],
        brand: u32_at(r, 76),
        kill_count: r[80],
        require_duty_class: r[81..84].try_into().unwrap(),
        require_class: r[84..91].try_into().unwrap(),
        magic_icon: u16_at(r, 92),
        type_skill: r[94],
        strength: u32_at(r, 96) as i32,
        dexterity: u32_at(r, 100) as i32,
    }
}

/// Reads a `skill_eng.bmd` in either layout.
pub fn read_skills(buf: &[u8]) -> Result<SkillTable> {
    let (len, count, read): (usize, usize, fn(&[u8]) -> SkillDef) = match buf.len() {
        n if n == MAX_SKILLS_MAIN52 * 80 + 4 => (80, MAX_SKILLS_MAIN52, read_main52),
        n if n == MAX_SKILLS_MUMAIN * 108 + 4 => (108, MAX_SKILLS_MUMAIN, read_mumain),
        n => return Err(Error::Format(format!("skill.bmd: {n} bytes, unknown layout"))),
    };
    let table: Vec<SkillDef> = buf
        .chunks_exact(len)
        .take(count)
        .map(|c| {
            let mut r = c.to_vec();
            bux_convert(&mut r);
            read(&r)
        })
        .collect();
    // A file with another cipher (MuMain's `Local/Skill.bmd`) gives names without their zero.
    if table.iter().take(64).any(|s| s.name.chars().count() >= if len == 80 { 32 } else { 50 }) {
        return Err(Error::Format("skill.bmd: not BuxConvert-ciphered".into()));
    }
    Ok(SkillTable(table))
}

/// The table of the `Data` folder: `Local/Eng/skill_eng.bmd` (`Data\Local\%s\Skill_%s.bmd`,
/// ZzzOpenData.cpp:5433-5434).
pub fn load_skills(data: &Path) -> Result<SkillTable> {
    read_skills(&fs::read(&data.join("Local").join("Eng"), "skill_eng.bmd")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main52_layout() {
        // Poison as Main 5.2's `skill_eng.bmd` has it (after `BuxConvert`).
        let mut file = vec![0u8; 600 * 80 + 4];
        let r = &mut file[80..160];
        r[..6].copy_from_slice(b"Poison");
        r[32] = 30;
        r[34..36].copy_from_slice(&12u16.to_le_bytes());
        r[36..38].copy_from_slice(&42u16.to_le_bytes());
        r[40] = 6;
        r[48..52].copy_from_slice(&100u32.to_le_bytes());
        r[54] = 1;
        r[61] = 1;
        r[64] = 1;
        r[70] = 2;
        for c in file[..600 * 80].chunks_exact_mut(80) {
            bux_convert(c);
        }
        let t = read_skills(&file).unwrap();
        let p = t.get(1).unwrap();
        assert_eq!((p.name.as_str(), p.level, p.damage, p.mana, p.distance, p.energy), ("Poison", 30, 12, 42, 6, 100));
        assert_eq!(p.require_class, [1, 0, 0, 1, 0, 0, 0]);
        assert_eq!(p.type_skill, 2);
        assert_eq!(t.get(0).unwrap().name, "");
    }

    #[test]
    fn energy_rules() {
        let s = SkillDef { level: 30, energy: 100, ..Default::default() };
        assert_eq!(s.required_energy(false, false), 20 + 100 * 30 * 4 / 100);
        assert_eq!(s.required_energy(true, false), 10 + 120);
        assert_eq!(s.required_energy(false, true), 20 + 90);
        assert_eq!(SkillDef::default().required_energy(false, false), 0);
        // `DemendConditionCheckSkill`: 20 + 30·100·0.04 = 140 energy.
        let hero = HeroStats { energy: 139, ..Default::default() };
        assert!(!s.demand_ok(&hero));
        assert!(s.demand_ok(&HeroStats { energy: 140, ..hero }));
        assert!(SkillDef::default().demand_ok(&hero));
    }

    /// The real files (`MU_DATA=... cargo test -p mu-data -- --ignored`): the client's (MuMain's layout)
    /// and, if present, Main 5.2's, which must agree on the classic skills.
    #[test]
    #[ignore]
    fn real_tables() {
        let data = std::path::PathBuf::from(std::env::var_os("MU_DATA").expect("MU_DATA"));
        let t = load_skills(&data).unwrap();
        let p = t.get(1).unwrap();
        assert_eq!((p.name.as_str(), p.level, p.mana, p.distance), ("Poison", 30, 42, 6));
        let twist = t.get(41).unwrap();
        assert_eq!((twist.name.as_str(), twist.mana, twist.ability, twist.distance), ("Twisting Slash", 10, 10, 2));
        let main52 = data.join("../../../referencias/Main5.2/Source Main 5.2/bin/Data");
        if let Ok(m) = load_skills(&main52) {
            for i in [1u16, 2, 3, 17, 19, 41, 42, 43, 44, 47, 52] {
                let (a, b) = (t.get(i).unwrap(), m.get(i).unwrap());
                assert_eq!((&a.name, a.mana, a.ability, a.distance, a.delay), (&b.name, b.mana, b.ability, b.distance, b.delay), "skill {i}");
            }
        }
    }
}
