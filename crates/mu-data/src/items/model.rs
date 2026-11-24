//! Which model each item draws and how it is placed in an inventory slot.
//!
//! - [`model_file`]: the `AccessModel` calls of `OpenItems` and `OpenPlayers` (ZzzOpenData.cpp:108-457 and
//!   648-1296) for type `MODEL_ITEM + type` (`group · 512 + index`). Armor lives in
//!   `Data\Player` (and is drawn with the `player.bmd` skeleton), the rest in `Data\Item`. Whatever is not
//!   in this table (rare pets, event items...) comes from MuMain's `Items/Models/*.json`
//!   ([`read_model_json`]).
//! - [`screen_pose`]: what `RenderItem3D` (ZzzInventory.cpp:10605-10975: the slot point
//!   where the item goes) and `RenderObjectScreen` (ZzzInventory.cpp:8553-10603: offset, angle,
//!   scale and `BodyHeight` per type) do. Covers equipment groups, wings, rings and pendants,
//!   potions and jewels; event and shop items have many special cases that have not been
//!   ported and use their group's general rule.

use super::group::*;
use super::{Item, kind};

/// Model file: folder inside `Data` (`Item`, `Player`...) and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    pub dir: &'static str,
    pub name: String,
}

/// `AccessModel(type, dir, name, i)` (LoadData.cpp:21-30): `name0i.bmd` with `i` < 10,
/// `namei.bmd` otherwise and `name.bmd` with `i` = -1.
fn access(dir: &'static str, base: &str, i: i32) -> Option<ModelFile> {
    let name = match i {
        -1 => format!("{base}.bmd"),
        0..=9 => format!("{base}0{i}.bmd"),
        _ => format!("{base}{i}.bmd"),
    };
    Some(ModelFile { dir, name })
}

fn item(base: &str, i: i32) -> Option<ModelFile> {
    access("Item", base, i)
}

fn player(base: &str, i: i32) -> Option<ModelFile> {
    access("Player", base, i)
}

