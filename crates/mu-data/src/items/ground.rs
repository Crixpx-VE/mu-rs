//! The name over an item lying on the ground (`RenderItemName`, ZzzInventory.cpp:6495-7420) and the
//! one the chat shows when picking it up (`GetItemName`, ZzzInventory.cpp:1581-1780), with the texts
//! from `Text_eng.bmd` (`GlobalText[n]`, the number beside each).
//!
//! `RenderItemName` picks the text and a `glColor3f`; at the end the letters take that color only if
//! one of its channels is below 0.9 (`glGetFloatv(GL_CURRENT_COLOR)`, 7400-7403), otherwise they stay
//! white (or the set items' green). Branches that set no color leave whatever color OpenGL had; here
//! they are white. Not done yet: socket items (`g_SocketItemMgr.IsSocketItem`, violet), the set name
//! in front of set items (`GetSetItemName`: no `ItemSetType` data yet, only its colors), the jewel
//! bundles (`COMGEM::Check_Jewel_Com`), the summoning rings' monster names (`ITEM_HELPER+10`) and the
//! skill name of `ITEM_WING+11`; those use the general rule.

use super::group::*;
use super::model::{GroundModel, ground_model};
use super::{Item, kind};

/// `MODEL_ITEM` (_enum.h:1006, the generated `ids::MODEL_ITEM`): some branches index `ItemAttribute`
/// with a model number, i.e. the name of the item `MODEL_ITEM` places further on.
const MODEL_ITEM: usize = 1170;

/// `GlobalText` (Text_eng.bmd).
mod txt {
    pub const ZEN_EVENT: &str = "Zen"; // 100
    pub const HEART: &str = "Heart"; // 101
    pub const CHAOS_CERTIFICATE: &str = "Chaos Event Gift Certificate"; // 104
    pub const STAR: &str = "Star of Sacred Birth"; // 105
    pub const FIRECRACKER: &str = "Firecracker"; // 106
    pub const HEART_OF_LOVE: &str = "Heart of love"; // 107
    pub const OLIVE: &str = "Olive of love"; // 108
    pub const SILVER_MEDAL: &str = "Silver medal"; // 109
    pub const GOLD_MEDAL: &str = "Gold medal"; // 110
    pub const BOX_OF_HEAVEN: &str = "Box of Heaven"; // 111
    pub const BOX_OF_KUNDUN: &str = "Box of Kundun"; // 115
    pub const HEART_OF_DARK_LORD: &str = "Heart of Dark Lord"; // 117
    pub const STR: &str = "STR"; // 166
    pub const AGI: &str = "AGI"; // 167
    pub const ENG: &str = "ENG"; // 168
    pub const STA: &str = "STA"; // 169
    pub const SKILL: &str = "+Skill"; // 176
    pub const OPTION: &str = "+Option"; // 177
    pub const LUCK: &str = "+Luck"; // 178
    pub const KNIGHT_SKILL: &str = "Knight specific skill"; // 179
    pub const STONE: &str = "Stone"; // 810
    pub const RING_OF_HONOR: &str = "Ring of Honor"; // 906
    pub const DARK_STONE: &str = "Dark Stone"; // 907
    pub const RING_OF_WARRIOR: &str = "Ring of Warrior"; // 922, 928
    pub const RING_OF_WIZARD: &str = "Ring of Wizard"; // 925
    pub const RING_OF_GLORY: &str = "Ring of glory"; // 929
    pub const DARK_HORSE: &str = "Dark horse"; // 1187
    pub const DARK_RAVEN: &str = "Dark Raven"; // 1214
    pub const CREST_OF_MONARCH: &str = "Crest of monarch"; // 1235
    pub const SIGN_OF_LORD: &str = "Sign of lord"; // 1290
    pub const POTION_OF_BLESS: &str = "Potion of bless"; // 1413
    pub const POTION_OF_SOUL: &str = "Potion of soul"; // 1414
    pub const SCROLL_OF_GUARDIAN: &str = "Scroll of Guardian"; // 1416
    pub const ARCHER: &str = "Archer"; // 1460
    pub const SPEARMAN: &str = "Spearman"; // 1461
    pub const LIFE_STONE: &str = "Place Life Stone"; // 1462
    pub const BLUE_POUCH: &str = "Blue lucky pouch"; // 1650
    pub const RED_POUCH: &str = "Red lucky pouch"; // 1651
    pub const COMMAND: &str = "Command"; // 1900
    pub const LILAC_CANDY: &str = "Lilac candy box"; // 2012
    pub const ORANGE_CANDY: &str = "Orange candy box"; // 2013
    pub const NAVY_CANDY: &str = "Navy candy box"; // 2014
}

