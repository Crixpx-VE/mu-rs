//! S6 items: what the original knows about each type (`ITEM_ATTRIBUTE`) and each concrete
//! item (`ITEM`).
//!
//! - [`ItemTable`]: `Data\Local\Eng\item_eng.bmd`, as read by `OpenItemScript` (ZzzInfomation.cpp:386-420,
//!   path from ZzzOpenData.cpp:5421): `MAX_ITEM` (16 × 512) records of `sizeof(ITEM_ATTRIBUTE)` = 84 bytes
//!   (`_struct.h:150-182` with `MAX_CLASS` = 6), each with `BuxConvert` (XOR `FC CF AB`), and a checksum
//!   `DWORD` at the end. The checksum is not verified: `GenerateCheckSum2` is not in the source.
//!   The `dist` data is MuMain's, which does not ship that file: without it, its
//!   `Items/GroupNN_*.json` are read (same values, different format).
//! - [`Item`]: a concrete item (type, level, options) and its 12 bytes in the S6 packet
//!   (`ConvertItemType` and `ItemConvert`, ZzzInfomation.cpp:379-384 and 566-1573).
//! - [`ItemStats`]: what `ItemConvert` computes (damage, defense, requirements and options with their value).
//! - [`model`]: which model each item draws and how it sits in an inventory cell.
//! - [`glow`]: the level, excellent and ancient glow passes (`RenderPartObjectEffect`).
//! - [`info`]: the lines of the info tooltip (`RenderItemInfo`).

pub mod body;
pub mod glow;
pub mod ground;
pub mod info;
pub mod model;
pub mod price;

use std::path::Path;

use crate::warps::bux_convert;
use crate::{Error, Result, fs};

/// `MAX_ITEM_INDEX` (_define.h:342): items per group.
pub const MAX_ITEM_INDEX: usize = 512;
/// `MAX_ITEM_TYPE` (_define.h:339): groups.
pub const MAX_ITEM_TYPE: usize = 16;
/// `MAX_ITEM` (_define.h:344).
pub const MAX_ITEM: usize = MAX_ITEM_TYPE * MAX_ITEM_INDEX;
/// `sizeof(ITEM_ATTRIBUTE)` of the S6 `item_eng.bmd`.
pub const RECORD_LEN: usize = 84;

/// Groups (`ITEM_SWORD`... from _define.h; the type is `group · 512 + index`).
pub mod group {
    pub const SWORD: u8 = 0;
    pub const AXE: u8 = 1;
    pub const MACE: u8 = 2;
    pub const SPEAR: u8 = 3;
    pub const BOW: u8 = 4;
    pub const STAFF: u8 = 5;
    pub const SHIELD: u8 = 6;
    pub const HELM: u8 = 7;
    pub const ARMOR: u8 = 8;
    pub const PANTS: u8 = 9;
    pub const GLOVES: u8 = 10;
    pub const BOOTS: u8 = 11;
    pub const WING: u8 = 12;
    pub const HELPER: u8 = 13;
    pub const POTION: u8 = 14;
    pub const ETC: u8 = 15;
}

/// Equipment slots (`EQUIPMENT_*`, _define.h:197-208).
pub mod slot {
    pub const WEAPON_RIGHT: u8 = 0;
    pub const WEAPON_LEFT: u8 = 1;
    pub const HELM: u8 = 2;
    pub const ARMOR: u8 = 3;
    pub const PANTS: u8 = 4;
    pub const GLOVES: u8 = 5;
    pub const BOOTS: u8 = 6;
    pub const WING: u8 = 7;
    pub const HELPER: u8 = 8;
    pub const AMULET: u8 = 9;
    pub const RING_RIGHT: u8 = 10;
    pub const RING_LEFT: u8 = 11;
    /// `MAX_EQUIPMENT`.
    pub const COUNT: usize = 12;
}

/// Original type (`ITEM_SWORD + index`...): `group · MAX_ITEM_INDEX + index`.
pub const fn kind(group: u8, index: u16) -> usize {
    group as usize * MAX_ITEM_INDEX + index as usize
}

/// `ITEM_ATTRIBUTE` (_struct.h:150-182).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemAttribute {
    pub name: String,
    pub two_hand: bool,
    /// Item level (for drops and the requirement formulas).
    pub level: u16,
    /// `m_byItemSlot`: equipment slot it goes into (`None` = 255, not equippable).
    pub slot: Option<u8>,
    /// `m_bySkillIndex`: the skill granted by the skill option.
    pub skill: u16,
    /// Inventory cells it occupies.
    pub width: u8,
    pub height: u8,
    pub damage_min: u8,
    pub damage_max: u8,
    pub blocking: u8,
    pub defense: u8,
    pub magic_defense: u8,
    pub weapon_speed: u8,
    pub walk_speed: u8,
    pub durability: u8,
    pub magic_durability: u8,
    pub magic_power: u8,
    pub require_strength: u16,
    pub require_dexterity: u16,
    pub require_energy: u16,
    pub require_vitality: u16,
    pub require_charisma: u16,
    pub require_level: u16,
    pub value: u16,
    pub zen: i32,
    pub attack_type: u8,
    /// `RequireClass[CLASS_*]`: 0 = cannot; 1, 2 or 3 = from the 1st, 2nd or 3rd class.
    pub require_class: [u8; 7],
    pub resistance: [u8; 8],
}