/// The item's model (`None` = the original loads none for that type in this table).
pub fn model_file(group: u8, index: u16) -> Option<ModelFile> {
    let i = index as i32;
    match group {
        SWORD => match i {
            0..=21 | 31 => item("Sword", i + 1),
            22 => item("HDK_Sword", -1),
            23 => item("HDK_Sword2", -1),
            24 => item("CW_Sword", -1),
            25 => item("CW_Sword2", -1),
            26..=28 => item(&format!("Sword_{}", i + 1), -1),
            _ => None,
        },
        AXE => (0..=8).contains(&i).then(|| item("Axe", i + 1)).flatten(),
        MACE => match i {
            0..=12 => item("Mace", i + 1),
            13 => item("Saint", -1),
            14 => item("HDK_Mace", -1),
            15 => item("CW_Mace", -1),
            16 | 17 => item(&format!("Mace_{}", i + 1), -1),
            18 => item("gamble_safter01", -1),
            _ => None,
        },
        SPEAR => match i {
            0..=10 => item("Spear", i + 1),
            11 => item("gamble_scyder01", -1),
            _ => None,
        },
        BOW => match i {
            0..=6 => item("Bow", i + 1),
            7 => item("Arrows", 1),
            8..=14 => item("CrossBow", i - 7),
            15 => item("Arrows", 2),
            16 => item("CrossBow", 17),
            17 => item("Bow", 18),
            18 => item("Bow", 19),
            19 => item("CrossBow", 20),
            20 => item("Bow", 20),
            21 => item("HDK_Bow", -1),
            22 => item("CW_Bow", -1),
            23 => item("Bow_24", -1),
            24 => item("gamblebow", -1),
            _ => None,
        },
        STAFF => match i {
            0..=11 | 14..=20 => item("Staff", i + 1),
            12 => item("HDK_Staff", -1),
            13 => item("CW_Staff", -1),
            21 => item("Book_of_Sahamutt", -1),
            22 => item("Book_of_Neil", -1),
            23 => item("Book_of_Rargle", -1),
            30..=32 => item(&format!("Staff_{}", i + 1), -1),
            33 => item("gamble_wand", -1),
            34 => item("gamble_stick", -1),
            _ => None,
        },
        SHIELD => match i {
            0..=16 => item("Shield", i + 1),
            17..=20 => item(&format!("Shield_{}", i + 1), -1),
            21 => item("crosssheild", -1),
            _ => None,
        },
        HELM..=BOOTS => armor_file(group, i),
        WING => match i {
            0..=6 => item("Wing", i + 1),
            7..=14 | 16..=19 => item("Gem", i - 6),
            15 => item("Jewel", 15),
            20 => item("Gem", 14),
            21..=24 | 35 | 48 => item("SkillScroll", -1),
            30 => item("Jewel", 1),
            31 => item("Jewel", 2),
            32 => item("giftbox_r", -1),
            33 => item("giftbox_g", -1),
            34 => item("giftbox_b", -1),
            36..=39 => item("Wing", i - 28),
            40 => item("DarkLordRobe02", -1),
            41..=43 => item("Wing", i + 1),
            44..=47 => item("Gem", 6),
            130 => item("DarkLordRobe", -1),
            136 => item("Jewel", 3),
            137 => item("jewel", 22),
            138 => item("suho", -1),
            139 => item("rs", -1),
            140 => item("jos", -1),
            141 => item("Jewel", 15),
            142 => item("LowRefineStone", -1),
            143 => item("HighRefineStone", -1),
            _ => None,
        },
        HELPER => match i {
            0..=3 => player("Helper", i + 1),
            4 => item("DarkHorseHorn", -1),
            5 => item("SpiritBill", -1),
            7 => item("Covenant", -1),
            8 | 9 => item("Ring", i - 7),
            11 => item("SummonBook", -1),
            12 | 13 => item("Necklace", i - 11),
            14 | 15 => item("Quest", i - 10),
            16..=19 => item("EventBloodCastle", i - 16),
            21 => item("FireRing", -1),
            22 => item("GroundRing", -1),
            23 => item("WindRing", -1),
            24 => item("ManaRing", -1),
            25 => item("IceNecklace", -1),
            26 => item("WindNecklace", -1),
            27 => item("WaterNecklace", -1),
            28 => item("AgNecklace", -1),
            29 => item("EventChaosCastle", -1),
            30 => item("DarkLordRobe", -1),
            31 => item("DarkHorseSoul", -1),
            32..=37 => item(&format!("FR_{}", i - 31), -1),
            38 => item("kanneck2", -1),
            _ => None,
        },
        POTION => match i {
            0..=6 => item("Potion", i + 1),
            7 => item("SpecialPotion", -1),
            8 => item("Antidote", 1),
            9 => item("Beer", 1),
            10 => item("Scroll", 1),
            11 => item("MagicBox", 1),
            12 => item("Event", 1),
            13 => item("Jewel", 1),
            14 => item("Jewel", 2),
            15 => item("Gold", 1),
            16 => item("Jewel", 3),
            17..=19 => item("Devil", i - 17),
            20 => item("Drink", 0),
            21 => item("ConChip", 0),
            22 => item("jewel", 22),
            23..=26 => item("Quest", i - 23),
            27 => item("godesteel", -1),
            28 | 29 => item("HELLASITEM", i - 28),
            31 => item("suho", -1),
            35..=37 => item("sdwater", i - 34),
            38..=40 => item("megawater", i - 37),
            41 => item("rs", -1),
            42 => item("jos", -1),
            43 => item("LowRefineStone", -1),
            44 => item("HighRefineStone", -1),
            _ => None,
        },
        ETC => match i {
            0..=18 => item("Book", i + 1),
            19..=27 => item("rollofpaper", -1),
            28 | 29 => item("Book", 18),
            _ => None,
        },
        _ => None,
    }
}

