//! The lines of an item's info box (`RenderItemInfo`, ZzzInventory.cpp:2053-5640),
//! with the texts from `Text_eng.bmd` (`GlobalText[n]`, the number beside each) and the colors of
//! `RenderTipTextList` (ZzzInventory.cpp:285-453).
//!
//! Included: the name (with its color: excellent, +7, set...), damage, defense, speed, durability, the
//! requirements (in red with what is lacking), the classes that can wear it and the options (skill,
//! luck, additional option and excellent ones). Missing: the lines of special items (pets,
//! potions with their effect, chests...), ancient sets (`RenderSetOptionListInItem`), harmony and
//! sockets.

use super::group::*;
use super::{Item, ItemAttribute, ItemStats, Special, kind};

/// `TEXT_COLOR_*` (_define.h:226-239).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextColor {
    White,
    Blue,
    Red,
    Yellow,
    Green,
    DarkRed,
    Purple,
    DarkBlue,
    DarkYellow,
    GreenBlue,
    Gray,
    RedPurple,
    Violet,
    Orange,
}

impl TextColor {
    /// Text color (each color's `glColor3f`; the blue-green one uses `SetTextColor(0, 255, 0)`).
    pub fn rgb(self) -> [f32; 3] {
        match self {
            TextColor::White | TextColor::DarkRed | TextColor::DarkBlue | TextColor::DarkYellow => [1.0, 1.0, 1.0],
            TextColor::Blue => [0.5, 0.7, 1.0],
            TextColor::Gray => [0.4, 0.4, 0.4],
            TextColor::GreenBlue => [0.0, 1.0, 0.0],
            TextColor::Red => [1.0, 0.2, 0.1],
            TextColor::Yellow => [1.0, 0.8, 0.1],
            TextColor::Green => [0.1, 1.0, 0.5],
            TextColor::Purple => [1.0, 0.1, 1.0],
            TextColor::RedPurple => [0.8, 0.5, 0.8],
            TextColor::Violet => [0.7, 0.4, 1.0],
            TextColor::Orange => [0.9, 0.42, 0.04],
        }
    }

    /// Line background (`SetBgColor`), if any.
    pub fn background(self) -> Option<[u8; 3]> {
        match self {
            TextColor::DarkRed => Some([160, 0, 0]),
            TextColor::DarkBlue => Some([0, 0, 160]),
            TextColor::DarkYellow => Some([160, 102, 0]),
            TextColor::GreenBlue => Some([60, 60, 200]),
            _ => None,
        }
    }
}

/// One line; `"\n"` is an empty half-height line.
#[derive(Debug, Clone, PartialEq)]
pub struct TipLine {
    pub text: String,
    pub color: TextColor,
    pub bold: bool,
}

impl TipLine {
    fn new(text: impl Into<String>, color: TextColor) -> Self {
        Self { text: text.into(), color, bold: false }
    }

    fn gap() -> Self {
        Self::new("\n", TextColor::White)
    }

    pub fn is_gap(&self) -> bool {
        self.text == "\n"
    }
}

/// What the character looking at the box has (`Hero->Class`, `CharacterAttribute`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HeroView {
    /// `GetBaseClass` (`CLASS_*`, 0 a 6).
    pub class: u8,
    /// `GetStepClass`: 1, 2 or 3.
    pub step: u8,
    pub level: u32,
    /// Strength, agility, vitality, energy and command (`None` without the server: not checked).
    pub stats: Option<[u32; 5]>,
}

/// Class names by class and tier (`RequireClass`, ZzzInventory.cpp:549-700):
/// `GlobalText[20-27, 1668-1672, 1687-1689]` and the Rage Fighter (from MuMain).
fn class_name(class: usize, step: u8) -> Option<&'static str> {
    Some(match (class, step) {
        (0, 1) => "Dark Wizard",
        (0, 2) => "Soul Master",
        (0, 3) => "Grand Master",
        (1, 1) => "Dark Knight",
        (1, 2) => "Blade Knight",
        (1, 3) => "Blade Master",
        (2, 1) => "Elf",
        (2, 2) => "Muse Elf",
        (2, 3) => "High Elf",
        (3, 1) => "Magic Gladiator",
        (3, 3) => "Dual Master",
        (4, 1) => "Dark Lord",
        (4, 3) => "Lord Emperor",
        (5, 1) => "Summoner",
        (5, 2) => "Bloody Summoner",
        (5, 3) => "Dimension Master",
        (6, 1) | (6, 3) => "Rage Fighter",
        _ => return None,
    })
}