/// The `MAX_ITEM` types (`ItemAttribute[]`); nonexistent ones are `None`.
#[derive(Debug, Clone)]
pub struct ItemTable {
    items: Vec<Option<ItemAttribute>>,
}

impl Default for ItemTable {
    fn default() -> Self {
        Self { items: vec![None; MAX_ITEM] }
    }
}

impl ItemTable {
    pub fn get(&self, group: u8, index: u16) -> Option<&ItemAttribute> {
        self.by_kind(kind(group, index))
    }

    pub fn by_kind(&self, kind: usize) -> Option<&ItemAttribute> {
        self.items.get(kind).and_then(|i| i.as_ref())
    }

    /// How many types have a name.
    pub fn count(&self) -> usize {
        self.items.iter().filter(|i| i.is_some()).count()
    }
}

/// Reads `item_eng.bmd`. Records without a name (`Name[0] == 0`, as `PrintItem` checks) are left empty.
pub fn read_item_bmd(buf: &[u8]) -> Result<ItemTable> {
    if buf.len() < MAX_ITEM * RECORD_LEN {
        return Err(Error::Format(format!("item.bmd: {} bytes, se esperaban {}", buf.len(), MAX_ITEM * RECORD_LEN + 4)));
    }
    let mut table = ItemTable::default();
    for (i, chunk) in buf.chunks_exact(RECORD_LEN).take(MAX_ITEM).enumerate() {
        let mut r = [0u8; RECORD_LEN];
        r.copy_from_slice(chunk);
        bux_convert(&mut r);
        if r[0] == 0 {
            continue;
        }
        let w = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
        let end = r[..30].iter().position(|&b| b == 0).unwrap_or(30);
        let mut require_class = [0u8; 7];
        require_class.copy_from_slice(&r[69..76]);
        let mut resistance = [0u8; 8];
        resistance.copy_from_slice(&r[76..84]);
        table.items[i] = Some(ItemAttribute {
            // Windows-1252 (English names are ASCII).
            name: r[..end].iter().map(|&b| b as char).collect(),
            two_hand: w(30) != 0,
            level: w(32),
            slot: (w(34) < slot::COUNT as u16).then_some(w(34) as u8),
            skill: w(36),
            width: r[38],
            height: r[39],
            damage_min: r[40],
            damage_max: r[41],
            blocking: r[42],
            defense: r[43],
            magic_defense: r[44],
            weapon_speed: r[45],
            walk_speed: r[46],
            durability: r[47],
            magic_durability: r[48],
            magic_power: r[49],
            require_strength: w(50),
            require_dexterity: w(52),
            require_energy: w(54),
            require_vitality: w(56),
            require_charisma: w(58),
            require_level: w(60),
            value: w(62),
            zen: i32::from_le_bytes([r[64], r[65], r[66], r[67]]),
            attack_type: r[68],
            require_class,
            resistance,
        });
    }
    Ok(table)
}

/// Reads MuMain's `Items/GroupNN_*.json` (one per group: `{"group": N, "items": [...]}`).
pub fn read_items_json(files: &[String]) -> Result<ItemTable> {
    let mut table = ItemTable::default();
    for text in files {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| Error::Format(format!("items json: {e}")))?;
        let group = v["group"].as_u64().ok_or_else(|| Error::Format("items json sin grupo".into()))? as usize;
        for it in v["items"].as_array().into_iter().flatten() {
            let Some(number) = it["number"].as_u64().map(|n| n as usize).filter(|&n| n < MAX_ITEM_INDEX) else { continue };
            let n = |k: &str| it[k].as_u64().unwrap_or(0);
            let req = |k: &str| it["requirements"][k].as_u64().unwrap_or(0) as u16;
            let class = |k: &str| it["classRequirements"][k].as_u64().unwrap_or(0) as u8;
            let name = it["name"]["en"].as_str().or_else(|| it["name"].as_str()).unwrap_or("").to_string();
            // `m_byItemSlot`: MuMain's names for the `EQUIPMENT_*` values.
            let slot = match it["slot"].as_str().unwrap_or("") {
                "mainHand" => Some(slot::WEAPON_RIGHT),
                "offHand" => Some(slot::WEAPON_LEFT),
                "helm" => Some(slot::HELM),
                "armor" => Some(slot::ARMOR),
                "pants" => Some(slot::PANTS),
                "gloves" => Some(slot::GLOVES),
                "boots" => Some(slot::BOOTS),
                "wings" => Some(slot::WING),
                "pet" => Some(slot::HELPER),
                "pendant" => Some(slot::AMULET),
                "ring" => Some(slot::RING_RIGHT),
                _ => None,
            };
            table.items[group * MAX_ITEM_INDEX + number] = Some(ItemAttribute {
                name,
                two_hand: it["twoHanded"].as_bool().unwrap_or(false),
                level: n("level") as u16,
                slot,
                skill: n("skill") as u16,
                width: n("width").max(1) as u8,
                height: n("height").max(1) as u8,
                damage_min: n("damageMin") as u8,
                damage_max: n("damageMax") as u8,
                blocking: n("blockRate") as u8,
                defense: n("defense") as u8,
                magic_defense: 0,
                weapon_speed: n("attackSpeed") as u8,
                walk_speed: n("walkSpeed") as u8,
                durability: n("durability") as u8,
                magic_durability: n("magicDurability") as u8,
                magic_power: n("magicPower") as u8,
                require_strength: req("strength"),
                require_dexterity: req("dexterity"),
                require_energy: req("energy"),
                require_vitality: req("vitality"),
                require_charisma: req("leadership"),
                require_level: req("level"),
                // `attribute.Value = definition.sellValue; attribute.iZen = definition.buyPrice`
                // (MuMain Data/GameData/ItemData/ItemAttributeConversion.cpp:68-69).
                value: n("sellValue") as u16,
                zen: n("buyPrice") as i32,
                attack_type: n("attackType") as u8,
                require_class: [
                    class("darkWizard"),
                    class("darkKnight"),
                    class("fairyElf"),
                    class("magicGladiator"),
                    class("darkLord"),
                    class("summoner"),
                    class("rageFighter"),
                ],
                resistance: [0; 8],
            });
        }
    }
    Ok(table)
}