/// Helms, armors, pants, gloves and boots (`OpenPlayers`, ZzzOpenData.cpp:146-443).
fn armor_file(group: u8, i: i32) -> Option<ModelFile> {
    let part = match group {
        HELM => "Helm",
        ARMOR => "Armor",
        PANTS => "Pant",
        GLOVES => "Glove",
        _ => "Boot",
    };
    let male = |n: i32| player(&format!("{part}Male"), n);
    match i {
        0..=9 => male(i + 1),
        10..=14 => player(&format!("{part}Elf"), i - 9),
        // `MODEL_HELM+15` is not loaded (the Legendary Dragon set has no helm).
        15 if group == HELM => None,
        15..=28 => male(i + 1),
        // Kundun (HDK) and Raven (CW): helms 32 and 37 do not exist.
        29..=33 => {
            if group == HELM && i == 32 {
                None
            } else {
                player(&format!("HDK_{part}Male"), i - 28)
            }
        }
        34..=38 => {
            if group == HELM && i == 37 {
                None
            } else {
                player(&format!("CW_{part}Male"), i - 33)
            }
        }
        39..=53 => male(i + 1),
        // `MODEL_*+59..61` (no gloves).
        59..=61 if group != GLOVES => male(i + 1),
        _ => None,
    }
}

/// MuMain's `Data/Items/Models/GroupNN_*.json`: (index, file relative to `Data`).
pub fn read_model_json(text: &str) -> Vec<(u8, u16, String)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let group = v["group"].as_u64().unwrap_or(0) as u8;
    v["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let n = m["number"].as_u64()? as u16;
            let file = m["file"].as_str()?;
            Some((group, n, file.trim_start_matches("Data/").to_string()))
        })
        .collect()
}

/// How an item is drawn in a slot (`RenderItem3D` + `RenderObjectScreen`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPose {
    /// Where it goes inside the slot rectangle (fraction of width and height: `sx += Width · a`).
    pub anchor: [f32; 2],
    /// `Position[0..2] +=` in the item camera space (x right, y up).
    pub offset: [f32; 3],
    /// `ObjectSelect.Angle` (degrees, `AngleMatrix`).
    pub angle: [f32; 3],
    /// `o->Scale`.
    pub scale: f32,
    /// `b->BodyHeight`: how far the `player.bmd` root bone goes down (helms -160, armors -100...).
    pub body_height: f32,
    /// Posed with the `player.bmd` skeleton (`ObjectSelect.Type = MODEL_PLAYER`).
    pub player_skeleton: bool,
}

/// `RENDER_ITEMVIEW_FAR` and `RENDER_ITEMVIEW_NEAR` (_define.h:107-108).
pub const ITEMVIEW_FAR: f32 = 2000.0;
pub const ITEMVIEW_NEAR: f32 = 20.0;
/// `gluPerspective2(1.f, ...)` of the item camera (`CNewUI3DCamera::Render`,
/// NewUI3DRenderMng.cpp:106-110): one degree of vertical field.
pub const ITEMVIEW_FOV_DEG: f32 = 1.0;