/// `GetSpecialOptionText` (ZzzInventory.cpp:1823-2051). `mana` = the skill's mana.
fn special_text(o: Special, value: u32, mana: u32) -> Option<String> {
    Some(match o {
        // `AT_SKILL_*` (SkillManager.h:28-60).
        Special::Skill(s) => match s {
            18 => format!("Defend skill (Mana:{mana})"),          // 80
            19 => format!("Falling Slash skill (Mana:{mana})"),   // 81
            20 => format!("Lunge skill (Mana:{mana})"),           // 82
            21 => format!("Uppercut skill (Mana:{mana})"),        // 83
            22 => format!("Cyclone Cutting skill (Mana:{mana})"), // 84
            23 => format!("Slashing skill (Mana:{mana})"),        // 85
            24 => format!("Triple Shot skill (Mana:{mana})"),     // 86
            56 => format!("Power Slash Skill (Mana:{mana})"),     // 98
            60 => format!("Long spear skill (mana:{mana})"),      // 1210
            _ => return None,
        },
        Special::Luck => "Luck (success rate of Jewel of Soul +25%)".into(), // 87
        Special::ImproveDamage => format!("Additional Dmg +{value}"),       // 88
        Special::ImproveMagic => format!("Additional Wizardry Dmg +{value}"), // 89
        Special::ImproveCurse => format!("Additional Curse Spell +{value}"), // 1697
        Special::ImproveBlocking => format!("Additional defense rate +{value}"), // 90
        Special::ImproveDefense => format!("Additional defense +{value}"), // 91
        Special::LifeRegeneration => format!("Automatic HP recovery {value}%"), // 92
        Special::ImproveMaxMana => format!("Max mana increased by {value}%"), // 1087
        Special::ImproveMaxAg => format!("Max AG increased by {value}%"),   // 1088
        Special::ImproveLife => "Increase Max HP +4%".into(),               // 622
        Special::ImproveMana => "Increase Max Mana +4%".into(),             // 623
        Special::DecreaseDamage => "Damage Decrease +4%".into(),            // 624
        Special::ReflectionDamage => "Reflect Damage +5%".into(),           // 625
        Special::ImproveBlockingPercent => "Defense success rate +10%".into(), // 626
        Special::ImproveGainGold => "Increases acquisition rate of Zen after hunting monsters +30%".into(), // 627
        Special::ExcellentDamage => "Excellent Damage rate +10%".into(),    // 628
        Special::ImproveDamageLevel => "Increase Damage +level/20".into(),  // 629
        Special::ImproveDamagePercent => format!("Increase Damage +{value}%"), // 630
        Special::ImproveMagicLevel => "Increase Wizardry Dmg +level/20".into(), // 631
        Special::ImproveMagicPercent => format!("Increase Wizardry Dmg +{value}%"), // 632
        Special::ImproveAttackSpeed => format!("Increase Attacking(Wizardry)speed +{value}"), // 633
        Special::ImproveGainLife => "Increases acquisition rate of Life after hunting monsters +life/8".into(), // 634
        Special::ImproveGainMana => "Increases acquisition rate of Mana after hunting monsters +Mana/8".into(), // 635
        Special::ImproveHpMax => format!("HP +{value} increased"),          // 740
        Special::ImproveMpMax => format!("Mana +{value} increased"),        // 741
        Special::OnePercentDamage => format!("Ignor opponent's defensive power by {value}%"), // 742
        Special::ImproveAgMax => format!("Max AG +{value} increased"),      // 743
    })
}

/// Name color (ZzzInventory.cpp ≈2100-2187).
pub fn name_color(item: &Item, stats: &ItemStats) -> TextColor {
    let t = item.kind();
    let k = |g: u8, i: u16| kind(g, i);
    let level = item.level;
    let mut c = if [k(POTION, 13), k(POTION, 14), k(WING, 15), k(POTION, 31), k(POTION, 16), k(POTION, 22)].contains(&t)
        || (k(WING, 30)..=k(WING, 31)).contains(&t)
        || (k(POTION, 17)..=k(POTION, 19)).contains(&t)
        || (k(HELPER, 16)..=k(HELPER, 17)).contains(&t)
    {
        TextColor::Yellow
    } else if [k(STAFF, 10), k(SWORD, 19), k(BOW, 18), k(MACE, 13)].contains(&t) {
        TextColor::Purple
    } else if item.is_ancient() {
        TextColor::GreenBlue
    } else if stats.special_count() > 0 && item.excellent & 63 > 0 {
        TextColor::Green
    } else if level >= 7 {
        TextColor::Yellow
    } else if stats.special_count() > 0 {
        TextColor::Blue
    } else {
        TextColor::White
    };
    let no_excellent_name = (k(WING, 3)..=k(WING, 6)).contains(&t)
        || t == k(HELPER, 30)
        || (k(WING, 36)..=k(WING, 40)).contains(&t)
        || (k(WING, 42)..=k(WING, 43)).contains(&t);
    if no_excellent_name {
        c = if level >= 7 {
            TextColor::Yellow
        } else if stats.special_count() > 0 {
            TextColor::Blue
        } else {
            TextColor::White
        };
    }
    c
}

