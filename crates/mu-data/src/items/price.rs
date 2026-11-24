//! Item prices and the shop rules of the original client:
//!
//! - [`item_value`]: `ItemValue` (ZzzInfomation.cpp:1575-2338), the buy, sell and repair base price.
//! - [`convert_repair_gold`]: `ConvertRepairGold` (ZzzInventory.cpp:1148-1212), the repair cost.
//! - [`repair_all_gold`]: `RepairAllGold` (ZzzInventory.cpp:1214-1467), the "Repair all" cost.
//! - The bans: [`is_selling_ban`], [`is_repair_ban`], [`is_high_value_item`], [`is_part_charge_item`].
//! - The tooltip lines: [`price_line`] (the price line `RenderItemInfo` adds with the NPC shop open,
//!   ZzzInventory.cpp:2189-2213) and [`repair_info`] (`RenderRepairInfo`, ZzzInventory.cpp:5642-5948).
//!
//! The server does not send prices: OpenMU computes its own (`GameLogic/ItemPriceCalculator.cs`)
//! and is the authority over the zen. Its formulas are data driven but give the same numbers for
//! the ordinary items checked in the tests; known differences: it applies the skill bonus to any
//! item with skill (the original only to the skills listed in [`SKILL_PRICE`]) and it never raises
//! the selling price of an item with more durability than its maximum (the original's signed
//! `persent`, see [`item_value`]).
//!
//! Not modelled (the item bytes here do not carry them): period items (`bPeriodItem`), sockets
//! (`CalcSocketBonusItemValue` counts as 0), harmony options and the pet data of the Dark Horse and
//! Dark Raven (their value comes from `giPetManager::GetPetItemValue`: [`item_value`] gives `None`).
//!
//! Float math: the original stores to `float` variables but computes on the x87 with MSVC's double
//! precision control; that is emulated by doing each expression in `f64` and rounding to `f32` where
//! the source assigns to a `float`.

use super::group::*;
use super::info::{HeroView, TextColor, TipLine, name_color};
use super::{Item, ItemAttribute, ItemStats, Special, convert, kind, max_durability};

/// `goldType` of `ItemValue`: 0 buying from an NPC, 1 selling to it (the default), 2 the repair base.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GoldType {
    Buy = 0,
    Sell = 1,
    Repair = 2,
}

/// Where a repair is paid, which changes the price (`ConvertRepairGold`, ZzzInventory.cpp:1187-1198).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepairPlace {
    /// The NPC shop is open and repairs: the plain price.
    NpcShop,
    /// The inventory alone (its own repair button): 2.5 times the price.
    SelfRepair,
    /// Anything else: the plain price.
    Other,
}

const fn k(group: u8, index: u16) -> usize {
    kind(group, index)
}

/// The rounding repeated all over the price code: hundreds from 1000, tens from 100.
fn round_price(gold: i64) -> i64 {
    if gold >= 1000 {
        gold / 100 * 100
    } else if gold >= 100 {
        gold / 10 * 10
    } else {
        gold
    }
}

/// The skills whose option raises the price by 150% (`AT_SKILL_*` of ZzzInfomation.cpp:2045-2070,
/// values from SkillManager.h:28-48, 139-141 and 153-194): blocking, the sword skills, triple shot,
/// ice blade, recover, multi shot, gaotic, the arrow and power slash masteries.
pub const SKILL_PRICE: [u16; 21] = [18, 19, 20, 21, 22, 23, 24, 56, 234, 235, 238, 490, 491, 492, 493, 494, 505, 506, 507, 508, 509];

/// What `ItemValue` looks at in `ip->Special[]` (ZzzInfomation.cpp:2041-2151).
#[derive(Clone, Copy, PartialEq, Debug)]
enum ValueSpecial {
    /// One of the skills of [`SKILL_PRICE`]: +150%.
    Skill,
    /// `AT_IMPROVE_DAMAGE`, `_MAGIC`, `_CURSE`, `_DEFENSE` (the additional option) with its value.
    Option(u32),
    /// `AT_LIFE_REGENERATION` with its value.
    LifeRegeneration(u32),
    /// `AT_IMPROVE_BLOCKING` with its value.
    Blocking(u32),
    /// `AT_LUCK`: +25%.
    Luck,
    /// An excellent option: doubles it (and `Excellent` adds 25 to the level, 1603-1627).
    Excellent,
    /// The 2nd and 3rd wing options (`AT_IMPROVE_HP_MAX`... `AT_RECOVER_FULL_MANA`): +25%.
    Quarter,
}

fn value_special(s: Special, value: u32) -> Option<ValueSpecial> {
    use Special::*;
    Some(match s {
        Skill(n) if SKILL_PRICE.contains(&n) => ValueSpecial::Skill,
        Skill(_) | ImproveMaxMana | ImproveMaxAg => return None,
        ImproveDamage | ImproveMagic | ImproveCurse | ImproveDefense => ValueSpecial::Option(value),
        LifeRegeneration => ValueSpecial::LifeRegeneration(value),
        ImproveBlocking => ValueSpecial::Blocking(value),
        Luck => ValueSpecial::Luck,
        ImproveLife | ImproveMana | DecreaseDamage | ReflectionDamage | ImproveBlockingPercent | ImproveGainGold | ExcellentDamage
        | ImproveDamageLevel | ImproveDamagePercent | ImproveMagicLevel | ImproveMagicPercent | ImproveAttackSpeed | ImproveGainLife
        | ImproveGainMana => ValueSpecial::Excellent,
        ImproveHpMax | ImproveMpMax | OnePercentDamage | ImproveAgMax => ValueSpecial::Quarter,
    })
}