/// `RenderItem3D`: `sx += Width · a[0]`, `sy += Height · a[1]`.
fn anchor(item: &Item) -> [f32; 2] {
    let (g, i, level) = (item.group, item.index as i32, item.level as i32);
    let t = item.kind();
    let k = |gr: u8, ix: u16| kind(gr, ix);
    let mut a = match g {
        SWORD => [0.8, 0.85],
        AXE | MACE if t == k(MACE, 13) => [0.6, 0.5],
        AXE | MACE => [0.8, 0.7],
        SPEAR => [0.6, 0.65],
        BOW if i == 17 => [0.5, 0.5],
        BOW if i == 19 => [0.7, 0.75],
        BOW if i == 20 => [0.5, 0.55],
        BOW if i >= 8 => [0.7, 0.7],
        STAFF => [0.6, 0.55],
        SHIELD => match i {
            15 => [0.5, 0.7],
            16 => [0.5, 0.9],
            21 => [0.55, 0.5],
            _ => [0.5, 0.6],
        },
        HELM => [0.5, 0.8],
        ARMOR => match i {
            2 | 4 | 6 => [0.5, 1.05],
            3 | 8 => [0.5, 1.1],
            15 => [0.5, 1.0],
            _ => [0.5, 0.8],
        },
        PANTS..=BOOTS => [0.5, 0.9],
        HELPER => match i {
            14 if level == 1 => [0.55, 0.85],
            14 | 15 => [0.6, 1.0],
            16 | 17 => [0.5, 0.9],
            18 => [0.5, 0.75],
            19 => match level {
                0 => [0.5, 0.5],
                1 => [0.7, 0.8],
                2 => [0.7, 0.7],
                _ => [0.0, 0.0],
            },
            20 => match level {
                0 => [0.5, 0.65],
                1..=3 => [0.5, 0.8],
                _ => [0.0, 0.0],
            },
            29 | 30 => [0.5, 0.5],
            4 => [0.5, 0.6],
            31 | 7 => [0.5, 0.9],
            11 => match level {
                0 => [0.5, 0.8],
                1 => [0.5, 0.5],
                _ => [0.0, 0.0],
            },
            // The original compares 33-37 with `MODEL_HELPER+33`... (not `ITEM_`): they never match.
            32 => [0.5, 0.5],
            _ => [0.5, 0.7],
        },
        POTION => match i {
            7 | 12 | 17..=19 | 31 | 46..=48 => [0.5, 0.5],
            11 if level == 3 || level == 13 => [0.5, 0.5],
            11 if level == 14 || level == 15 => [0.5, 0.8],
            9 if level == 1 => [0.5, 0.8],
            21 => match level {
                0 | 3 => [0.5, 0.5],
                1 | 2 => [0.4, 0.8],
                _ => [0.0, 0.0],
            },
            24 if level == 1 => [0.5, 0.8],
            22..=24 => [0.5, 0.95],
            25 | 26 => [0.5, 0.9],
            100 => [0.49, 0.28],
            _ => [0.5, 0.95],
        },
        WING => match i {
            30 | 31 => [0.55, 0.8],
            3 => [0.5, 0.45],
            4 => [0.5, 0.4],
            5 => [0.5, 0.75],
            6 => [0.5, 0.55],
            12..=14 | 16..=19 => [0.5, 0.75],
            49 | 50 => [0.5, 0.5],
            _ => [0.5, 0.6],
        },
        _ => [0.5, 0.6],
    };
    // `ITEM_SWORD+32..34` (Rage Fighter).
    if g == SWORD && (32..=34).contains(&i) {
        a = [a[0] - 0.25, a[1] - 0.25];
    }
    a
}