/// The table from the `Data` folder: `Local/Eng/item_eng.bmd` (the original's) or, if missing,
/// MuMain's `Items/GroupNN_*.json`.
pub fn load(data: &Path) -> Result<ItemTable> {
    match fs::read(&data.join("Local").join("Eng"), "item_eng.bmd") {
        Ok(b) => read_item_bmd(&b),
        Err(Error::NotFound(_)) => {
            let dir = fs::find(data, "Items").ok_or_else(|| Error::NotFound(data.join("Items")))?;
            let mut files: Vec<_> = std::fs::read_dir(&dir)
                .map_err(|e| Error::Io(dir.clone(), e))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    n.starts_with("group") && n.ends_with(".json")
                })
                .collect();
            files.sort();
            let texts = files
                .iter()
                .map(|p| std::fs::read_to_string(p).map_err(|e| Error::Io(p.clone(), e)))
                .collect::<Result<Vec<_>>>()?;
            read_items_json(&texts)
        }
        Err(e) => Err(e),
    }
}

/// Name and mana of each skill (`SkillAttribute[i].Mana`), from the skill table in either layout
/// (see [`crate::skills`]).
pub fn read_skill_mana(buf: &[u8]) -> Result<Vec<(String, u16)>> {
    Ok(crate::skills::read_skills(buf)?.0.into_iter().map(|s| (s.name, s.mana)).collect())
}

/// `Local/Eng/skill_eng.bmd` (Main 5.2's or MuMain's layout).
pub fn load_skill_mana(data: &Path) -> Result<Vec<(String, u16)>> {
    Ok(crate::skills::load_skills(data)?.0.into_iter().map(|s| (s.name, s.mana)).collect())
}

// --- One item --------------------------------------------------------------------------------------

/// `EXT_A_SET_OPTION` and `EXT_B_SET_OPTION` (`ExtOption % 4`): set (ancient) item.
pub const EXT_A_SET_OPTION: u8 = 1;
pub const EXT_B_SET_OPTION: u8 = 2;

/// A concrete item (`ITEM`): type, level and options, as they arrive in the S6's 12 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Item {
    pub group: u8,
    pub index: u16,
    /// +0 to +15 (`(Level >> 3) & 15`).
    pub level: u8,
    pub durability: u8,
    /// Skill option (`Level & 128`).
    pub skill: bool,
    /// Luck (`Level & 4`).
    pub luck: bool,
    /// Additional option, 0 to 7 (`(Level & 3) + (Option1 & 64) / 64 · 4`).
    pub option: u8,
    /// Excellent options (`Option1 & 63`).
    pub excellent: u8,
    /// `ExtOption`: ancient set (`% 4` = 1 or 2) and its bonus level.
    pub ancient: u8,
}

impl Item {
    pub fn new(group: u8, index: u16) -> Self {
        Self { group, index, ..Default::default() }
    }

    /// Original type (`ITEM_*`).
    pub fn kind(&self) -> usize {
        kind(self.group, self.index)
    }

    /// `ip->Level` exactly as it goes in the packet (`Attribute1`).
    pub fn level_byte(&self) -> u8 {
        (self.skill as u8) << 7 | (self.level & 15) << 3 | (self.luck as u8) << 2 | (self.option & 3)
    }

    /// `ip->Option1` (`Attribute2`): excellent options, the option's high bit and bit 8 of the index.
    pub fn option1(&self) -> u8 {
        (self.excellent & 63) | ((self.option >> 2) & 1) << 6 | (((self.index >> 8) & 1) as u8) << 7
    }

    /// Set item (`(ExtOption % 4) == EXT_A/B_SET_OPTION`).
    pub fn is_ancient(&self) -> bool {
        matches!(self.ancient % 4, EXT_A_SET_OPTION | EXT_B_SET_OPTION)
    }