/// 3rd-class wings (`ITEM_WING+36` to `+40` and `+43`).
fn third_wing(t: usize) -> bool {
    (k(WING, 36)..=k(WING, 40)).contains(&t) || t == k(WING, 43)
}

/// `ip->Special[]` in the original's order, as `ItemValue` reads it. [`convert`] has most of them;
/// the 3rd wing options it lacks (ZzzInfomation.cpp:1063-1089: `excelWing` bits 0 to 3, pushed
/// before the skill) are added here.
fn value_specials(item: &Item, stats: &ItemStats) -> Vec<ValueSpecial> {
    let mut out = Vec::new();
    if third_wing(item.kind()) {
        out.extend((0..4).filter(|b| (item.excellent >> b) & 1 != 0).map(|_| ValueSpecial::Quarter));
    }
    out.extend(stats.specials.iter().filter_map(|&(s, v)| value_special(s, v)));
    out
}

/// `COMGEM::CalcItemValue` (CComGem.cpp:226-260): bundles of jewels (`Check_Jewel_Com`,
/// CComGem.cpp:337-349) are worth `(Level + 1) · FIRST` (10, _enum.h:1973) jewels. The `int`
/// multiplication wraps like the original's.
fn compiled_gem_value(item: &Item) -> Option<i32> {
    if item.group != WING {
        return None;
    }
    let each: i32 = match item.index {
        30 => 9_000_000,
        31 => 6_000_000,
        136 => 45_000_000,
        137 => 36_000_000,
        138 => 60_000_000,
        141 => 810_000,
        139 | 140 | 142 | 143 => 18_600,
        _ => return None,
    };
    Some(each.wrapping_mul(item.level as i32 + 1).wrapping_mul(10))
}

/// `COMGEM::isCompiledGem`.
pub fn is_compiled_gem(item: &Item) -> bool {
    compiled_gem_value(item).is_some()
}

/// `Check_LuckyItem` (ZzzInventory.cpp:8282-8290, `LEM_ADD_LUCKYITEM`): helms to boots and
/// `ITEM_WING` itself with an index of 62 to 72.
pub fn is_lucky_item(t: usize) -> bool {
    (k(HELM, 0)..=k(WING, 0)).contains(&t) && (62..=72).contains(&(t % super::MAX_ITEM_INDEX))
}

/// `Check_ItemAction(item, action)` with the default `_bType = false` (ZzzInventory.cpp:8233-8280):
/// the lucky tickets (`ITEM_HELPER+135` to `+146`, `ITEM_POTION+160` and `+161`, option row 0) can be
/// neither sold nor repaired; the lucky armor (`+62` to `+73` of helms, armors, boots, gloves and
/// pants, row 1) can be sold but not repaired.
fn item_action_ban(t: usize, sell: bool) -> bool {
    let ticket = (k(HELPER, 135)..=k(HELPER, 146)).contains(&t) || (k(POTION, 160)..=k(POTION, 161)).contains(&t);
    let armor = [HELM, ARMOR, PANTS, GLOVES, BOOTS].iter().any(|&g| (k(g, 62)..=k(g, 73)).contains(&t));
    ticket || (armor && !sell)
}

/// `ITEM_TYPE_CHARM_MIXWING + EWS_BEGIN..EWS_END` (_enum.h:2285-2301): `ITEM_HELPER+83` to `+93`.
fn wing_charm(t: usize) -> bool {
    (k(HELPER, 83)..=k(HELPER, 93)).contains(&t)
}