/// `RenderObjectScreen`: offset and angle (the `if` chain at the start and the `switch`es at the end).
fn offset_angle(item: &Item, two_hand: bool) -> ([f32; 3], [f32; 3]) {
    let (g, i, level) = (item.group, item.index as i32, item.level as i32);
    let mut o = [0.0f32; 3];
    let mut dz = 0.0f32;
    let mut off = |x: f32, y: f32| {
        o[0] += x;
        o[1] += y;
    };
    let mut angle = match (g, i) {
        (SWORD, 0) => {
            off(-0.02, 0.03);
            [180.0, 270.0, 15.0]
        }
        (BOW, 7) | (BOW, 15) => [0.0, 270.0, 15.0],
        (SPEAR, 0) => {
            off(0.0, 0.05);
            [0.0, 90.0, 20.0]
        }
        (BOW, 17) => [0.0, 90.0, 15.0],
        (HELM, 31) => {
            off(0.03, -0.06);
            [-90.0, 0.0, 0.0]
        }
        (HELM, 30) => {
            off(-0.03, 0.07);
            [-90.0, 0.0, 0.0]
        }
        (ARMOR, 30) => {
            off(0.0, 0.1);
            [-90.0, 0.0, 0.0]
        }
        (ARMOR, 29) => {
            off(0.0, 0.07);
            [-90.0, 0.0, 0.0]
        }
        (BOW, 21) => {
            off(0.0, 0.12);
            [180.0, -90.0, 15.0]
        }
        (STAFF, 12) => {
            off(0.025, -0.1);
            [180.0, 0.0, 8.0]
        }
        (STAFF, 21..=29) => [0.0, 0.0, 0.0],
        (MACE, 14) => {
            off(-0.01, 0.1);
            [180.0, 90.0, 13.0]
        }
        (ARMOR, 34) => {
            off(0.0, 0.03);
            [-90.0, 0.0, 0.0]
        }
        (HELM, 35) => {
            off(-0.02, 0.05);
            [-90.0, 0.0, 0.0]
        }
        (ARMOR, 35) => {
            off(0.0, 0.05);
            [-90.0, 0.0, 0.0]
        }
        (ARMOR, 36) | (ARMOR, 37) => {
            off(0.0, -0.05);
            [-90.0, 0.0, 0.0]
        }
        (HELM, 39..=44) => {
            off(0.0, -0.05);
            [-90.0, 25.0, 0.0]
        }
        (ARMOR, 38..=44) => {
            off(0.0, -0.08);
            [-90.0, 0.0, 0.0]
        }
        (SWORD, 24) => {
            off(-0.02, 0.03);
            [180.0, 90.0, 15.0]
        }
        (MACE, 15) => {
            off(0.0, 0.05);
            [180.0, 90.0, 13.0]
        }
        (BOW, 22) | (BOW, 23) => {
            off(-0.1, 0.08);
            [180.0, -90.0, 15.0]
        }
        (STAFF, 13) => {
            off(0.02, 0.02);
            [180.0, 90.0, 8.0]
        }
        (BOW, 20) => [180.0, -90.0, 15.0],
        (BOW, 8..) => [90.0, 180.0, 20.0],
        (SPEAR, 10) => [180.0, 270.0, 20.0],
        (SWORD..=STAFF, _) => {
            match (g, i) {
                (STAFF, 14) => off(0.0, 0.04),
                (STAFF, 17) => off(0.02, 0.03),
                (STAFF, 18) => off(0.02, 0.0),
                (STAFF, 19) => off(-0.02, -0.02),
                (STAFF, 20) => off(0.01, -0.01),
                _ => {}
            }
            if two_hand { [180.0, 270.0, 25.0] } else { [180.0, 270.0, 15.0] }
        }
        (SHIELD, _) => [270.0, 270.0, 0.0],
        (HELPER, 3) | (HELPER, 4) | (HELPER, 31) => [-90.0, -90.0, 0.0],
        (HELPER, 5) => [-90.0, -35.0, 0.0],
        (HELPER, 30) => [-90.0, 0.0, 0.0],
        (HELPER, 16) | (HELPER, 17) => [270.0, -10.0, 0.0],
        (HELPER, 18) | (HELPER, 29) => [290.0, 0.0, 0.0],
        (HELPER, 21..=24) => [270.0, 160.0, 20.0],
        (HELPER, 32..=37) => {
            match i {
                32 => off(0.01, -0.03),
                33 => off(0.0, 0.02),
                34 | 35 => off(0.01, 0.02),
                36 => off(0.01, 0.05),
                _ => off(0.01, 0.04),
            }
            [270.0, 0.0, 0.0]
        }
        (WING, 37) | (WING, 38) | (WING, 40) => {
            off(0.0, 0.05);
            [270.0, -10.0, 0.0]
        }
        (WING, 39) => {
            off(0.0, 0.08);
            [270.0, -10.0, 0.0]
        }
        (WING, 42) => {
            off(0.0, 0.05);
            [270.0, 0.0, 2.0]
        }
        (HELPER, 38) => {
            off(0.0, 0.02);
            [-48.0 - 150.0, 0.0, 0.0]
        }
        (POTION, 41) => {
            off(0.0, 0.02);
            [270.0, 90.0, 0.0]
        }
        (POTION, 42) => {
            off(0.0, 0.02);
            [270.0, -10.0, 0.0]
        }
        (POTION, 43) | (POTION, 44) => {
            dz = 0.02;
            off(-0.04, 0.02);
            [270.0, -10.0, -45.0]
        }
        (HELPER, 12..) if i != 14 && i != 15 => [270.0 + 90.0, 0.0, 0.0],
        (POTION, 12) => match level {
            0 => [180.0, 0.0, 0.0],
            1 => [270.0, 90.0, 0.0],
            2 => [90.0, 0.0, 0.0],
            _ => [270.0, -10.0, 0.0],
        },
        (POTION, 20) | (POTION, 27) => [270.0, 0.0, 0.0],
        (ETC, 19..=27) => {
            off(0.03, 0.03);
            [270.0, 0.0, 0.0]
        }
        (WING, 7) | (WING, 44..=47) => {
            off(0.005, -0.015);
            [270.0, 0.0, 0.0]
        }
        (ARMOR, 10) | (ARMOR, 11) => {
            off(0.0, -0.1);
            [270.0, 0.0, 0.0]
        }
        (PANTS, 10) | (PANTS, 11) => {
            off(0.0, -0.08);
            [270.0, 0.0, 0.0]
        }
        _ => [270.0, -10.0, 0.0],
    };
    o[2] += dz;
    // The later `switch`es (ZzzInventory.cpp ≈9660-9737).
    match (g, i) {
        (SWORD, 26) => {
            o[0] -= 0.02;
            o[1] += 0.04;
            angle = [180.0, 270.0, 10.0];
        }
        (SWORD, 27) => angle = [180.0, 270.0, 15.0],
        (SWORD, 28) => {
            o[1] += 0.02;
            angle = [180.0, 270.0, 10.0];
        }
        (MACE, 16) => {
            o[0] -= 0.02;
            angle = [180.0, 270.0, 15.0];
        }
        (MACE, 17) => {
            o[0] -= 0.02;
            o[1] += 0.04;
            angle = [180.0, 270.0, 15.0];
        }
        (STAFF, 30..=32) => angle = [180.0, 90.0, 10.0],
        (STAFF, 7) => angle = [0.0, 0.0, 205.0],
        (WING, 8..=11) => {
            o[0] += 0.005;
            o[1] -= 0.02;
        }
        (POTION, 21) => {
            o[0] += 0.005;
            o[1] -= 0.005;
        }
        (POTION, 13) | (POTION, 14) | (POTION, 22) => {
            o[0] += 0.005;
            o[1] += 0.015;
        }
        _ => {}
    }
    (o, angle)
}