/// What `RenderItemName` writes: the text, its letters and background (`SetTextColor`,
/// `SetBgColor`, 0..255) and whether it uses `g_hFontBold`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundName {
    pub text: String,
    pub color: [u8; 3],
    pub background: [u8; 3],
    pub bold: bool,
}

const GOLD: [f32; 3] = [1.0, 0.8, 0.1];
const ORANGE: [f32; 3] = [0.9, 0.53, 0.13];

/// `RenderItemName(i, o, Level, Option1, ExtOption, ...)` for a ground item. `zen` is the amount of a
/// zen drop (its `Item.Level`); `name_of(kind)` is `ItemAttribute[kind].Name`.
pub fn ground_name(item: &Item, zen: Option<u32>, name_of: &dyn Fn(usize) -> Option<String>) -> GroundName {
    let (model, level) = ground_model(item);
    // `ItemLevel` (`ip->Level` with the option bits); `n->Level = 0` for the remapped event rings.
    let level_byte = if level == item.level { item.level_byte() } else { 0 };
    let name = |t: usize| name_of(t).unwrap_or_default();
    let mut bold = false;
    let mut color: Option<[f32; 3]> = None;
    let mut background = [0u8; 3];
    let mut text_color = [255u8; 3];
    let p = |i| kind(POTION, i);
    let h = |i| kind(HELPER, i);
    let w = |i| kind(WING, i);
    let first: Option<String> = match model {
        GroundModel::Item(t) if t == p(15) => {
            color = Some(GOLD);
            Some(format!("{} {}", name(t), zen.unwrap_or(0)))
        }
        GroundModel::Item(t) if [p(13), p(14), p(16), w(15), p(22), p(31), h(14), p(41), p(42), p(43), p(44)].contains(&t) => {
            bold = true;
            color = Some(GOLD);
            Some(name(t))
        }
        GroundModel::Item(t)
            if [h(46), h(47), h(48), p(54), p(53)].contains(&t)
                || (p(58)..=p(62)).contains(&t)
                || (p(145)..=p(150)).contains(&t)
                || (h(125)..=h(127)).contains(&t) =>
        {
            color = Some(ORANGE);
            Some(name(t))
        }
        // `ItemAttribute[o->Type - MODEL_HELPER]`: the sword with that index (6596-6600).
        GroundModel::Item(t) if (h(43)..=h(45)).contains(&t) => {
            color = Some(ORANGE);
            Some(name(t - h(0)))
        }
        GroundModel::Item(t)
            if [p(83), p(91), p(92), p(93), p(95), p(94), p(97), p(98), p(96), h(62), h(63), h(64), h(65), h(67), h(80), h(106), h(123), h(68), h(76), h(122)]
                .contains(&t)
                || (p(84)..=p(90)).contains(&t) =>
        {
            color = Some(ORANGE);
            Some(name(t))
        }
        GroundModel::Item(t) if (p(17)..=p(19)).contains(&t) => {
            bold = true;
            color = Some(GOLD);
            Some(if level == 0 { name(t) } else { format!("{} +{level}", name(t)) })
        }
        GroundModel::Item(t) if t == p(11) && level == 7 => {
            color = Some(GOLD);
            Some(txt::BOX_OF_HEAVEN.into())
        }
        GroundModel::Item(t) if t == p(12) => match level {
            0 => Some(txt::ZEN_EVENT.into()),
            1 => Some(txt::HEART.into()),
            2 => Some(txt::CHAOS_CERTIFICATE.into()),
            // `Name` stays as it was: nothing sensible to show.
            _ => Some(String::new()),
        },
        GroundModel::Item(t) if t == h(15) => {
            color = Some(GOLD);
            let stat = [txt::ENG, txt::STA, txt::AGI, txt::STR, txt::COMMAND].get(level as usize).copied();
            Some(stat.map_or(String::new(), |s| format!("{s} {}", name(t))))
        }
        GroundModel::Item(t) if t == h(31) => {
            color = Some(GOLD);
            Some(match level {
                0 => format!("{} of {}", name(t), txt::DARK_HORSE),
                1 => format!("{} of {}", name(t), txt::DARK_RAVEN),
                _ => String::new(),
            })
        }
        GroundModel::Event(16) => {
            color = Some(GOLD);
            Some(txt::CREST_OF_MONARCH.into())
        }
        GroundModel::Event(4) => Some(txt::STAR.into()),
        GroundModel::Event(5) => {
            color = Some(GOLD);
            Some(match level {
                14 => txt::BLUE_POUCH,
                15 => txt::RED_POUCH,
                _ => txt::FIRECRACKER,
            }
            .into())
        }
        GroundModel::Event(6) => {
            if level == 13 {
                color = Some(GOLD);
                Some(txt::HEART_OF_DARK_LORD.into())
            } else {
                Some(txt::HEART_OF_LOVE.into())
            }
        }
        GroundModel::Event(7) => Some(txt::OLIVE.into()),
        GroundModel::Event(8) => Some(txt::SILVER_MEDAL.into()),
        GroundModel::Event(9) => Some(txt::GOLD_MEDAL.into()),
        GroundModel::Event(10) => Some(format!("{} +{}", txt::BOX_OF_KUNDUN, level as i32 - 7)),
        GroundModel::Item(t) if t == w(32) => {
            color = Some([1.0, 0.3, 0.3]);
            Some(name(t))
        }
        GroundModel::Item(t) if t == w(33) => {
            color = Some([0.3, 1.0, 0.3]);
            Some(name(t))
        }
        GroundModel::Item(t) if t == w(34) => {
            color = Some([0.3, 0.3, 1.0]);
            Some(name(t))
        }
        GroundModel::Item(t) if t == p(64) => {
            color = Some(GOLD);
            Some(name(t))
        }
        GroundModel::Item(t) if t == w(48) || t == w(35) => {
            color = Some([1.0; 3]);
            Some(name(t))
        }
        GroundModel::Item(t) if (p(45)..=p(50)).contains(&t) => {
            color = Some(ORANGE);
            Some(name(t))
        }
        GroundModel::Item(t) if t == p(32) || t == p(33) || t == p(34) => {
            color = Some(match t - p(32) {
                0 => [1.0, 0.3, 1.0],
                1 => [1.0, 0.3, 0.3],
                _ => [0.3, 0.3, 1.0],
            });
            Some(if level == 0 { name(t) } else { String::new() })
        }
        GroundModel::Event(e @ 21..=23) => {
            color = Some(match e {
                21 => [1.0, 0.3, 1.0],
                22 => [1.0, 0.3, 0.3],
                _ => [0.3, 0.3, 1.0],
            });
            Some([txt::LILAC_CANDY, txt::ORANGE_CANDY, txt::NAVY_CANDY][(e - 21) as usize].into())
        }
        GroundModel::Event(11) => {
            color = Some(GOLD);
            Some(txt::STONE.into())
        }
        GroundModel::Event(12) => {
            color = Some(GOLD);
            Some(txt::RING_OF_HONOR.into())
        }
        GroundModel::Event(13) => {
            color = Some(GOLD);
            Some(txt::DARK_STONE.into())
        }
        GroundModel::Event(14) => {
            color = Some(GOLD);
            Some(match level {
                3 => txt::RING_OF_GLORY,
                _ => txt::RING_OF_WARRIOR,
            }
            .into())
        }
        GroundModel::Event(15) => {
            color = Some(GOLD);
            Some(txt::RING_OF_WIZARD.into())
        }
        GroundModel::Item(t) if t == p(21) && level == 3 => {
            color = Some(GOLD);
            Some(txt::SIGN_OF_LORD.into())
        }
        GroundModel::Item(t) if t == p(7) => Some(match level {
            0 => txt::POTION_OF_BLESS.into(),
            1 => txt::POTION_OF_SOUL.into(),
            _ => String::new(),
        }),
        GroundModel::Item(t) if t == h(7) => Some(match level {
            0 => txt::ARCHER.into(),
            1 => txt::SPEARMAN.into(),
            _ => String::new(),
        }),
        GroundModel::Item(t) if t == h(11) => Some(match level {
            0 => txt::SCROLL_OF_GUARDIAN.into(),
            1 => txt::LIFE_STONE.into(),
            _ => String::new(),
        }),
        GroundModel::Event(18) => Some(txt::LIFE_STONE.into()),
        _ => None,
    };
    let text = match (first, model) {
        (Some(t), _) => t,
        (None, GroundModel::Event(_)) => String::new(),
        (None, GroundModel::Item(t)) => {
            let lvl = level;
            if (w(60)..=w(65)).contains(&t) || (w(70)..=w(74)).contains(&t) || (w(100)..=w(129)).contains(&t) {
                color = Some([0.7, 0.4, 1.0]);
                name(t)
            } else if t == h(66) {
                color = Some([0.6, 0.4, 1.0]);
                name(t)
            } else if t == p(100) || t == h(99) {
                color = Some(GOLD);
                name(t)
            } else if (h(83)..=h(93)).contains(&t) {
                color = Some(ORANGE);
                name(t)
            } else if t == h(97) || t == h(98) || (h(109)..=h(115)).contains(&t) || t == p(112) || t == p(113) {
                // `ItemAttribute[o->Type]` / `ItemAttribute[MODEL_HELPER+109]`...: a model number as
                // the index (7169-7210).
                color = Some(GOLD);
                name(t + MODEL_ITEM)
            } else {
                let excellent_name = item.excellent & 63 > 0
                    && !(w(3)..=w(6)).contains(&t)
                    && t != h(30)
                    && !(w(36)..=w(40)).contains(&t)
                    && !(w(42)..=w(43)).contains(&t);
                // The cash shop items in between (7279-7330, in that order: they all come before
                // the level's colors); `ITEM_WING+130..=134` compares a model with an item number
                // and never matches.
                let shop_gold = (p(101)..=p(109)).contains(&t)
                    || t == p(111)
                    || (p(120)..=p(124)).contains(&t)
                    || (p(134)..=p(139)).contains(&t)
                    || (p(114)..=p(119)).contains(&t)
                    || (p(126)..=p(132)).contains(&t)
                    || t == h(121);
                let shop_orange = t == h(116) || (p(141)..=p(144)).contains(&t);
                color = Some(if [kind(STAFF, 10), kind(SWORD, 19), kind(BOW, 18), kind(MACE, 13)].contains(&t) {
                    [1.0, 0.1, 1.0]
                } else if excellent_name {
                    [0.1, 1.0, 0.5]
                } else if shop_orange {
                    ORANGE
                } else if shop_gold || t == w(25) || t == p(28) || t == p(29) || lvl >= 7 {
                    GOLD
                } else if level_byte & (128 | 4 | 2 | 1) != 0 {
                    [0.4, 0.7, 1.0]
                } else if lvl == 0 {
                    [0.7, 0.7, 0.7]
                } else if lvl < 3 {
                    [0.9, 0.9, 0.9]
                } else if lvl < 5 {
                    [1.0, 0.5, 0.2]
                } else {
                    [0.4, 0.7, 1.0]
                });
                if item.is_ancient() {
                    // `GetSetItemName`: green letters on blue, bold (7363-7370). The set's name is not
                    // known yet (no `ItemSetType` data).
                    color = Some([1.0; 3]);
                    bold = true;
                    text_color = [0, 255, 0];
                    background = [60, 60, 200];
                }
                let mut s = if lvl == 0 { name(t) } else { format!("{} +{lvl}", name(t)) };
                if item.skill {
                    if t != h(3) {
                        s.push_str(txt::SKILL);
                    } else {
                        s.push_str(" +");
                        s.push_str(txt::KNIGHT_SKILL);
                    }
                }
                if item.option > 0 {
                    s.push_str(txt::OPTION);
                }
                if item.luck {
                    s.push_str(txt::LUCK);
                }
                s
            }
        }
    };
    // `if (fCurColor[0] < 0.9f || ...) SetTextColor(fCurColor * 255)`.
    if let Some(c) = color
        && c.iter().any(|&v| v < 0.9)
    {
        text_color = c.map(|v| (v * 255.0) as u8);
    }
    GroundName { text, color: text_color, background, bold }
}