    /// An item's bytes in S6 packets (`ConvertItemType`, ZzzInfomation.cpp:379-384:
    /// `Item[0] + (Item[3] & 128) · 2 + (Item[5] & 240) · 32`; the rest, `ItemConvert(ip, Item[1],
    /// Item[3], Item[4])` and `Item[2]` = durability). Needs at least 6.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 6 || (b[0] == 0xFF && b[5] >> 4 == 0xF) {
            return None;
        }
        let kind = b[0] as usize + (b[3] as usize & 128) * 2 + (b[5] as usize & 240) * 32;
        Some(Self {
            group: (kind / MAX_ITEM_INDEX) as u8,
            index: (kind % MAX_ITEM_INDEX) as u16,
            level: (b[1] >> 3) & 15,
            durability: b[2],
            skill: b[1] & 128 != 0,
            luck: b[1] & 4 != 0,
            option: (b[1] & 3) + ((b[3] & 64) >> 6) * 4,
            excellent: b[3] & 63,
            ancient: b[4],
        })
    }

    /// The S6's 12 bytes (OpenMU's `ItemSerializer`); harmony and sockets zeroed.
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0] = (self.index & 0xFF) as u8;
        b[1] = self.level_byte();
        b[2] = self.durability;
        b[3] = self.option1();
        b[4] = self.ancient;
        b[5] = (self.group & 15) << 4;
        b
    }
}

/// Options computed by `ItemConvert` (`ip->Special[]`, the `AT_*` from SkillManager.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// The item's skill (`m_bySkillIndex`).
    Skill(u16),
    Luck,
    ImproveDamage,
    ImproveMagic,
    ImproveCurse,
    ImproveBlocking,
    ImproveDefense,
    LifeRegeneration,
    ImproveMaxMana,
    ImproveMaxAg,
    // Excellent options for armor and shields.
    ImproveLife,
    ImproveMana,
    DecreaseDamage,
    ReflectionDamage,
    ImproveBlockingPercent,
    ImproveGainGold,
    // Excellent options for weapons.
    ExcellentDamage,
    ImproveDamageLevel,
    ImproveDamagePercent,
    ImproveMagicLevel,
    ImproveMagicPercent,
    ImproveAttackSpeed,
    ImproveGainLife,
    ImproveGainMana,
    // 2nd-class wings.
    ImproveHpMax,
    ImproveMpMax,
    OnePercentDamage,
    ImproveAgMax,
}

/// What `ItemConvert` leaves in the `ITEM` (ZzzInfomation.cpp:566-1573).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemStats {
    pub damage_min: u32,
    pub damage_max: u32,
    pub magic_power: u32,
    pub blocking: u32,
    pub defense: u32,
    pub magic_defense: u32,
    pub require_level: u32,
    pub require_strength: u32,
    pub require_dexterity: u32,
    pub require_energy: u32,
    pub require_vitality: u32,
    pub require_charisma: u32,
    /// `Special[i]` and `SpecialValue[i]`, in the original's order.
    pub specials: Vec<(Special, u32)>,
}

impl ItemStats {
    /// `SpecialNum` (counts for the name color).
    pub fn special_count(&self) -> usize {
        self.specials.len()
    }
}

/// `(min(9, level) · 3)` plus the +10 to +15 extra (`switch (Level - 9)`, 4 to 9).
fn level_bonus(level: u32) -> u32 {
    level.min(9) * 3 + if level >= 10 { level - 9 + 3 } else { 0 }
}