/// `RenderObjectScreen`: `Scale` (ZzzInventory.cpp ≈9790-10590) and `BodyHeight`.
fn scale_height(item: &Item, offset: &mut [f32; 3]) -> (f32, f32) {
    let (g, i) = (item.group, item.index as i32);
    let body_height = match g {
        HELM => -160.0,
        ARMOR => -100.0,
        GLOVES => -70.0,
        PANTS => -50.0,
        _ => 0.0,
    };
    let scale = match (g, i) {
        (HELM, 39..=44) | (HELM, 31) => 0.007,
        (HELM, _) => 0.0039,
        (ARMOR, 34) | (ARMOR, 35) | (GLOVES, 38) => 0.0032,
        (ARMOR, _) => 0.0039,
        (GLOVES, _) => 0.0038,
        (PANTS, _) => 0.0033,
        (BOOTS, _) => 0.0032,
        (WING, 6) => 0.0015,
        (WING, 32..=34) => {
            offset[1] -= 0.05;
            0.001
        }
        (WING, 60..=65) => 0.0022,
        (WING, 70..=74) | (WING, 100..=129) => 0.0017,
        (WING, _) => 0.002,
        (POTION, 45) | (POTION, 49) => 0.003,
        (POTION, 46..=48) => 0.0025,
        (POTION, 50) => 0.001,
        (POTION, 32..=34) => {
            offset[1] += 0.05;
            0.002
        }
        (POTION, 21) | (BOW, 19) => 0.002,
        (HELPER, 4) => 0.0015,
        (HELPER, 5) => 0.005,
        (HELPER, 30) | (HELPER, 16) => 0.002,
        (HELPER, 17) | (HELPER, 18) => 0.0018,
        (SWORD, 19) | (BOW, 18) => 0.0025,
        (STAFF, 10) => 0.0019,
        (MACE, 8..=11) => 0.003,
        (MACE, 12) => 0.0025,
        (MACE, 18) => 0.0024,
        (POTION, 22..=24) => 0.0025,
        (POTION, 25) | (POTION, 26) => 0.0028,
        (HELPER, 38) => 0.0025,
        (POTION, 41) => 0.0035,
        (POTION, 42) => 0.005,
        (POTION, 43) => {
            offset[1] -= 0.005;
            0.0035
        }
        (POTION, 44) => {
            offset[1] -= 0.005;
            0.004
        }
        (POTION, 7) | (HELPER, 7) | (HELPER, 11) => 0.0025,
        (HELPER, 32) => 0.0019,
        (HELPER, 33..=35) => 0.004,
        (HELPER, 36) => 0.007,
        (HELPER, 37) => 0.005,
        (BOW, 21) => 0.0022,
        (SWORD, 24) => 0.0028,
        (BOW, 22) => 0.002,
        (BOW, 23) => 0.0032,
        (HELPER, 14) | (HELPER, 15) => 0.003,
        (POTION, 100) => 0.004,
        (POTION, _) => 0.0035,
        (SPEAR, 10) => 0.0018,
        (SPEAR, 11) => 0.0025,
        (SPEAR, _) => 0.0021,
        (STAFF, 14..=20) | (STAFF, 33) | (STAFF, 34) => 0.0028,
        (STAFF, 21..=29) => 0.004,
        (STAFF, _) => 0.0022,
        (BOW, 15) => 0.0011,
        (BOW, 7) => 0.0012,
        (ETC, 19..=27) => 0.0023,
        _ => 0.0025,
    };
    (scale, body_height)
}