/// `ItemValue(ip, goldType)` (ZzzInfomation.cpp:1575-2338). `None` where the original needs data
/// not modelled here: the Dark Horse and Dark Raven (`giPetManager::GetPetItemValue`) and the
/// additional option of the 3rd wings and `ITEM_WING+42`, which [`convert`] does not compute yet.
pub fn item_value(attr: &ItemAttribute, item: &Item, gold_type: GoldType) -> Option<i32> {
    let t = item.kind();
    let gt = gold_type as i64;
    // `p->iZen`: a fixed price (1583-1598).
    if attr.zen != 0 {
        let mut gold = attr.zen as i64;
        if gt != 0 {
            gold /= 3;
        }
        return Some(round_price(gold) as i32);
    }
    // The pets' value comes from their pet data (2196-2204).
    if t == k(HELPER, 4) || t == k(HELPER, 5) {
        return None;
    }
    if (third_wing(t) || t == k(WING, 42)) && item.option > 0 {
        return None;
    }
    let group = item.group;
    let level = item.level.min(15) as i64;
    let durability = item.durability as i64;
    let stats = convert(attr, item, 1);
    let specials = value_specials(item, &stats);
    let excellent = specials.contains(&ValueSpecial::Excellent);
    let mut level2 = attr.level as i64 + level * 3;
    if excellent {
        level2 += 25;
    }
    let by_level = |values: &[i64]| values.get(level as usize).copied().unwrap_or(0);

    let mut gold: i64 = if t == k(BOW, 7) {
        // Arrows (1629-1650): by level and the fraction of the stack.
        let money = by_level(&[100, 1400, 2200, 3000]);
        if attr.durability > 0 { money * durability / attr.durability as i64 } else { 0 }
    } else if t == k(BOW, 15) {
        let money = by_level(&[70, 1200, 2000, 2800]);
        if attr.durability > 0 { money * durability / attr.durability as i64 } else { 0 }
    } else if let Some(v) = compiled_gem_value(item) {
        v as i64
    } else if t == k(POTION, 13) {
        9_000_000
    } else if t == k(POTION, 14) {
        6_000_000
    } else if t == k(WING, 15) {
        810_000
    } else if t == k(POTION, 16) {
        45_000_000
    } else if t == k(POTION, 22) {
        36_000_000
    } else if t == k(POTION, 141) {
        224_000 * 3
    } else if t == k(POTION, 142) {
        182_000 * 3
    } else if t == k(POTION, 143) {
        157_000 * 3
    } else if t == k(POTION, 144) {
        121_000 * 3
    } else if t == k(HELPER, 14) {
        by_level(&[180_000, 7_500_000])
    } else if t == k(HELPER, 3) {
        // The Dinorant (1722-1736): each of its options (`Option3` bits, ZzzInfomation.cpp:1139-1156)
        // adds 300 000.
        960_000 + 300_000 * (item.option & 7).count_ones() as i64
    } else if t == k(HELPER, 15) {
        33_000_000
    } else if t == k(HELPER, 16) || t == k(HELPER, 17) {
        by_level(&[0, 10_000, 50_000, 100_000, 300_000, 500_000, 800_000, 1_000_000, 1_200_000])
    } else if t == k(HELPER, 18) {
        let g = if level == 1 { 50_000 } else { 200_000 + 20_000 * (level - 1) };
        g * 3
    } else if t == k(POTION, 28) || t == k(POTION, 111) {
        200_000 * 3
    } else if t == k(POTION, 29) || t == k(POTION, 110) {
        durability * 10_000 * 3
    } else if t == k(POTION, 102) || t == k(POTION, 109) {
        10_000 * 3
    } else if t == k(POTION, 101) || (k(POTION, 103)..=k(POTION, 108)).contains(&t) {
        durability * 10_000 * 3
    } else if t == k(HELPER, 29) {
        5000
    } else if t == k(POTION, 21) {
        match level {
            0..=2 => 9000,
            3 => durability * 3900,
            _ => 0,
        }
    } else if t == k(POTION, 17) {
        [30_000, 10_000, 50_000, 100_000, 300_000, 500_000, 800_000, 1_000_000][level.min(7) as usize]
    } else if t == k(POTION, 18) {
        [30_000, 15_000, 75_000, 150_000, 450_000, 750_000, 1_200_000, 1_500_000][level.min(7) as usize]
    } else if t == k(POTION, 19) {
        [120_000, 60_000, 84_000, 120_000, 180_000, 240_000, 300_000, 180_000][level.min(7) as usize]
    } else if (k(HELPER, 49)..=k(HELPER, 51)).contains(&t) {
        match level {
            1..=6 => [500_000, 600_000, 800_000, 1_000_000, 1_200_000, 1_400_000][level as usize - 1],
            _ => 3000 * 3,
        }
    } else if (k(POTION, 23)..=k(POTION, 26)).contains(&t) || (k(POTION, 65)..=k(POTION, 68)).contains(&t) {
        9000
    } else if t == k(POTION, 9) && level == 1 {
        1000
    } else if t == k(POTION, 20) {
        900
    } else if t == k(POTION, 51) || t == k(POTION, 63) {
        200_000
    } else if t == k(POTION, 52) {
        33 * 3
    } else if t == k(POTION, 10) {
        750
    } else if t == k(POTION, 31) {
        60_000_000
    } else if t == k(POTION, 7) {
        by_level(&[900_000, 450_000]) * durability
    } else if t == k(HELPER, 7) {
        by_level(&[1_500_000, 1_200_000])
    } else if t == k(HELPER, 11) {
        by_level(&[100_000, 2_400_000])
    } else if (k(POTION, 35)..=k(POTION, 37)).contains(&t) {
        [2000, 4000, 6000][t - k(POTION, 35)] * durability
    } else if (k(POTION, 38)..=k(POTION, 40)).contains(&t) {
        [2500, 5000, 7500][t - k(POTION, 38)] * durability
    } else if [k(POTION, 100), k(POTION, 90), k(POTION, 85), k(POTION, 86), k(POTION, 87)].contains(&t) {
        // `ITEM_POTION+100`: `100 · 3` each; the others 300 each.
        300 * durability
    } else if attr.value > 0 {
        // `p->Value` (1941-1965): potions are priced per piece and return here.
        let mut g = attr.value as i64 * attr.value as i64 * 10 / 12;
        if t == k(POTION, 3) || t == k(POTION, 6) {
            g = 1500;
        }
        if (k(POTION, 0)..=k(POTION, 8)).contains(&t) {
            if level > 0 {
                g *= 1 << level;
            }
            g = g / 10 * 10;
            g *= durability;
            if gt != 0 {
                g /= 3;
                g = g / 10 * 10;
            }
            return Some(g as i32);
        }
        g
    } else if t == k(HELPER, 20) {
        if level == 0 { 3000 } else { 0 }
    } else if t == k(HELPER, 52) || t == k(HELPER, 53) {
        3_000_000
    } else if t == k(HELPER, 31) {
        by_level(&[10_000_000 * 3, 5_000_000 * 3])
    } else if (group == WING && t > k(WING, 6) && !(k(WING, 36)..=k(WING, 43)).contains(&t)) || group == HELPER || group == ETC {
        // Jewelry, helpers, orbs and scrolls (1985-2003).
        let mut g = 100 + level2 * level2 * level2;
        for s in &specials {
            if let ValueSpecial::LifeRegeneration(v) = s {
                g += g * *v as i64;
            }
        }
        g
    } else if t == k(POTION, 53) {
        0
    } else {
        // Weapons, armor and wings (2008-2153).
        level2 += match level {
            5 => 4,
            6 => 10,
            7 => 25,
            8 => 45,
            9 => 65,
            10 => 95,
            11 => 135,
            12 => 185,
            13 => 245,
            14 => 305,
            15 => 365,
            _ => 0,
        };
        let wing = (group == WING && t <= k(WING, 6)) || (k(WING, 36)..=k(WING, 43)).contains(&t);
        let mut g = if wing || t == k(HELPER, 30) {
            40_000_000 + (40 + level2) * level2 * level2 * 11
        } else {
            100 + (40 + level2) * level2 * level2 / 8
        };
        if group <= STAFF && !attr.two_hand {
            g = g * 80 / 100;
        }
        let percent = |g: i64, by: f64| g + (g as f64 * by) as i64;
        let option_step = |g: i64, v: u32| match v {
            4 => percent(g, 6.0 / 10.0),
            8 => percent(g, 14.0 / 10.0),
            12 => percent(g, 28.0 / 10.0),
            16 => percent(g, 56.0 / 10.0),
            _ => g,
        };
        for s in &specials {
            g = match *s {
                ValueSpecial::Skill => percent(g, 1.5),
                ValueSpecial::Option(v) => option_step(g, v),
                // On wings the regeneration counts four times its value (2085-2089).
                ValueSpecial::LifeRegeneration(v) => option_step(g, if wing { v * 4 } else { v }),
                ValueSpecial::Blocking(v) => match v {
                    5 => percent(g, 6.0 / 10.0),
                    10 => percent(g, 14.0 / 10.0),
                    15 => percent(g, 28.0 / 10.0),
                    20 => percent(g, 56.0 / 10.0),
                    _ => g,
                },
                ValueSpecial::Luck | ValueSpecial::Quarter => percent(g, 25.0 / 100.0),
                ValueSpecial::Excellent => g + g,
            };
        }
        // `g_SocketItemMgr.CalcSocketBonusItemValue`: no sockets here.
        g
    };
    gold = gold.min(3_000_000_000);
    if gold_type == GoldType::Repair {
        gold = round_price(gold);
    }
    if gt != 0 {
        gold /= 3;
    }
    // Fixed values after the division (2173-2241).
    if group == HELPER {
        match item.index {
            32 => gold = durability * 50,
            33 => gold = durability * 100,
            34 => gold = durability * 1000,
            35 => gold = durability * 10_000,
            36 => gold = 30_000,
            37 => gold = 50_000,
            _ => {}
        }
    }
    if (k(POTION, 45)..=k(POTION, 50)).contains(&t) {
        gold = durability * 50;
    }
    if (k(HELPER, 71)..=k(HELPER, 75)).contains(&t) {
        gold = 2_000_000;
    }
    let thousand = [k(POTION, 112), k(POTION, 113), k(POTION, 121), k(POTION, 122), k(POTION, 123), k(POTION, 124), k(HELPER, 80), k(HELPER, 76), k(HELPER, 64), k(HELPER, 65)];
    if thousand.contains(&t) || (k(HELPER, 109)..=k(HELPER, 115)).contains(&t) {
        gold = 1000;
    } else if t == k(HELPER, 122) || t == k(HELPER, 123) {
        gold = 2000;
    } else if (k(WING, 130)..=k(WING, 134)).contains(&t) {
        gold = 80;
    }
    // Items whose price does not drop with their durability (2243-2308).
    let keeps_price = [k(HELPER, 10), k(HELPER, 20), k(HELPER, 29), k(BOW, 7), k(BOW, 15), k(POTION, 21), k(HELPER, 7), k(HELPER, 11)].contains(&t)
        || t >= k(POTION, 0)
        || (k(WING, 7)..=k(WING, 19)).contains(&t)
        || (k(HELPER, 14)..=k(HELPER, 19)).contains(&t)
        || (k(HELPER, 49)..=k(HELPER, 51)).contains(&t)
        || (k(WING, 130)..=k(WING, 134)).contains(&t)
        || [k(HELPER, 122), k(HELPER, 123), k(HELPER, 80), k(HELPER, 76), k(HELPER, 64), k(HELPER, 65)].contains(&t)
        || (k(HELPER, 109)..=k(HELPER, 115)).contains(&t)
        || (k(HELPER, 128)..=k(HELPER, 134)).contains(&t);
    if !keeps_price && gold_type == GoldType::Sell && !(k(HELPER, 32)..=k(HELPER, 37)).contains(&t) {
        // 2310-2322: `persent = 1 - Durability / maxDurability` (both as `float`) and
        // `repairGold = (DWORD)(Gold * 0.6f * persent)`. An infinite or NaN product (no maximum)
        // converts to the x87 "integer indefinite", whose low 32 bits are 0; a negative one (more
        // durability than the maximum) wraps as a `DWORD` and the subtraction is undone by the final
        // `(int)` cast.
        let max = max_durability(attr, item) as f32;
        let persent = 1.0f32 - (item.durability as f32 / max);
        let product = gold as f64 * 0.6f32 as f64 * persent as f64;
        let mut repair = if product.is_finite() { product.trunc() as i64 as u32 } else { 0 };
        if t == k(HELPER, 31) {
            repair = 0;
        }
        gold -= repair as i64;
    }
    gold = round_price(gold);
    if is_lucky_item(t) {
        gold = 0;
    }
    Some(gold as i32)
}