/// `ItemConvert` without harmony, sockets or pets. `hero_level` is the one for `+level/20`.
pub fn convert(attr: &ItemAttribute, item: &Item, hero_level: u32) -> ItemStats {
    use group::*;
    let (g, i) = (item.group, item.index);
    let t = item.kind();
    let k = |gr: u8, ix: u16| kind(gr, ix);
    let level = item.level.min(15) as u32;
    let mut excel = item.excellent as u32 & 63;
    let excel_wing = excel;
    // 2nd-class wings, the Dark Lord cape and the Blood Castle weapons do not increase from excellent.
    let wing2 = (k(WING, 3)..=k(WING, 6)).contains(&t)
        || (k(WING, 36)..=k(WING, 40)).contains(&t)
        || (k(WING, 42)..=k(WING, 43)).contains(&t);
    if wing2
        || t == k(SWORD, 19)
        || t == k(BOW, 18)
        || t == k(STAFF, 10)
        || t == k(MACE, 13)
        || t == k(HELPER, 30)
        || (k(WING, 130)..=k(WING, 134)).contains(&t)
    {
        excel = 0;
    }
    let ext_option = item.is_ancient();
    if ext_option {
        excel = 1;
    }
    let p_level = attr.level as u32;
    let set_drop_level = p_level + 30;
    let excel_add = match (g, i) {
        (MACE, 6) => 15,
        (BOW, 6) => 30,
        (STAFF, 7) => 25,
        _ => 0,
    };
    let mut s = ItemStats {
        damage_min: attr.damage_min as u32,
        damage_max: attr.damage_max as u32,
        magic_power: attr.magic_power as u32,
        blocking: attr.blocking as u32,
        defense: attr.defense as u32,
        magic_defense: attr.magic_defense as u32,
        ..Default::default()
    };
    let excel_damage = |base: u32| if excel_add != 0 { excel_add } else { base * 25 / p_level + 5 };
    if attr.damage_min > 0 {
        if excel > 0 && p_level > 0 {
            s.damage_min += excel_damage(attr.damage_min as u32);
        }
        if ext_option {
            s.damage_min += 5 + set_drop_level / 40;
        }
        s.damage_min += level_bonus(level);
    }
    if attr.damage_max > 0 {
        if excel > 0 && p_level > 0 {
            // The original also adds `DamageMin · 25 / Level + 5` here.
            s.damage_max += excel_damage(attr.damage_min as u32);
        }
        if ext_option {
            s.damage_max += 5 + set_drop_level / 40;
        }
        s.damage_max += level_bonus(level);
    }
    if attr.magic_power > 0 {
        if excel > 0 && p_level > 0 {
            s.magic_power += excel_damage(attr.magic_power as u32);
        }
        if ext_option {
            s.magic_power += 2 + set_drop_level / 60;
        }
        s.magic_power += level_bonus(level);
        s.magic_power /= 2;
        // `IsCepterItem`: Dark Lord scepters (maces 8 to 15 and 17) do not add `Level · 2`.
        let cepter = g == MACE && ((8..=15).contains(&i) || i == 17);
        if !cepter {
            s.magic_power += level * 2;
        }
    }
    if attr.blocking > 0 {
        if excel > 0 && p_level > 0 {
            s.blocking += attr.blocking as u32 * 25 / p_level + 5;
        }
        s.blocking += level_bonus(level);
    }
    if attr.defense > 0 {
        if g == SHIELD {
            s.defense += level;
            if ext_option {
                s.defense = s.defense + s.defense * 20 / set_drop_level + 2;
            }
        } else {
            if excel > 0 && p_level > 0 {
                s.defense += attr.defense as u32 * 12 / p_level + 4 + p_level / 5;
            }
            if ext_option {
                s.defense = s.defense + s.defense * 3 / set_drop_level + 2 + set_drop_level / 30;
            }
            let wing3 = (k(WING, 36)..=k(WING, 40)).contains(&t) || t == k(WING, 43);
            if (k(WING, 3)..=k(WING, 6)).contains(&t) || t == k(WING, 42) || t == k(HELPER, 30) {
                s.defense += level.min(9) * 2;
            } else if wing3 {
                s.defense += level.min(9) * 4;
            } else {
                s.defense += level.min(9) * 3;
            }
            if level >= 10 {
                s.defense += level - 9 + if wing3 { 4 } else { 3 };
            }
        }
    }
    if attr.magic_defense > 0 {
        s.magic_defense += level.min(9) * 3;
        // The original's `switch` has no `break` and no `case 5`: it falls through downward.
        let extra = match level {
            15 => 9 + 7 + 6 + 5 + 4,
            13 => 7 + 6 + 5 + 4,
            12 => 6 + 5 + 4,
            11 => 5 + 4,
            10 => 4,
            _ => 0,
        };
        s.magic_defense += extra;
    }

    let item_level = if excel > 0 {
        p_level + 25
    } else if ext_option {
        p_level + 30
    } else {
        p_level
    };
    let add_value = if (k(WING, 3)..=k(WING, 6)).contains(&t) || t == k(WING, 42) { 5 } else { 4 };
    let req_level = attr.require_level as u32;
    s.require_level = if req_level > 0
        && ((k(SWORD, 0)..k(WING, 0)).contains(&t)
            || t == k(HELPER, 37)
            || (k(WING, 7)..=k(WING, 40)).contains(&t)
            || (k(WING, 43)..k(HELPER, 0)).contains(&t))
    {
        req_level
    } else if req_level > 0 && ((k(WING, 0)..=k(WING, 7)).contains(&t) || (k(WING, 41)..=k(WING, 42)).contains(&t) || t >= k(HELPER, 0)) {
        req_level + level * add_value
    } else {
        0
    };
    let req = |base: u16, div: u32, mul: u32| {
        if base == 0 { 0 } else { 20 + base as u32 * (item_level + level * 3) * mul / div }
    };
    s.require_strength = req(attr.require_strength, 100, 3);
    s.require_dexterity = req(attr.require_dexterity, 100, 3);
    s.require_vitality = req(attr.require_vitality, 100, 3);
    s.require_energy = if attr.require_energy == 0 {
        0
    } else if (k(STAFF, 21)..=k(STAFF, 29)).contains(&t) {
        20 + attr.require_energy as u32 * (item_level + level) * 3 / 100
    } else if req_level > 0 && g == ETC {
        20 + attr.require_energy as u32 * req_level * 4 / 100
    } else {
        req(attr.require_energy, 100, 4)
    };
    // `RequireCharisma`: the formula is overwritten right after with the raw value (or the raven's).
    s.require_charisma = if attr.require_charisma == 0 {
        0
    } else if t == k(HELPER, 5) {
        185 + attr.require_charisma as u32 * 15
    } else {
        attr.require_charisma as u32
    };
    if t == k(HELPER, 10) {
        s.require_level = if level <= 2 { 20 } else { 50 };
    }
    // The Kundun sets, the 3rd Blood Castle weapons... carry no excellent bonus for the level.
    if (k(HELM, 29)..=k(HELM, 33)).contains(&t)
        || (k(ARMOR, 29)..=k(ARMOR, 33)).contains(&t)
        || (k(PANTS, 29)..=k(PANTS, 33)).contains(&t)
        || (k(GLOVES, 29)..=k(GLOVES, 33)).contains(&t)
        || (k(BOOTS, 29)..=k(BOOTS, 33)).contains(&t)
        || [k(SWORD, 22), k(SWORD, 23), k(STAFF, 12), k(BOW, 21), k(MACE, 14), k(STAFF, 19)].contains(&t)
        || [k(HELM, 43), k(ARMOR, 43), k(PANTS, 43), k(GLOVES, 43), k(BOOTS, 43)].contains(&t)
    {
        excel = 0;
    }
    if excel > 0 && s.require_level > 0 && t != k(HELPER, 37) {
        s.require_level += 20;
    }

    let sp = &mut s.specials;
    // 2nd-class wings: their excellent options come first.
    if (k(WING, 3)..=k(WING, 6)).contains(&t) || t == k(WING, 42) {
        let bits = [
            (Special::ImproveHpMax, 50 + level * 5),
            (Special::ImproveMpMax, 50 + level * 5),
            (Special::OnePercentDamage, 3),
            (Special::ImproveAgMax, 50),
            (Special::ImproveAttackSpeed, 5),
        ];
        for (b, (o, v)) in bits.into_iter().enumerate() {
            if (excel_wing >> b) & 1 != 0 {
                sp.push((o, v));
            }
        }
    }
    if item.skill && attr.skill != 0 {
        sp.push((Special::Skill(attr.skill), 0));
    }
    if item.luck {
        let weapon_or_armor = (k(SWORD, 0)..k(BOOTS, 0) + MAX_ITEM_INDEX).contains(&t) && t != k(BOW, 7) && t != k(BOW, 15);
        let wings = (k(WING, 0)..=k(WING, 6)).contains(&t)
            || (k(WING, 41)..=k(WING, 42)).contains(&t)
            || t == k(HELPER, 30)
            || (k(WING, 36)..=k(WING, 40)).contains(&t)
            || t == k(WING, 43);
        if weapon_or_armor || wings {
            sp.push((Special::Luck, 0));
        }
    }
    let option3 = item.option as u32 & 7;
    if option3 > 0 {
        if g <= BOW && t != k(BOW, 7) && t != k(BOW, 15) {
            sp.push((Special::ImproveDamage, option3 * 4));
            s.require_strength += option3 * 5;
        }
        if g == STAFF {
            let o = if (21..=29).contains(&i) { Special::ImproveCurse } else { Special::ImproveMagic };
            sp.push((o, option3 * 4));
            s.require_strength += option3 * 5;
        }
        if g == SHIELD {
            sp.push((Special::ImproveBlocking, option3 * 5));
            s.require_strength += option3 * 5;
        }
        if (HELM..=BOOTS).contains(&g) {
            sp.push((Special::ImproveDefense, option3 * 4));
            s.require_strength += option3 * 5;
        }
        if g == HELPER && i >= 8 && i != 30 {
            let o = match i {
                24 => Special::ImproveMaxMana,
                28 => Special::ImproveMaxAg,
                _ => Special::LifeRegeneration,
            };
            sp.push((o, option3));
        }
        if g == WING {
            let bit5 = (excel_wing >> 5) & 1 != 0;
            let o = match i {
                0 => Some((Special::LifeRegeneration, option3)),
                1 | 41 => Some((Special::ImproveMagic, option3 * 4)),
                2 => Some((Special::ImproveDamage, option3 * 4)),
                3 if bit5 => Some((Special::LifeRegeneration, option3)),
                3 => Some((Special::ImproveDamage, option3 * 4)),
                4 if bit5 => Some((Special::ImproveMagic, option3 * 4)),
                4 => Some((Special::LifeRegeneration, option3)),
                5 if bit5 => Some((Special::ImproveDamage, option3 * 4)),
                5 => Some((Special::LifeRegeneration, option3)),
                6 if bit5 => Some((Special::ImproveDamage, option3 * 4)),
                6 => Some((Special::ImproveMagic, option3 * 4)),
                _ => None,
            };
            sp.extend(o);
        }
    }
    let bit = |n: u32| (item.excellent as u32 >> n) & 1 != 0;
    // Defensive excellent options: shields and armor, the 4-option rings and those of 8-9.
    if (SHIELD..=BOOTS).contains(&g) || (g == HELPER && ((8..=9).contains(&i) || (21..=24).contains(&i))) {
        let defs = [
            (5, Special::ImproveLife),
            (4, Special::ImproveMana),
            (3, Special::DecreaseDamage),
            (2, Special::ReflectionDamage),
            (1, Special::ImproveBlockingPercent),
            (0, Special::ImproveGainGold),
        ];
        for (b, o) in defs {
            if bit(b) {
                sp.push((o, 0));
            }
        }
    }
    // Offensive excellent options: weapons and necklaces.
    if g <= STAFF || (g == HELPER && ((12..=13).contains(&i) || (25..=28).contains(&i))) {
        if bit(5) {
            sp.push((Special::ExcellentDamage, 0));
        }
        let magic = g == STAFF || (g == HELPER && (i == 12 || i == 25 || i == 27));
        if bit(4) {
            sp.push((if magic { Special::ImproveMagicLevel } else { Special::ImproveDamageLevel }, hero_level / 20));
        }
        if bit(3) {
            sp.push((if magic { Special::ImproveMagicPercent } else { Special::ImproveDamagePercent }, 2));
        }
        if bit(2) {
            sp.push((Special::ImproveAttackSpeed, 7));
        }
        if bit(1) {
            sp.push((Special::ImproveGainLife, 0));
        }
        if bit(0) {
            sp.push((Special::ImproveGainMana, 0));
        }
    }
    s
}