/// How `item` is drawn in an inventory slot (`two_hand` = `ItemAttribute.TwoHand`).
pub fn screen_pose(item: &Item, two_hand: bool) -> ScreenPose {
    let anchor = anchor(item);
    let (mut offset, angle) = offset_angle(item, two_hand);
    let (scale, body_height) = scale_height(item, &mut offset);
    ScreenPose { anchor, offset, angle, scale, body_height, player_skeleton: (HELM..=BOOTS).contains(&item.group) }
}

/// `CreateScreenVector(sx, sy, Target, false)` + `VectorMA(MousePosition, k, Target - MousePosition)` with
/// the item camera (identity, `MousePosition` = 0): the 640×480 screen point at
/// depth `k · RENDER_ITEMVIEW_FAR` (0.1, or 0.07 when held by the mouse), in camera space
/// (x right, y up, -z forward), with a 4:3 screen. The result is `p1 · k`, with
/// `p1 = ((sx - 320) · FAR · tan(fov/2) / 240, -(sy - 240) · FAR · tan(fov/2) / 240, -FAR)`.
pub fn screen_point(sx: f32, sy: f32, picked: bool) -> [f32; 3] {
    let k = if picked { 0.07 } else { 0.1 };
    let unit = ITEMVIEW_FAR * (ITEMVIEW_FOV_DEG.to_radians() * 0.5).tan() / 240.0;
    // `(int)(sx)`, `(int)(sy)`.
    let (sx, sy) = (sx.trunc(), sy.trunc());
    [(sx - 320.0) * unit * k, -(sy - 240.0) * unit * k, -ITEMVIEW_FAR * k]
}