/// `GetItemName(Type, Level, ...)` (ZzzInventory.cpp:1581-1780) for the "<name> Obtained" chat line,
/// with the cases a ground item can have; the rest is "name" or "name +level".
pub fn obtained_name(item: &Item, name_of: &dyn Fn(usize) -> Option<String>) -> String {
    let t = item.kind();
    let level = item.level;
    let name = name_of(t).unwrap_or_default();
    let p = |i| kind(POTION, i);
    match t {
        _ if t == p(23) && level == 1 => txt::RING_OF_HONOR.into(),
        _ if t == p(24) && level == 1 => txt::DARK_STONE.into(),
        _ if (p(23)..=p(26)).contains(&t) => name,
        _ if t == p(12) => match level {
            0 => txt::ZEN_EVENT.into(),
            1 => txt::HEART.into(),
            _ => name,
        },
        _ if t == p(11) => match level {
            0 => name,
            1 => txt::STAR.into(),
            2 => txt::FIRECRACKER.into(),
            3 => txt::HEART_OF_LOVE.into(),
            5 => txt::SILVER_MEDAL.into(),
            6 => txt::GOLD_MEDAL.into(),
            7 => txt::BOX_OF_HEAVEN.into(),
            8..=12 => format!("{} +{}", txt::BOX_OF_KUNDUN, level - 7),
            13 => txt::HEART_OF_DARK_LORD.into(),
            14 => txt::BLUE_POUCH.into(),
            15 => txt::RED_POUCH.into(),
            _ => String::new(),
        },
        _ if t == p(7) => if level == 0 { txt::POTION_OF_BLESS } else { txt::POTION_OF_SOUL }.into(),
        _ if (kind(WING, 60)..=kind(WING, 65)).contains(&t)
            || (kind(WING, 70)..=kind(WING, 74)).contains(&t)
            || (kind(WING, 100)..=kind(WING, 129)).contains(&t) =>
        {
            name
        }
        _ if level == 0 => name,
        _ => format!("{name} +{level}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(t: usize) -> Option<String> {
        Some(
            match t {
                _ if t == kind(POTION, 15) => "Zen",
                _ if t == kind(POTION, 13) => "Jewel of Bless",
                _ if t == kind(SWORD, 1) => "Kris",
                _ if t == kind(ARMOR, 5) => "Leather Armor",
                _ => return None,
            }
            .into(),
        )
    }

    /// The colors of `RenderItemName`'s general rule (ZzzInventory.cpp:7310-7360) and when they are used.
    #[test]
    fn name_colors() {
        let kris = |f: fn(&mut Item)| {
            let mut i = Item::new(SWORD, 1);
            f(&mut i);
            ground_name(&i, None, &names)
        };
        // +0 without options: gray (0.7 · 255 = 178).
        let plain = kris(|_| {});
        assert_eq!((plain.text.as_str(), plain.color, plain.background, plain.bold), ("Kris", [178; 3], [0; 3], false));
        // +1 / +2: (0.9, 0.9, 0.9) is not below 0.9: white letters.
        assert_eq!(kris(|i| i.level = 2).color, [255; 3]);
        assert_eq!(kris(|i| i.level = 2).text, "Kris +2");
        // +3 / +4 orange, +5 / +6 blue, +7 and more gold.
        assert_eq!(kris(|i| i.level = 4).color, [255, 127, 51]);
        assert_eq!(kris(|i| i.level = 6).color, [102, 178, 255]);
        assert_eq!(kris(|i| i.level = 9).color, [255, 204, 25]);
        // Luck or skill: blue, with the suffixes after the level.
        let lucky = kris(|i| {
            i.level = 1;
            i.luck = true;
            i.skill = true;
            i.option = 1;
        });
        assert_eq!((lucky.text.as_str(), lucky.color), ("Kris +1+Skill+Option+Luck", [102, 178, 255]));
        // Only the option's high bit (`Option1 & 64`): "+Option" but not blue.
        let opt4 = kris(|i| i.option = 4);
        assert_eq!((opt4.text.as_str(), opt4.color), ("Kris+Option", [178; 3]));
        // Excellent: green, before the level.
        assert_eq!(kris(|i| {
            i.excellent = 1;
            i.level = 9;
        })
        .color, [25, 255, 127]);
        // Set item: green letters on blue, bold.
        let set = ground_name(&Item { ancient: 5, ..Item::new(ARMOR, 5) }, None, &names);
        assert_eq!((set.color, set.background, set.bold), ([0, 255, 0], [60, 60, 200], true));
        // Zen: "Zen <amount>" in gold; a jewel in bold gold.
        let zen = ground_name(&Item::new(POTION, 15), Some(12345), &names);
        assert_eq!((zen.text.as_str(), zen.color, zen.bold), ("Zen 12345", [255, 204, 25], false));
        let bless = ground_name(&Item::new(POTION, 13), None, &names);
        assert_eq!((bless.text.as_str(), bless.color, bless.bold), ("Jewel of Bless", [255, 204, 25], true));
        // A Box of Kundun +3 (`ITEM_POTION+11` level 10 → `MODEL_EVENT+10`): no color of its own.
        let kundun = ground_name(&Item { level: 10, ..Item::new(POTION, 11) }, None, &names);
        assert_eq!((kundun.text.as_str(), kundun.color), ("Box of Kundun +3", [255; 3]));
    }

    /// `CreateItem` + `ItemObjectAttribute` + `ItemAngle` (ZzzObject.cpp:5286-6324).
    #[test]
    fn ground_poses() {
        use super::super::model::{ground_lift, ground_pose};
        let pose = |g, i, level| ground_pose(&Item { level, ..Item::new(g, i) });
        // A sword: tilted 60° around x, scale 0.8; 70 above the terrain (30 + 40 for weapons).
        let sword = pose(SWORD, 1, 0);
        assert_eq!((sword.angle, sword.scale, sword.player_skeleton), ([60.0, 0.0, -45.0], 0.8, false));
        assert_eq!(ground_lift(&Item::new(SWORD, 1)), 70.0);
        // A staff (0.7 from `ItemObjectAttribute`), a shield and an armor (player skeleton, -100).
        assert_eq!((pose(STAFF, 1, 0).angle, pose(STAFF, 1, 0).scale), ([0.0, 270.0, -45.0], 0.7));
        assert_eq!(pose(SHIELD, 2, 0).angle, [0.0, 270.0, 225.0]);
        let armor = pose(ARMOR, 5, 0);
        assert_eq!((armor.angle, armor.player_skeleton, armor.body_height), ([270.0, 0.0, -45.0], true, -100.0));
        assert_eq!(ground_lift(&Item::new(ARMOR, 5)), 30.0);
        // Wings and a jewel.
        assert_eq!(pose(WING, 1, 0).angle, [270.0, 0.0, 45.0]);
        assert_eq!(pose(POTION, 13, 0).angle, [0.0, 0.0, -45.0]);
        // A Box of Kundun +1 draws `MODEL_EVENT+10` (MagicBox08) at 0.2.
        assert_eq!(ground_model(&Item { level: 8, ..Item::new(POTION, 11) }).0, GroundModel::Event(10));
        assert_eq!(pose(POTION, 11, 8).scale, 0.2);
        assert_eq!(super::super::model::event_model_file(10).map(|f| f.name), Some("MagicBox08.bmd".into()));
        // The Lord's ring of level 1 is drawn as the Sword of Destruction, at level 0.
        assert_eq!(ground_model(&Item { level: 1, ..Item::new(HELPER, 19) }), (GroundModel::Item(kind(SWORD, 19)), 0));
    }

    /// `IsDropBan` (ZzzInventory.cpp:8312-8361).
    #[test]
    fn drop_bans() {
        use super::super::price::is_drop_ban;
        assert!(!is_drop_ban(&Item::new(SWORD, 1)));
        assert!(!is_drop_ban(&Item::new(POTION, 13)));
        // Quest items, the cash shop's and the lucky tickets and armor.
        assert!(is_drop_ban(&Item::new(POTION, 23)));
        assert!(is_drop_ban(&Item::new(HELPER, 43)));
        assert!(is_drop_ban(&Item::new(HELPER, 135)));
        assert!(is_drop_ban(&Item::new(ARMOR, 62)));
        // Not period items: droppable.
        assert!(!is_drop_ban(&Item::new(POTION, 54)));
        assert!(!is_drop_ban(&Item::new(HELPER, 20)));
        assert!(!is_drop_ban(&Item { level: 1, ..Item::new(HELPER, 20) }));
    }

    #[test]
    fn obtained_names() {
        assert_eq!(obtained_name(&Item { level: 3, ..Item::new(SWORD, 1) }, &names), "Kris +3");
        assert_eq!(obtained_name(&Item::new(POTION, 13), &names), "Jewel of Bless");
        assert_eq!(obtained_name(&Item { level: 9, ..Item::new(POTION, 11) }, &names), "Box of Kundun +2");
    }
}