/// `ConvertRepairGold(Gold, Durability, MaxDurability, Type, Text)` (ZzzInventory.cpp:1148-1212):
/// the cost of repairing an item whose repair base is `gold` (`ItemValue(ip, 2)`).
pub fn convert_repair_gold(gold: i32, durability: i32, max_durability: i32, t: usize, place: RepairPlace) -> i32 {
    let gold = gold.min(400_000_000);
    let mut repair = gold as f32;
    let persent = 1.0f32 - (durability as f32 / max_durability as f32);
    if persent > 0.0 {
        let root = (repair as f64).sqrt() as f32;
        let root_root = (repair as f64).sqrt().sqrt() as f32;
        repair = (3.0 * root as f64 * root_root as f64) as f32;
        repair = (repair as f64 * persent as f64) as f32;
        repair += 1.0;
        if durability <= 0 {
            if t == k(HELPER, 4) || t == k(HELPER, 5) {
                repair *= 2.0;
            } else {
                repair = (repair as f64 + repair as f64 * 0.4f32 as f64) as f32;
            }
        }
    } else {
        repair = 0.0;
    }
    let gold = match place {
        RepairPlace::SelfRepair => (repair as f64 + repair as f64 * 1.5) as i32,
        RepairPlace::NpcShop | RepairPlace::Other => repair as i32,
    };
    round_price(gold as i64) as i32
}