/// `calcMaxDurability` (ZzzInventory.cpp:1497-1563).
pub fn max_durability(attr: &ItemAttribute, item: &Item) -> u32 {
    use group::*;
    let t = item.kind();
    let k = |g: u8, i: u16| kind(g, i);
    let mut d = if item.group == STAFF { attr.magic_durability as u32 } else { attr.durability as u32 };
    for i in 0..item.level.min(15) as u32 {
        if t >= k(HELPER, 51) {
            break;
        }
        d = match i {
            14.. => (d + 8).min(255),
            13 => (d + 7).min(255),
            12 => d + 6,
            11 => d + 5,
            10 => d + 4,
            9 => d + 3,
            4..=8 => d + 2,
            _ => d + 1,
        };
    }
    if t == k(HELPER, 4) || t == k(HELPER, 5) {
        d = 255;
    }
    let excellent_ok = (t < k(WING, 3) || t > k(WING, 6))
        && t != k(SWORD, 19)
        && t != k(BOW, 18)
        && t != k(STAFF, 10)
        && t != k(HELPER, 30)
        && (t < k(WING, 36) || t > k(WING, 40))
        && (t < k(WING, 42) || t > k(WING, 43))
        && t != k(MACE, 13);
    if item.is_ancient() {
        d += 20;
    } else if item.excellent & 63 > 0 && excellent_ok {
        d += 15;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn main52_data() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/Main5.2/Source Main 5.2/bin/Data")
    }

    fn mumain_data() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data")
    }

    /// Some known items from the table, whatever the format.
    fn check_known(t: &ItemTable) {
        let kris = t.get(group::SWORD, 0).unwrap();
        assert_eq!(kris.name, "Kris");
        assert_eq!((kris.width, kris.height, kris.damage_min, kris.damage_max, kris.weapon_speed), (1, 2, 6, 11, 50));
        assert_eq!((kris.require_strength, kris.require_dexterity, kris.slot), (40, 40, Some(slot::WEAPON_RIGHT)));
        let short = t.get(group::SWORD, 1).unwrap();
        assert_eq!((short.name.as_str(), short.width, short.height), ("Short Sword", 1, 3));
        let helm = t.get(group::HELM, 0).unwrap();
        assert_eq!((helm.name.as_str(), helm.width, helm.height, helm.defense, helm.slot), ("Bronze Helm", 2, 2, 9, Some(slot::HELM)));
        assert_eq!(helm.require_class[..2], [0, 1]);
        let dragon = t.get(group::ARMOR, 1).unwrap();
        assert_eq!((dragon.name.as_str(), dragon.width, dragon.height, dragon.defense), ("Dragon Armor", 2, 3, 37));
        let elf_wings = t.get(group::WING, 0).unwrap();
        assert_eq!((elf_wings.width, elf_wings.height, elf_wings.require_level, elf_wings.slot), (3, 2, 180, Some(slot::WING)));
        let bless = t.get(group::POTION, 13).unwrap();
        assert_eq!((bless.name.as_str(), bless.width, bless.height, bless.slot), ("Jewel of Bless", 1, 1, None));
        let ring = t.get(group::HELPER, 8).unwrap();
        assert_eq!((ring.name.as_str(), ring.slot), ("Ring of Ice", Some(slot::RING_RIGHT)));
        let katana = t.get(group::SWORD, 3).unwrap();
        assert_eq!((katana.name.as_str(), katana.skill), ("Katana", 20));
        assert!(t.count() > 500, "{} objetos", t.count());
    }

    /// The Main 5.2 `item_eng.bmd` (if present in the references).
    #[test]
    fn item_bmd_del_main52() {
        let Ok(buf) = std::fs::read(main52_data().join("Local/Eng/item_eng.bmd")) else {
            eprintln!("sin item_eng.bmd: no se prueba");
            return;
        };
        assert_eq!(buf.len(), MAX_ITEM * RECORD_LEN + 4);
        let t = read_item_bmd(&buf).unwrap();
        check_known(&t);
        assert!(read_item_bmd(&buf[..1000]).is_err());
    }

    /// MuMain's `Items/GroupNN_*.json` (the ones the `dist` client reads).
    #[test]
    fn items_json_de_mumain() {
        let Ok(t) = load(&mumain_data()) else {
            eprintln!("sin Items/*.json: no se prueba");
            return;
        };
        check_known(&t);
    }

    /// Both tables agree on what the client reads.
    #[test]
    fn bmd_y_json_coinciden() {
        let (Ok(buf), Ok(json)) = (std::fs::read(main52_data().join("Local/Eng/item_eng.bmd")), load(&mumain_data())) else {
            return;
        };
        let bmd = read_item_bmd(&buf).unwrap();
        // (The wings of heaven, 12,1, are 5×3 in MuMain and 5×2 in S6: not compared.)
        for (g, i) in [(0, 0), (0, 5), (1, 0), (4, 8), (5, 0), (6, 4), (7, 1), (8, 5), (9, 10), (11, 3), (12, 0), (13, 12)] {
            let (a, b) = (bmd.get(g, i).unwrap(), json.get(g, i).unwrap());
            assert_eq!((a.width, a.height, a.slot, a.defense, a.damage_min), (b.width, b.height, b.slot, b.defense, b.damage_min), "{g},{i}");
            assert_eq!((a.require_strength, a.require_class[..6].to_vec()), (b.require_strength, b.require_class[..6].to_vec()), "{g},{i}");
        }
    }

    #[test]
    fn bytes_del_paquete_ida_y_vuelta() {
        let item = Item { group: 13, index: 300, level: 13, durability: 77, skill: true, luck: true, option: 5, excellent: 0b101001, ancient: 5 };
        let b = item.to_bytes();
        // `ConvertItemType`: 300 = 44 + 256 (bit 7 of byte 3), group in byte 5.
        assert_eq!((b[0], b[3] & 0x80, b[5]), (44, 0x80, 13 << 4));
        // Level 13 << 3 | skill | luck | option & 3.
        assert_eq!(b[1], 0x80 | 13 << 3 | 4 | 1);
        assert_eq!(b[3] & 0x40, 0x40);
        assert_eq!(Item::from_bytes(&b), Some(item));
        // Empty slot (0xFF ... group 15 with bit 8): nothing.
        assert_eq!(Item::from_bytes(&[0xFF, 0, 0, 0xFF, 0, 0xFF]), None);
    }

    #[test]
    fn convert_como_el_original() {
        let attr = ItemAttribute {
            name: "Kris".into(),
            level: 6,
            damage_min: 6,
            damage_max: 11,
            require_strength: 40,
            require_dexterity: 40,
            ..Default::default()
        };
        // +0: the table values and `20 + 40 · 6 · 3 / 100` = 27.
        let s = convert(&attr, &Item::new(0, 0), 1);
        assert_eq!((s.damage_min, s.damage_max, s.require_strength), (6, 11, 27));
        // +13: 9 · 3 and the 7 from case 4 = 34 more.
        let s = convert(&attr, &Item { level: 13, ..Item::new(0, 0) }, 1);
        assert_eq!((s.damage_min, s.damage_max), (6 + 34, 11 + 34));
        // Excellent: `DamageMin · 25 / Level + 5` = 30 to both, and the item level + 25.
        let s = convert(&attr, &Item { excellent: 1, ..Item::new(0, 0) }, 1);
        assert_eq!((s.damage_min, s.damage_max), (36, 41));
        assert_eq!(s.require_strength, 20 + 40 * 31 * 3 / 100);
        assert_eq!(s.specials, vec![(Special::ImproveGainMana, 0)]);
        // Luck, option +8 (which requires 10 more strength), and it lacks the skill.
        let s = convert(&attr, &Item { luck: true, option: 2, skill: true, ..Item::new(0, 0) }, 1);
        assert_eq!(s.specials, vec![(Special::Luck, 0), (Special::ImproveDamage, 8)]);
        assert_eq!(s.require_strength, 27 + 10);
    }

    #[test]
    fn durabilidad_maxima() {
        let attr = ItemAttribute { durability: 20, ..Default::default() };
        // +4: 4 of 1; +9: 4 + 5 · 2; +15: ... + 3 + 4 + 5 + 6 + 7 + 8.
        assert_eq!(max_durability(&attr, &Item { level: 4, ..Item::new(0, 0) }), 24);
        assert_eq!(max_durability(&attr, &Item { level: 9, ..Item::new(0, 0) }), 34);
        assert_eq!(max_durability(&attr, &Item { level: 15, ..Item::new(0, 0) }), 34 + 3 + 4 + 5 + 6 + 7 + 8);
        assert_eq!(max_durability(&attr, &Item { excellent: 2, ..Item::new(0, 0) }), 35);
    }

    #[test]
    fn mana_de_las_habilidades() {
        let Ok(skills) = load_skill_mana(&main52_data()) else { return };
        // MuMain's layout is read too and agrees.
        if let Ok(m) = load_skill_mana(&mumain_data()) {
            assert_eq!(m[19], skills[19]);
        }
        assert_eq!(skills.len(), 600);
        // `AT_SKILL_SWORD1` = 19 (SkillManager.h:29).
        assert_eq!(skills[19].0, "Falling Slash");
        assert!(skills[19].1 > 0);
    }
}