/// The box lines (`RenderItemInfo` with `Sell = false`). `skill_mana(i)` = `SkillAttribute[i].Mana`;
/// without it (MuMain's data lacks the S6 `skill.bmd`) the skill line is not shown.
pub fn item_info(attr: &ItemAttribute, item: &Item, hero: &HeroView, skill_mana: &dyn Fn(u16) -> Option<u32>) -> Vec<TipLine> {
    let stats = super::convert(attr, item, hero.level);
    let t = item.kind();
    let k = |g: u8, i: u16| kind(g, i);
    let g = item.group;
    let excellent = item.excellent & 63 > 0;
    let mut out = vec![TipLine::gap()];

    // Name: `GlobalText[620]` "Excellent" before and `+level` after.
    let mut name = attr.name.clone();
    if excellent {
        name = format!("Excellent {name}");
    }
    if item.level > 0 {
        name = format!("{name} +{}", item.level);
    }
    out.push(TipLine { text: name, color: name_color(item, &stats), bold: true });
    out.push(TipLine::gap());

    // Damage (`GlobalText[40 + TwoHand]`), in blue if excellent.
    if stats.damage_min > 0 && g != ETC {
        let (lo, hi) = (stats.damage_min.min(stats.damage_max), stats.damage_max);
        let label = if attr.two_hand { "Two-Handed Damage" } else { "One-Handed Damage" };
        out.push(TipLine::new(format!("{label}: {lo} ~ {hi}"), if excellent { TextColor::Blue } else { TextColor::White }));
    }
    if stats.defense > 0 {
        let blue = (HELM..=BOOTS).contains(&g) && excellent;
        out.push(TipLine::new(format!("Defense: {}", stats.defense), if blue { TextColor::Blue } else { TextColor::White })); // 65
    }
    if stats.magic_defense > 0 {
        out.push(TipLine::new(format!("Spell resistance: {}", stats.magic_defense), TextColor::White)); // 66
    }
    if attr.blocking > 0 {
        let c = if excellent { TextColor::Blue } else { TextColor::White };
        out.push(TipLine::new(format!("Defense rate: {}", stats.blocking), c)); // 67
    }
    if attr.weapon_speed > 0 {
        out.push(TipLine::new(format!("Attack speed: {}", attr.weapon_speed), TextColor::White)); // 64
    }
    if attr.walk_speed > 0 {
        out.push(TipLine::new(format!("Moving speed: {}", attr.walk_speed), TextColor::White)); // 68
    }

    // Durability (`bDurExist`, ZzzInventory.cpp ≈4538-4750).
    let dur_exist = ((attr.durability > 0 || attr.magic_durability > 0) && ((t < k(WING, 0) || t >= k(HELPER, 0)) && t < k(POTION, 0)))
        || (k(WING, 0)..=k(WING, 6)).contains(&t)
        || (k(WING, 36)..=k(WING, 43)).contains(&t);
    let shown = (dur_exist || item.durability > 0)
        && (t < k(HELPER, 14) || t > k(HELPER, 19))
        && !(t == k(HELPER, 20) && (item.level == 1 || item.level == 2))
        && ![k(HELPER, 29), k(POTION, 7), k(HELPER, 7), k(HELPER, 11), k(HELPER, 35)].contains(&t);
    if shown {
        if (k(POTION, 0)..=k(POTION, 8)).contains(&t) {
            out.push(TipLine::new(format!("Number of items: {}", item.durability), TextColor::White)); // 69
        } else if dur_exist {
            let max = super::max_durability(attr, item);
            out.push(TipLine::new(format!("Durability: [{}/{max}]", item.durability), TextColor::White)); // 71
        }
    }

    // Requirements: in red with `GlobalText[74]` "(lacking %d)" if not met.
    let mut require = |text: String, need: u32, have: Option<u32>| {
        match have.filter(|&h| h < need) {
            Some(h) => {
                out.push(TipLine::new(text, TextColor::Red));
                out.push(TipLine::new(format!("(lacking {})", need - h), TextColor::Red));
            }
            None => out.push(TipLine::new(text, TextColor::White)),
        }
    };
    let stat = |i: usize| hero.stats.map(|s| s[i]);
    if stats.require_level > 0 && t != k(HELPER, 14) {
        require(format!("Minimum Level Requirement: {}", stats.require_level), stats.require_level, Some(hero.level)); // 76
    }
    if stats.require_strength > 0 {
        require(format!("Strength Requirement: {}", stats.require_strength), stats.require_strength, stat(0)); // 73
    }
    if stats.require_dexterity > 0 {
        require(format!("Agility Requirement: {}", stats.require_dexterity), stats.require_dexterity, stat(1)); // 75
    }
    if stats.require_vitality > 0 {
        require(format!("Usable HP: {}", stats.require_vitality), stats.require_vitality, stat(2)); // 1930
    }
    if stats.require_energy > 0 {
        require(format!("Available Energy: {}", stats.require_energy), stats.require_energy, stat(3)); // 77
    }
    if stats.require_charisma > 0 {
        require(format!("Available command: {}", stats.require_charisma), stats.require_charisma, stat(4)); // 698
    }

    // `RequireClass`: "Can be equipped by %s" (61), own class in white and the rest in dark red;
    // nothing if every class can wear it. (`IsRequireClassRenderItem` excludes a few special items.)
    let special_no_class = [k(HELPER, 19), k(HELPER, 29), k(WING, 26), k(HELPER, 10), k(HELPER, 37), k(WING, 15), k(WING, 32), k(WING, 33), k(WING, 34)]
        .contains(&t)
        || (k(WING, 30)..=k(WING, 31)).contains(&t)
        || (k(HELPER, 39)..=k(HELPER, 48)).contains(&t);
    if !special_no_class {
        out.push(TipLine::gap());
        let all = attr.require_class.iter().filter(|&&c| c == 1).count() == attr.require_class.len();
        if !all {
            for (c, &req) in attr.require_class.iter().enumerate() {
                let Some(name) = (req > 0).then(|| class_name(c, req)).flatten() else { continue };
                let own = c == hero.class as usize && req <= hero.step.max(1);
                out.push(TipLine::new(format!("Can be equipped by {name}"), if own { TextColor::White } else { TextColor::DarkRed }));
            }
        }
    }

    // Options (blue), and after luck its second line (94).
    for (o, v) in &stats.specials {
        let mana = match o {
            Special::Skill(s) => match skill_mana(*s) {
                Some(m) => m,
                None => continue,
            },
            _ => 0,
        };
        if let Some(text) = special_text(*o, *v, mana) {
            out.push(TipLine::new(text, TextColor::Blue));
        }
        if *o == Special::Luck {
            out.push(TipLine::new("Luck (critical damage rate +5%)", TextColor::Blue));
        }
    }
    out.push(TipLine::gap());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kris() -> ItemAttribute {
        ItemAttribute {
            name: "Kris".into(),
            level: 6,
            slot: Some(0),
            width: 1,
            height: 2,
            damage_min: 6,
            damage_max: 11,
            weapon_speed: 50,
            durability: 20,
            require_strength: 40,
            require_dexterity: 40,
            require_class: [1; 7],
            ..Default::default()
        }
    }

    #[test]
    fn cuadro_de_un_arma() {
        let item = Item { level: 7, luck: true, option: 1, durability: 20, excellent: 0b100000, ..Item::new(SWORD, 0) };
        let hero = HeroView { class: 1, step: 1, level: 10, stats: Some([30, 100, 20, 10, 0]) };
        let lines = item_info(&kris(), &item, &hero, &|_| Some(0));
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts[1], "Excellent Kris +7");
        // Excellent with options: green, bold.
        assert_eq!((lines[1].color, lines[1].bold), (TextColor::Green, true));
        assert!(texts.contains(&"One-Handed Damage: 57 ~ 62"), "{texts:?}");
        assert!(texts.contains(&"Attack speed: 50"));
        assert!(texts.contains(&"Durability: [20/45]"), "{texts:?}");
        // Lacking strength: two lines in red.
        let s = texts.iter().position(|t| t.starts_with("Strength Requirement")).unwrap();
        assert_eq!(lines[s].color, TextColor::Red);
        assert!(texts[s + 1].starts_with("(lacking "));
        // All classes: no class lines.
        assert!(!texts.iter().any(|t| t.starts_with("Can be equipped")));
        assert!(texts.contains(&"Luck (success rate of Jewel of Soul +25%)"));
        assert!(texts.contains(&"Additional Dmg +4"));
        assert!(texts.contains(&"Excellent Damage rate +10%"));
        assert!(lines.last().unwrap().is_gap());
    }

    #[test]
    fn colores_del_nombre() {
        let attr = kris();
        let stats = |it: &Item| super::super::convert(&attr, it, 1);
        let plain = Item::new(SWORD, 0);
        assert_eq!(name_color(&plain, &stats(&plain)), TextColor::White);
        let lucky = Item { luck: true, ..plain };
        assert_eq!(name_color(&lucky, &stats(&lucky)), TextColor::Blue);
        let seven = Item { level: 7, ..plain };
        assert_eq!(name_color(&seven, &stats(&seven)), TextColor::Yellow);
        let set = Item { ancient: 5, ..plain };
        assert_eq!(name_color(&set, &stats(&set)), TextColor::GreenBlue);
        assert_eq!(name_color(&Item::new(POTION, 13), &ItemStats::default()), TextColor::Yellow);
    }
}