/// The repair cost of one item, as `RepairAllGold` adds it (ZzzInventory.cpp:1236-1243 and 1457-1463):
/// `None` with full durability. Lucky items repair for free.
pub fn repair_cost(attr: &ItemAttribute, item: &Item, place: RepairPlace) -> Option<i32> {
    let max = max_durability(attr, item) as i32;
    if item.durability as i32 >= max {
        return None;
    }
    let base = item_value(attr, item, GoldType::Repair)?;
    let gold = convert_repair_gold(base, item.durability as i32, max, item.kind(), place);
    Some(if is_lucky_item(item.kind()) { 0 } else { gold })
}

/// The inventory items `RepairAllGold` leaves out (ZzzInventory.cpp:1260-1453). The equipment uses
/// [`is_repair_ban`] instead (1230).
pub fn repair_all_skips(t: usize) -> bool {
    let h = |i| k(HELPER, i);
    let p = |i| k(POTION, i);
    let in_h = |a, b| (h(a)..=h(b)).contains(&t);
    let in_p = |a, b| (p(a)..=p(b)).contains(&t);
    in_p(55, 57)
        || in_h(0, 5)
        || t == h(10)
        || t == h(31)
        || t == k(BOW, 7)
        || t == k(BOW, 15)
        || t >= p(0)
        || (k(WING, 7)..=k(WING, 19)).contains(&t)
        || in_h(14, 20)
        || t == h(38)
        || in_h(46, 48)
        || in_h(125, 127)
        || in_h(43, 45)
        || t == h(59)
        || in_h(54, 58)
        || in_h(60, 61)
        || in_h(62, 63)
        || [h(64), h(65), h(66), h(67), h(68), h(69), h(70), h(76), h(80), h(81), h(82), h(93), h(94), h(97), h(98), h(106), h(107), h(121), h(122), h(123)].contains(&t)
        || t == h(37)
        || in_h(71, 75)
        || wing_charm(t)
        || (k(WING, 130)..=k(WING, 134)).contains(&t)
        || in_h(109, 115)
        || item_action_ban(t, false)
}

/// `RepairAllGold` (ZzzInventory.cpp:1214-1467): what "Repair all" costs, for the equipped items
/// (`equipment`) and those in the inventory grid (`grid`).
pub fn repair_all_gold<'a>(
    equipment: impl IntoIterator<Item = (&'a ItemAttribute, &'a Item)>,
    grid: impl IntoIterator<Item = (&'a ItemAttribute, &'a Item)>,
    place: RepairPlace,
) -> i32 {
    let mut total = 0i32;
    for (attr, item) in equipment {
        if !is_repair_ban(item) {
            total = total.wrapping_add(repair_cost(attr, item, place).unwrap_or(0));
        }
    }
    for (attr, item) in grid {
        if !repair_all_skips(item.kind()) {
            total = total.wrapping_add(repair_cost(attr, item, place).unwrap_or(0));
        }
    }
    total
}

/// `IsPartChargeItem` (ZzzInventory.cpp:7884-7969): the cash shop items.
pub fn is_part_charge_item(t: usize) -> bool {
    let h = |i| k(HELPER, i);
    let p = |i| k(POTION, i);
    let in_h = |a, b| (h(a)..=h(b)).contains(&t);
    let in_p = |a, b| (p(a)..=p(b)).contains(&t);
    in_h(46, 48)
        || t == p(54)
        || in_p(58, 62)
        || in_p(145, 150)
        || in_h(125, 127)
        || t == p(53)
        || in_h(43, 45)
        || in_p(70, 77)
        || t == h(59)
        || in_h(54, 58)
        || in_p(78, 82)
        || in_h(60, 61)
        || in_p(91, 98)
        || in_h(62, 65)
        || in_h(69, 70)
        || [h(81), h(82), h(93), h(94), h(107), h(104), h(105), h(103), p(133), h(116), h(106), h(124), h(121), p(140)].contains(&t)
        || in_h(109, 115)
        || [p(112), p(113), p(120), p(123), p(124)].contains(&t)
        || in_p(134, 139)
        || (k(WING, 130)..=k(WING, 134)).contains(&t)
        || in_p(114, 119)
        || in_p(126, 132)
}