/// Item position (camera space) in the `(x, y, w, h)` rectangle of the 640×480 screen.
pub fn item_position(pose: &ScreenPose, rect: [f32; 4], picked: bool) -> [f32; 3] {
    let sx = rect[0] + rect[2] * pose.anchor[0];
    let sy = rect[1] + rect[3] * pose.anchor[1];
    let p = screen_point(sx, sy, picked);
    [p[0] + pose.offset[0], p[1] + pose.offset[1], p[2] + pose.offset[2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn nombres_de_los_modelos() {
        let f = |g, i| model_file(g, i).map(|m| format!("{}/{}", m.dir, m.name));
        assert_eq!(f(SWORD, 0).as_deref(), Some("Item/Sword01.bmd"));
        assert_eq!(f(SWORD, 18).as_deref(), Some("Item/Sword19.bmd"));
        assert_eq!(f(SWORD, 22).as_deref(), Some("Item/HDK_Sword.bmd"));
        assert_eq!(f(BOW, 8).as_deref(), Some("Item/CrossBow01.bmd"));
        assert_eq!(f(BOW, 7).as_deref(), Some("Item/Arrows01.bmd"));
        assert_eq!(f(HELM, 0).as_deref(), Some("Player/HelmMale01.bmd"));
        assert_eq!(f(ARMOR, 10).as_deref(), Some("Player/ArmorElf01.bmd"));
        assert_eq!(f(PANTS, 29).as_deref(), Some("Player/HDK_PantMale01.bmd"));
        assert_eq!(f(HELM, 15), None);
        assert_eq!(f(WING, 3).as_deref(), Some("Item/Wing04.bmd"));
        assert_eq!(f(WING, 36).as_deref(), Some("Item/Wing08.bmd"));
        assert_eq!(f(WING, 15).as_deref(), Some("Item/Jewel15.bmd"));
        assert_eq!(f(HELPER, 8).as_deref(), Some("Item/Ring01.bmd"));
        assert_eq!(f(HELPER, 12).as_deref(), Some("Item/Necklace01.bmd"));
        assert_eq!(f(POTION, 13).as_deref(), Some("Item/Jewel01.bmd"));
        assert_eq!(f(POTION, 17).as_deref(), Some("Item/Devil00.bmd"));
    }

    /// The table's models exist in the S6 `Data` (missing ones are counted).
    #[test]
    fn modelos_en_los_datos() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        if !data.join("Item").is_dir() {
            return;
        }
        let mut missing = Vec::new();
        let mut found = 0;
        for g in 0..16u8 {
            for i in 0..64u16 {
                if let Some(m) = model_file(g, i) {
                    if crate::fs::find(&data.join(m.dir), &m.name).is_some() {
                        found += 1;
                    } else {
                        missing.push(format!("{}/{}", m.dir, m.name));
                    }
                }
            }
        }
        eprintln!("modelos: {found} encontrados, faltan {missing:?}");
        assert!(found > 400, "{found}");
        // All the normal equipment is there.
        for (g, i) in [(0, 0), (0, 5), (1, 3), (2, 2), (3, 4), (4, 0), (4, 10), (5, 2), (6, 4), (7, 1), (8, 5), (9, 10), (10, 3), (11, 7), (12, 0), (12, 4), (13, 8), (14, 13)] {
            let m = model_file(g, i).unwrap();
            assert!(crate::fs::find(&data.join(m.dir), &m.name).is_some(), "{}/{}", m.dir, m.name);
        }
    }

    #[test]
    fn postura_en_el_inventario() {
        // Kris: (0.8, 0.85) of the slot, offset (-0.02, 0.03), (180, 270, 15) and scale 0.0025.
        let p = screen_pose(&Item::new(SWORD, 0), false);
        assert_eq!(p.anchor, [0.8, 0.85]);
        assert_eq!((p.offset, p.angle, p.scale), ([-0.02, 0.03, 0.0], [180.0, 270.0, 15.0], 0.0025));
        // Two-handed ones, 25 degrees.
        assert_eq!(screen_pose(&Item::new(SWORD, 5), true).angle, [180.0, 270.0, 25.0]);
        // Armor: character skeleton, `BodyHeight` -100, scale 0.0039.
        let a = screen_pose(&Item::new(ARMOR, 1), false);
        assert!(a.player_skeleton);
        assert_eq!((a.body_height, a.scale, a.angle, a.anchor), (-100.0, 0.0039, [270.0, -10.0, 0.0], [0.5, 0.8]));
        // Shields sideways; wings at 0.002; jewels 0.0035 with their offset.
        assert_eq!(screen_pose(&Item::new(SHIELD, 2), false).angle, [270.0, 270.0, 0.0]);
        assert_eq!(screen_pose(&Item::new(WING, 1), false).scale, 0.002);
        let bless = screen_pose(&Item::new(POTION, 13), false);
        assert_eq!((bless.scale, bless.offset), (0.0035, [0.005, 0.015, 0.0]));
    }

    #[test]
    fn punto_de_la_pantalla() {
        // The screen center lies on the axis, at 200 (0.1 · FAR) or 140 (0.07 · FAR).
        assert_eq!(screen_point(320.0, 240.0, false), [0.0, 0.0, -200.0]);
        assert_eq!(screen_point(320.0, 240.0, true), [0.0, 0.0, -140.0]);
        // The top edge, tan(0.5°) · 200 upwards.
        let top = screen_point(320.0, 0.0, false);
        assert!((top[1] - 200.0 * 0.5f32.to_radians().tan()).abs() < 1e-4);
        let right = screen_point(640.0, 240.0, false);
        assert!((right[0] - top[1] * 4.0 / 3.0).abs() < 1e-4);
    }
}