/// `IsSellingBan` (ZzzInventory.cpp:8363-8439) without period items: these cannot be sold to an NPC
/// (and show no price).
pub fn is_selling_ban(item: &Item) -> bool {
    let t = item.kind();
    let level = item.level;
    t == k(POTION, 11)
        || (t == k(POTION, 21) && level == 1)
        || (t == k(HELPER, 20) && (level == 1 || level == 2))
        || t == k(HELPER, 19)
        || (t == k(POTION, 20) && (1..=5).contains(&level))
        || is_part_charge_item(t)
        || wing_charm(t)
        || [k(HELPER, 80), k(HELPER, 76), k(HELPER, 122), k(HELPER, 123), k(HELPER, 106), k(HELPER, 107)].contains(&t)
        || item_action_ban(t, true)
}

/// `IsRepairBan` (ZzzInventory.cpp:8443-8513). (Its `MODEL_HELPER+43` to `+45` compare an item type
/// with a model number and never match; those are cash shop items anyway.)
pub fn is_repair_ban(item: &Item) -> bool {
    let t = item.kind();
    let h = |i| k(HELPER, i);
    // `g_ChangeRingMgr->CheckRepair` (ChangeRingManager.cpp:162-203): the transformation rings.
    [h(10), h(39), h(40), h(41), h(42), h(68), h(76), h(122)].contains(&t)
        || is_part_charge_item(t)
        || wing_charm(t)
        || (h(0)..=h(5)).contains(&t)
        || t == k(BOW, 7)
        || t == k(BOW, 15)
        || t >= k(POTION, 0)
        || (k(WING, 7)..=k(WING, 19)).contains(&t)
        || (h(14)..=h(19)).contains(&t)
        || [h(38), h(67), h(80), h(123), h(106), h(7), h(11), h(20), h(29), h(66)].contains(&t)
        || (h(32)..=h(37)).contains(&t)
        || (h(49)..=h(51)).contains(&t)
        || (h(71)..=h(75)).contains(&t)
        || item_action_ban(t, false)
}

/// `IsHighValueItem` (ZzzInventory.cpp:7971-8065) without period items: selling one asks first.
pub fn is_high_value_item(item: &Item) -> bool {
    let t = item.kind();
    let h = |i| k(HELPER, i);
    let w = |i| k(WING, i);
    let p = |i| k(POTION, i);
    let listed = [h(3), p(13), p(14), p(16), p(22), w(15), p(31), w(30), w(31), h(4), h(5), h(30)].contains(&t)
        || (w(0)..=w(6)).contains(&t)
        || (w(36)..=w(43)).contains(&t)
        || item.is_ancient()
        || [k(SWORD, 19), k(STAFF, 10), k(BOW, 18), k(MACE, 13), h(14), h(15), h(19), h(31)].contains(&t)
        || (p(41)..=p(44)).contains(&t)
        || (item.level > 6 && t < w(0))
        || item.excellent & 63 > 0
        || (h(34)..=h(37)).contains(&t)
        || [h(52), h(53), p(121), p(122), h(80), h(76), h(122), h(123), h(64), h(65), p(112), p(113)].contains(&t)
        || (w(130)..=w(134)).contains(&t)
        || (h(109)..=h(115)).contains(&t)
        || (t == h(20) && item.level == 0)
        || is_compiled_gem(item);
    // Without a period, these six are not (8047-8060).
    listed && ![h(80), h(76), h(64), h(65), h(122), h(123)].contains(&t)
}

/// `ConvertGold` (ZzzInventory.cpp:1059-1088): thousands separated by commas. The original takes a
/// `double` and casts it to `DWORD`, so a negative `int` wraps.
pub fn convert_gold(gold: i64) -> String {
    let s = (gold as u32).to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `ConvertTaxGold` (ZzzInventory.cpp:1112-1128): the price with the shop's tax (`DWORD` math).
pub fn tax_gold(gold: i32, tax_rate: i32) -> String {
    let g = gold as u32;
    let taxed = g.wrapping_add(((g as i64 * tax_rate as i64) / 100) as u32);
    convert_gold(taxed as i64)
}

/// The item name color of `RenderItemInfo` (ZzzInventory.cpp:2103-2187), which the price line uses:
/// [`name_color`] plus the jewels (`COMGEM::Check_Jewel`, CComGem.cpp:314-352) and the other
/// yellow items it lists.
fn info_color(item: &Item, stats: &ItemStats) -> TextColor {
    let t = item.kind();
    let jewel = [k(POTION, 13), k(POTION, 14), k(POTION, 16), k(POTION, 22), k(POTION, 31), k(WING, 15)].contains(&t)
        || (k(POTION, 41)..=k(POTION, 44)).contains(&t)
        || is_compiled_gem(item);
    let yellow = jewel
        || (k(POTION, 65)..=k(POTION, 68)).contains(&t)
        || [k(HELPER, 52), k(HELPER, 53), k(POTION, 100), k(POTION, 160), k(POTION, 161)].contains(&t)
        || (k(POTION, 141)..=k(POTION, 144)).contains(&t)
        || (k(HELPER, 135)..=k(HELPER, 145)).contains(&t);
    if yellow { TextColor::Yellow } else { name_color(item, stats) }
}

/// The line `RenderItemInfo` puts first while the NPC shop is open (ZzzInventory.cpp:2189-2213), or
/// `None` if the item cannot be sold there. `shop_item`: it is one of the shop's (`Sell = true`,
/// `GlobalText[1620]` with the taxed and the plain buying price); otherwise an inventory item with its
/// selling price (`GlobalText[63]`). English texts from MuMain's Localization/Game.en.resx:4776 and 184.
pub fn price_line(attr: &ItemAttribute, item: &Item, shop_item: bool, tax_rate: i32) -> Option<TipLine> {
    if is_selling_ban(item) {
        return None;
    }
    let text = if shop_item {
        let value = item_value(attr, item, GoldType::Buy)?;
        format!("Purchasing price: {}({})", tax_gold(value, tax_rate), convert_gold(value as i64))
    } else {
        format!("Selling Price: {}", convert_gold(item_value(attr, item, GoldType::Sell)? as i64))
    };
    let stats = convert(attr, item, 1);
    Some(TipLine { text, color: info_color(item, &stats), bold: false })
}

/// Inserts [`price_line`] where `RenderItemInfo` puts it: after the first empty line, followed by
/// another one.
pub fn add_price_line(lines: &mut Vec<TipLine>, price: TipLine) {
    let at = 1.min(lines.len());
    lines.insert(at, price);
    lines.insert(at + 1, TipLine { text: "\n".into(), color: TextColor::White, bold: false });
}

/// `RenderRepairInfo` (ZzzInventory.cpp:5642-5948): the lines of the tooltip in repair mode and
/// whether the item needs repairing (`RepairEnable` 2, otherwise 1). `None` for items that cannot be
/// repaired (no tooltip; its other early returns are all within [`is_repair_ban`], or compare model
/// numbers and never match).
pub fn repair_info(attr: &ItemAttribute, item: &Item, hero: &HeroView, place: RepairPlace) -> Option<(Vec<TipLine>, bool)> {
    if is_repair_ban(item) {
        return None;
    }
    let t = item.kind();
    let level = item.level;
    let stats = convert(attr, item, hero.level);
    let specials = stats.special_count();
    let excellent = item.excellent & 63 > 0;
    // 5772-5806.
    let mut color = if [k(POTION, 13), k(POTION, 14), k(WING, 15)].contains(&t) || is_compiled_gem(item) {
        TextColor::Yellow
    } else if [k(STAFF, 10), k(SWORD, 19), k(BOW, 18), k(MACE, 13)].contains(&t) {
        TextColor::Purple
    } else if (k(POTION, 17)..=k(POTION, 19)).contains(&t) {
        TextColor::Yellow
    } else if specials > 0 && excellent {
        TextColor::Green
    } else if level >= 7 {
        TextColor::Yellow
    } else if specials > 0 {
        TextColor::Blue
    } else {
        TextColor::White
    };
    // 5808-5832: the original compares `>= ITEM_HELPER+30` here, so every helper from the cape on.
    let wing_like = (k(WING, 3)..=k(WING, 6)).contains(&t)
        || t >= k(HELPER, 30)
        || (k(WING, 36)..=k(WING, 40)).contains(&t)
        || (k(WING, 42)..=k(WING, 43)).contains(&t);
    if wing_like {
        color = if level >= 7 {
            TextColor::Yellow
        } else if specials > 0 {
            TextColor::Blue
        } else {
            TextColor::White
        };
    }
    let gap = || TipLine { text: "\n".into(), color: TextColor::White, bold: false };
    let max = max_durability(attr, item) as i32;
    let needs = (item.durability as i32) < max;
    // `GlobalText[238]` "Repairing cost: %s" (MuMain Game.en.resx:664).
    let cost = if needs {
        let base = item_value(attr, item, GoldType::Repair)?;
        convert_gold(convert_repair_gold(base, item.durability as i32, max, t, place) as i64)
    } else {
        "0".to_string()
    };
    let mut lines = vec![gap(), TipLine { text: format!("Repairing cost: {cost}"), color, bold: true }, gap()];
    // The name (5868-5917), with `GlobalText[620]` "Excellent" before it.
    let name = match (wing_like || !excellent, level) {
        (true, 0) => attr.name.clone(),
        (true, l) => format!("{} +{l}", attr.name),
        (false, 0) => format!("Excellent {}", attr.name),
        (false, l) => format!("Excellent {} +{l}", attr.name),
    };
    lines.push(TipLine { text: name, color, bold: true });
    lines.push(gap());
    // `GlobalText[71]` "Durability: [%d/%d]".
    lines.push(TipLine { text: format!("Durability: [{}/{max}]", item.durability), color: TextColor::White, bold: false });
    lines.push(gap());
    Some((lines, needs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-handed sword of item level 6 and durability 22 (a Short Sword-like record).
    fn sword() -> ItemAttribute {
        ItemAttribute { name: "Sword".into(), level: 6, durability: 22, slot: Some(0), ..Default::default() }
    }

    fn it(group: u8, index: u16, level: u8, durability: u8) -> Item {
        Item { level, durability, ..Item::new(group, index) }
    }

    #[test]
    fn weapon_prices() {
        let a = sword();
        let s = it(SWORD, 1, 0, 22);
        // Level2 = 6: 100 + 46 · 36 / 8 = 307, one-handed · 80% = 245.
        assert_eq!(item_value(&a, &s, GoldType::Buy), Some(240));
        assert_eq!(item_value(&a, &s, GoldType::Sell), Some(81));
        // Repair base: rounded before dividing (245 → 240 / 3).
        assert_eq!(item_value(&a, &s, GoldType::Repair), Some(80));
        // Half durability: 81 - (DWORD)(81 · 0.6 · 0.5) = 57.
        assert_eq!(item_value(&a, &it(SWORD, 1, 0, 11), GoldType::Sell), Some(57));
        // Excellent (Excellent Damage): Level2 + 25 = 31, 100 + 71 · 961 / 8 = 8628, · 80% = 6902, doubled.
        let exc = Item { excellent: 0b10_0000, ..s };
        assert_eq!(item_value(&a, &exc, GoldType::Buy), Some(13_800));
        // Luck: 245 + 25% = 306.
        assert_eq!(item_value(&a, &Item { luck: true, ..s }, GoldType::Buy), Some(300));
        // Two-handed: no 80%.
        let two = ItemAttribute { two_hand: true, ..sword() };
        assert_eq!(item_value(&two, &s, GoldType::Buy), Some(300));
        // +7: Level2 = 6 + 21 + 25 = 52, 100 + 92 · 2704 / 8 = 31196, · 80% = 24956.
        assert_eq!(item_value(&a, &it(SWORD, 1, 7, 22), GoldType::Buy), Some(24_900));
    }

    #[test]
    fn fixed_prices() {
        let none = ItemAttribute::default();
        let bless = it(POTION, 13, 0, 1);
        assert_eq!(item_value(&none, &bless, GoldType::Buy), Some(9_000_000));
        assert_eq!(item_value(&none, &bless, GoldType::Sell), Some(3_000_000));
        // A bundle of 20 blessings (`ITEM_WING+30` +1).
        assert_eq!(item_value(&none, &it(WING, 30, 1, 0), GoldType::Buy), Some(180_000_000));
        // `iZen`: fixed, a third when selling.
        let zen = ItemAttribute { zen: 1000, ..Default::default() };
        assert_eq!(item_value(&zen, &it(POTION, 0, 0, 1), GoldType::Buy), Some(1000));
        assert_eq!(item_value(&zen, &it(POTION, 0, 0, 1), GoldType::Sell), Some(330));
        // Apples (`Value` 5): 5 · 5 · 10 / 12 = 20 per piece.
        let apple = ItemAttribute { value: 5, ..Default::default() };
        assert_eq!(item_value(&apple, &it(POTION, 0, 0, 3), GoldType::Buy), Some(60));
        assert_eq!(item_value(&apple, &it(POTION, 0, 0, 3), GoldType::Sell), Some(20));
        // Pets need their pet data.
        assert_eq!(item_value(&none, &it(HELPER, 4, 0, 255), GoldType::Buy), None);
        // Lucky items are worth nothing.
        assert_eq!(item_value(&sword(), &it(ARMOR, 62, 0, 22), GoldType::Sell), Some(0));
    }

    #[test]
    fn repair_prices() {
        let a = sword();
        // Base 80, half durability: 3 · √80 · ⁴√80 · 0.5 + 1 = 41.12 → 41 at the NPC.
        assert_eq!(convert_repair_gold(80, 11, 22, k(SWORD, 1), RepairPlace::NpcShop), 41);
        // From the inventory: · 2.5 = 102.8 → 100 (OpenMU: `repairPrice *= 2.5f` without an NPC).
        assert_eq!(convert_repair_gold(80, 11, 22, k(SWORD, 1), RepairPlace::SelfRepair), 100);
        // Broken: 81.25 + 40% = 113.7 → 110.
        assert_eq!(convert_repair_gold(80, 0, 22, k(SWORD, 1), RepairPlace::NpcShop), 110);
        assert_eq!(convert_repair_gold(80, 22, 22, k(SWORD, 1), RepairPlace::NpcShop), 0);
        assert_eq!(repair_cost(&a, &it(SWORD, 1, 0, 11), RepairPlace::NpcShop), Some(41));
        assert_eq!(repair_cost(&a, &it(SWORD, 1, 0, 22), RepairPlace::NpcShop), None);
        let worn = it(SWORD, 1, 0, 11);
        let potion = it(POTION, 0, 0, 1);
        let none = ItemAttribute::default();
        assert_eq!(repair_all_gold([(&a, &worn)], [(&a, &worn), (&none, &potion)], RepairPlace::NpcShop), 82);
        let (lines, needs) = repair_info(&a, &worn, &HeroView::default(), RepairPlace::NpcShop).unwrap();
        assert!(needs);
        assert_eq!(lines[1].text, "Repairing cost: 41");
        assert_eq!(lines[3].text, "Sword");
        assert_eq!(lines[5].text, "Durability: [11/22]");
        assert!(repair_info(&none, &potion, &HeroView::default(), RepairPlace::NpcShop).is_none());
    }

    #[test]
    fn bans_and_lines() {
        assert!(is_selling_ban(&it(HELPER, 19, 0, 0)));
        assert!(is_selling_ban(&it(HELPER, 135, 0, 0)), "lucky ticket");
        assert!(!is_selling_ban(&it(ARMOR, 62, 0, 0)), "lucky armor can be sold");
        assert!(is_repair_ban(&it(ARMOR, 62, 0, 0)), "but not repaired");
        assert!(is_repair_ban(&it(HELPER, 10, 0, 0)), "transformation ring");
        assert!(!is_repair_ban(&it(SWORD, 1, 0, 0)));
        assert!(is_high_value_item(&it(SWORD, 1, 7, 0)));
        assert!(!is_high_value_item(&it(SWORD, 1, 6, 0)));
        assert!(is_high_value_item(&Item { excellent: 1, ..it(SWORD, 1, 0, 0) }));
        assert_eq!(convert_gold(1_234_567), "1,234,567");
        assert_eq!(tax_gold(1000, 3), "1,030");
        let a = sword();
        let s = it(SWORD, 1, 0, 22);
        assert_eq!(price_line(&a, &s, true, 3).unwrap().text, "Purchasing price: 247(240)");
        assert_eq!(price_line(&a, &s, false, 0).unwrap().text, "Selling Price: 81");
        let mut lines = vec![TipLine { text: "\n".into(), color: TextColor::White, bold: false }];
        add_price_line(&mut lines, price_line(&a, &s, false, 0).unwrap());
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["\n", "Selling Price: 81", "\n"]);
    }
}
