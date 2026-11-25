//! The hero's skills (`CharacterAttribute->Skill[]`, `Hero->CurrentSkill`, `SkillDelay[]`) and the skill
//! bar of the main frame (`CNewUISkillList`, NewUIMainFrameWindow.cpp:1301-2577):
//! - the list from the server (`ReceiveMagicList`, WSclient.cpp:1170-1262), kept in
//!   `FxWorld::hero_skills` for the effects that look skills up by slot;
//! - the current skill (icon at (392, 437)), the five hotkey boxes (222..382, 431) showing keys 1-5
//!   or 6-0, the list that opens by clicking the current skill, keys `1`-`0` (`UseHotKey`) and
//!   Ctrl + number over a skill of the list (`SetHotKey`);
//! - the cooldowns (`CheckSkillDelay` / `CalcSkillDelay`, SkillManager.cpp:126-170, in real
//!   milliseconds as the original's `difTime`) and their red cover (`RenderSkillDelay`);
//! - the tooltip (`RenderSkillInfo`, ZzzInventory.cpp:6042-6488), with the parts that need data the
//!   client has.
//!
//! Not yet: the Dark Lord's pet commands (`AT_PET_COMMAND_*`, no Dark Raven), saving the hotkeys
//! on the server (`F3 30` option data), and the item-option parts of the "can't use" grey icon and
//! of the tooltip damage (`g_csItemOption`, mastery and set options).

use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_data::skills::{HeroStats, SkillDef, SkillTable, use_type};
use mu_protocol::combat::{SkillSlot, skill_list};
use mu_protocol::game::ServerMessage;

use crate::Args;
use crate::account::Class;
use crate::scene::GameScene;

/// `MAX_SKILLS` of Main 5.2 (_define.h:386): the size of `CharacterAttribute->Skill[]`.
pub const MAX_SKILLS: usize = 600;
/// `MAX_MAGIC` (_define.h:507): the slots the skill list window walks.
pub const MAX_MAGIC: usize = 64;
/// `SKILLHOTKEY_COUNT` (NewUIMainFrameWindow.h:76).
pub const HOTKEYS: usize = 10;

/// Skill numbers the bar needs that the effects tables do not name (SkillManager.h:15-202).
pub mod skill {
    pub const POWER_SLASH_UP: u16 = 505;
    pub const MANY_ARROW_UP: u16 = 490;
    pub const ICE_BLADE: u16 = 56;
    pub const CROSSBOW: u16 = 24;
    pub const TELEPORT: u16 = 6;
    pub const TELEPORT_B: u16 = 15;
    pub const BLOW_OF_DESTRUCTION: u16 = 232;
    pub const SWELL_OF_MAGICPOWER: u16 = 233;
    pub const RECOVER: u16 = 234;
    pub const MULTI_SHOT: u16 = 235;
    pub const FLAME_STRIKE: u16 = 236;
    pub const GIGANTIC_STORM: u16 = 237;
    pub const GAOTIC: u16 = 238;
    pub const LIGHTNING_SHOCK: u16 = 230;
    pub const LIGHTNING_SHOCK_UP: u16 = 545;
    pub const PLASMA_STORM_FENRIR: u16 = 76;
    pub const INFINITY_ARROW: u16 = 77;
    pub const STUN: u16 = 67;
    pub const REMOVAL_BUFF: u16 = 72;
    pub const ALICE_DRAINLIFE: u16 = 214;
    pub const ALICE_THORNS: u16 = 217;
    pub const ALICE_BERSERKER: u16 = 218;
    pub const ALICE_SLEEP: u16 = 219;
    pub const ALICE_BLIND: u16 = 220;
    pub const ALICE_WEAKNESS: u16 = 221;
    pub const ALICE_ENERVATION: u16 = 222;
    pub const SUMMON_EXPLOSION: u16 = 223;
    pub const SUMMON_REQUIEM: u16 = 224;
    pub const SUMMON_POLLUTION: u16 = 225;
    pub const WHEEL: u16 = 41;
    pub const TORNADO_SWORDA_UP: u16 = 455;
    pub const TORNADO_SWORDB_UP: u16 = 495;
    pub const REDUCEDEFENSE: u16 = 55;
    pub const BLOOD_ATT_UP: u16 = 500;
    /// `AT_PET_COMMAND_DEFAULT` (_enum.h:1491).
    pub const PET_COMMAND_DEFAULT: u16 = 120;
}

/// The bar's mouse state (`EVENT_STATE` of `CNewUISkillList`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Event {
    #[default]
    None,
    HoverCurrent,
    DownCurrent,
    HoverHotkey,
    DownHotkey,
    HoverList,
    DownList,
}

/// `CharacterAttribute->Skill[]` and friends, plus the bar's state.
#[derive(Resource)]
pub struct HeroSkills {
    /// Skill number by slot (0 = empty).
    pub slots: Vec<u16>,
    pub levels: Vec<u8>,
    /// `SkillDelay[slot]`: milliseconds left.
    pub delay: Vec<i32>,
    /// `SkillNumber` and `SkillMasterNumber`.
    pub count: usize,
    pub master_count: usize,
    /// `Hero->CurrentSkill`: a slot.
    pub current: usize,
    /// `m_wHeroPriorSkill`.
    pub prior: u16,
    /// `m_iHotKeySkillType`: the slot on each number key (-1 = none).
    pub hotkeys: [i32; HOTKEYS],
    list_open: bool,
    hotkey_up: bool,
    event: Event,
    /// `m_bRenderSkillInfo` with `m_iRenderSkillInfoType` and its position.
    info: Option<(usize, f32, f32)>,
}

impl Default for HeroSkills {
    fn default() -> Self {
        Self {
            slots: vec![0; MAX_SKILLS],
            levels: vec![0; MAX_SKILLS],
            delay: vec![0; MAX_SKILLS],
            count: 0,
            master_count: 0,
            current: 0,
            prior: 0,
            hotkeys: [-1; HOTKEYS],
            list_open: false,
            hotkey_up: false,
            event: Event::None,
            info: None,
        }
    }
}

/// `AT_SKILL_STUN..=AT_SKILL_REMOVAL_BUFF`: never shown in the bar.
fn is_castle_skill(s: u16) -> bool {
    (skill::STUN..=skill::REMOVAL_BUFF).contains(&s)
}

impl HeroSkills {
    /// The skill in `slot` (0 if none).
    pub fn skill(&self, slot: usize) -> u16 {
        self.slots.get(slot).copied().unwrap_or(0)
    }

    /// `CharacterAttribute->Skill[Hero->CurrentSkill]`.
    pub fn current_skill(&self) -> u16 {
        self.skill(self.current)
    }

    /// `ReceiveMagicList` (WSclient.cpp:1170-1262). `use_type(skill)` is `SkillAttribute[].SkillUseType`.
    pub fn apply(&mut self, msg: &ServerMessage, use_type_of: &dyn Fn(u16) -> u8) -> bool {
        let set = |s: &mut Self, e: &SkillSlot, v: u16| {
            if let Some(x) = s.slots.get_mut(e.index as usize) {
                *x = v;
                s.levels[e.index as usize] = e.level;
            }
        };
        match msg {
            ServerMessage::SkillRemoved { index, .. } => set(self, &SkillSlot { index: *index, ..default() }, 0),
            ServerMessage::SkillAdded(e) => set(self, e, e.number),
            ServerMessage::SkillList { list_type, skills } => match *list_type {
                skill_list::REMOVE_LISTED => {
                    // `Skill[Data2->Index] = 0` without advancing `Offset`: only the first entry,
                    // as many times as there are entries (WSclient.cpp:1188-1195).
                    if let Some(e) = skills.first() {
                        set(self, e, 0);
                    }
                }
                t => {
                    if t == skill_list::REPLACE {
                        self.slots.fill(0);
                    }
                    for e in skills {
                        set(self, e, e.number);
                    }
                    // TODO(original): the Dark Lord's pet commands go in `Skill[AT_PET_COMMAND_DEFAULT
                    // + i]` here (WSclient.cpp:1209-1215); no pets yet.
                }
            },
            _ => return false,
        }
        self.count = self.slots.iter().filter(|&&s| s != 0).count();
        self.master_count = self.slots.iter().filter(|&&s| s != 0 && use_type_of(s) == use_type::MASTER).count();
        if self.current >= self.count {
            self.current = 0;
        }
        if self.count == 1 {
            self.current = 0;
        }
        if self.skill(self.current) == 0 {
            self.current = 0;
        }
        // The current one is never a Castle Siege skill (WSclient.cpp:1238-1246).
        for _ in 0..MAX_SKILLS {
            if is_castle_skill(self.skill(self.current)) {
                self.current += 1;
            } else {
                break;
            }
        }
        // With Power Slash or Many Arrows mastered, Ice Blade / Triple Shot disappear (1247-1260).
        let (mut master, mut base) = (None, None);
        for (i, &s) in self.slots.iter().enumerate() {
            if (skill::POWER_SLASH_UP..=skill::POWER_SLASH_UP + 4).contains(&s) || (skill::MANY_ARROW_UP..=skill::MANY_ARROW_UP + 4).contains(&s) {
                master = Some(i);
            }
            if s == skill::ICE_BLADE || s == skill::CROSSBOW {
                base = Some(i);
            }
        }
        if let (Some(_), Some(b)) = (master, base) {
            self.slots[b] = 0;
        }
        true
    }

    /// `CNewUISkillList::SetHotKey` (NewUIMainFrameWindow.cpp:1830-1842).
    pub fn set_hotkey(&mut self, key: usize, slot: usize) {
        for h in self.hotkeys.iter_mut() {
            if *h == slot as i32 {
                *h = -1;
                break;
            }
        }
        self.hotkeys[key] = slot as i32;
    }

    /// `CNewUISkillList::UseHotKey` (1864-1900). The teleport + auto-attack part is not needed
    /// (no auto-attack option yet).
    pub fn use_hotkey(&mut self, key: usize) {
        let slot = self.hotkeys[key];
        if slot < 0 {
            return;
        }
        // TODO(original): pet commands need `Hero->m_pPet` (no pets yet).
        if self.skill(slot as usize) == 0 {
            return;
        }
        self.prior = self.current_skill();
        self.current = slot as usize;
    }

    /// `IsArrayIn` / `IsArrayUp` and `Update` (1797-1925): which half of the hotkeys the bar shows.
    fn update(&mut self) {
        if let Some(i) = self.hotkeys.iter().position(|&h| h == self.current as i32) {
            self.hotkey_up = i == 0 || i > 5;
        }
    }

    /// `SkillDelay[slot] > 0`.
    pub fn cooling(&self, slot: usize) -> bool {
        self.delay.get(slot).is_some_and(|&d| d > 0)
    }

    /// `CheckSkillDelay(SkillIndex)` (SkillManager.cpp:126-153): false while cooling or without the
    /// leadership; otherwise starts the cooldown.
    pub fn check_delay(&mut self, slot: usize, def: Option<&SkillDef>, charisma: u16) -> bool {
        let Some(d) = def else { return true };
        if d.delay > 0 {
            if self.cooling(slot) {
                return false;
            }
            if d.charisma > charisma {
                return false;
            }
            self.delay[slot] = d.delay;
        }
        true
    }

    /// `CalcSkillDelay(time)`: `SkillNumber + 2` slots.
    pub fn calc_delay(&mut self, ms: i32) {
        let n = (self.count + 2).min(MAX_SKILLS);
        for d in self.delay[..n].iter_mut() {
            if *d > 0 {
                *d = (*d - ms).max(0);
            }
        }
    }

    /// The bar's skill list is open (it blocks the mouse there).
    pub fn blocks(&self, c: Vec2, table: &SkillTable) -> bool {
        self.list_open && list_entries(self, table, true).iter().any(|&(_, x, y)| Rect::new(x, y, x + 32.0, y + 38.0).contains(c))
    }
}

/// Where the `k`-th skill of the list goes (`Render`, 2000-2034): around (385, 390), alternating
/// right and left, then further left, then a second row.
pub fn list_position(k: usize) -> (f32, f32) {
    let (orig, w, h) = (385.0, 32.0, 38.0);
    let y = if k >= 18 { 390.0 - h } else { 390.0 };
    let k = k as f32;
    let x = if k < 14.0 {
        let (q, r) = ((k / 2.0).floor(), k as usize % 2);
        if r == 0 { orig + q * w } else { orig - (q + 1.0) * w }
    } else if k < 18.0 {
        orig - 8.0 * w - (k - 14.0) * w
    } else {
        orig - 12.0 * w + (k - 17.0) * w
    };
    (x, y)
}

/// The slots the list shows with their box position. `render` = `Render`'s filter (no master
/// skills of either kind); otherwise `UpdateMouseEvent`'s, which only skips `SKILL_USE_TYPE_MASTERLEVEL`
/// (1636-1648), as the original does.
fn list_entries(s: &HeroSkills, table: &SkillTable, render: bool) -> Vec<(usize, f32, f32)> {
    let mut out = Vec::new();
    for i in 0..MAX_MAGIC {
        let t = s.skill(i);
        if t == 0 || is_castle_skill(t) {
            continue;
        }
        let u = table.get(t).map_or(0, |d| d.use_type);
        if u == use_type::MASTERLEVEL || (render && u == use_type::MASTER) {
            continue;
        }
        let (x, y) = list_position(out.len());
        out.push((i, x, y));
    }
    out
}

/// Which atlas an icon comes from (`KOS_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Atlas {
    Command,
    Skill1,
    Skill2,
}

/// `RenderSkillIcon`'s choice of atlas and cell (2275-2471) for a 20×28 icon: (atlas, column, row).
pub fn icon_cell(skill: u16, use_type: u8, magic_icon: u16) -> (Atlas, u32, u32) {
    use skill::*;
    let s = skill as u32;
    let two = |c: u32, r: u32| (Atlas::Skill2, c, r);
    match skill {
        _ if skill >= PET_COMMAND_DEFAULT && skill <= PET_COMMAND_DEFAULT + 8 && use_type != use_type::MASTERACTIVE && skill < 128 => {
            let k = s - PET_COMMAND_DEFAULT as u32;
            (Atlas::Command, k % 8, k / 8)
        }
        PLASMA_STORM_FENRIR => (Atlas::Command, 4, 0),
        ALICE_DRAINLIFE..=ALICE_THORNS => two((s - ALICE_DRAINLIFE as u32) % 8, 3),
        ALICE_SLEEP..=ALICE_BLIND => two((s - ALICE_SLEEP as u32 + 4) % 8, 3),
        ALICE_BERSERKER => two(10, 3),
        ALICE_WEAKNESS..=ALICE_ENERVATION => two(s - ALICE_WEAKNESS as u32 + 8, 3),
        SUMMON_EXPLOSION..=SUMMON_REQUIEM => two((s - SUMMON_EXPLOSION as u32 + 6) % 8, 3),
        SUMMON_POLLUTION => two(11, 3),
        BLOW_OF_DESTRUCTION => two(7, 2),
        GAOTIC => two(3, 8),
        RECOVER => two(9, 2),
        MULTI_SHOT => two(0, 8),
        FLAME_STRIKE => two(1, 8),
        GIGANTIC_STORM => two(2, 8),
        LIGHTNING_SHOCK => two(2, 3),
        LIGHTNING_SHOCK_UP..=549 => two(6, 8),
        SWELL_OF_MAGICPOWER => two(8, 2),
        _ if use_type == use_type::MASTERACTIVE => two(magic_icon as u32 % 12, magic_icon as u32 / 12 + 4),
        _ if skill >= 57 => two((s - 57) % 8, (s - 57) / 8),
        _ => (Atlas::Skill1, (s.saturating_sub(1)) % 8, (s.saturating_sub(1)) / 8),
    }
}

/// What decides the grey ("can't use") icon of `RenderSkillIcon` (2121-2273) that the client knows.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeroGear {
    pub class: Option<Class>,
    pub stats: HeroStats,
    /// `Equipment[EQUIPMENT_WEAPON_RIGHT/LEFT]` as (group, index).
    pub right: Option<(u8, u16)>,
    pub left: Option<(u8, u16)>,
}

impl HeroGear {
    /// `iTypeR != -1 && iTypeR is no staff && iTypeL is no staff` (2235-2245, 2398-2404).
    fn melee_weapon(&self) -> bool {
        use mu_data::items::group::STAFF;
        self.right.is_some_and(|(g, _)| g != STAFF) && self.left.is_none_or(|(g, _)| g != STAFF)
    }

    /// `GetEquipedBowType_Skill() != BOWTYPE_NONE`: a bow on the left or a crossbow on the right.
    fn bow(&self) -> bool {
        use mu_data::items::group::BOW;
        self.left.is_some_and(|(g, i)| g == BOW && i != 7) || self.right.is_some_and(|(g, i)| g == BOW && i >= 8 && i != 15)
    }
}

/// `bCantSkill` of `RenderSkillIcon` with the checks the client can make: `DemendConditionCheckSkill`,
/// Strength 596 for Blood Attack, Agility 646 for Ice Arrow, a melee weapon for Twisting Slash,
/// Blood Attack and Flame Strike, a bow for Multi-Shot. TODO(original): `IsCanBCSkill`, mounts
/// (`Helper`), item options (`g_csItemOption`), party, buffs.
pub fn cant_skill(skill: u16, table: &SkillTable, gear: &HeroGear) -> bool {
    use skill::*;
    let Some(def) = table.get(skill) else { return true };
    let base = table.get(base_skill(skill)).unwrap_or(def);
    if !base.demand_ok(&gear.stats) {
        return true;
    }
    if (skill == REDUCEDEFENSE || (BLOOD_ATT_UP..=BLOOD_ATT_UP + 4).contains(&skill)) && (gear.stats.strength < 596 || !gear.melee_weapon()) {
        return true;
    }
    // AT_SKILL_PARALYZE (51).
    if skill == 51 && gear.stats.dexterity < 646 {
        return true;
    }
    if (skill == WHEEL || (TORNADO_SWORDA_UP..=TORNADO_SWORDA_UP + 4).contains(&skill) || (TORNADO_SWORDB_UP..=TORNADO_SWORDB_UP + 4).contains(&skill)) && !gear.melee_weapon() {
        return true;
    }
    if skill == FLAME_STRIKE && !gear.melee_weapon() {
        return true;
    }
    if skill == MULTI_SHOT && !gear.bow() {
        return true;
    }
    false
}

/// `MasterSkillToBaseSkillIndex` (SkillManager.cpp:207-404) for the master skills that exist in S6.
pub fn base_skill(s: u16) -> u16 {
    const MAP: [(u16, u16); 24] = [
        (445, 9), (530, 9), (435, 16), (440, 10), (450, 39), (455, 41), (495, 41), (460, 43), (465, 42), (470, 48), (475, 26), (480, 27),
        (485, 28), (490, 24), (500, 49), (505, 56), (510, 13), (515, 62), (520, 61), (525, 78), (535, 219), (540, 215), (545, 216), (550, 214),
    ];
    MAP.iter().find(|(m, _)| (*m..*m + 5).contains(&s)).map_or(s, |&(_, b)| b)
}

// --- The tooltip (`RenderSkillInfo`) --------------------------------------------------------------

/// A tooltip line: text, colour (`TEXT_COLOR_*`), bold. `"\n"` is a half-height gap.
#[derive(Clone, Debug, PartialEq)]
pub struct InfoLine {
    pub text: String,
    pub color: [f32; 3],
    pub bold: bool,
    /// `TEXT_COLOR_DARKRED` lines have a background.
    pub bg: Option<[f32; 4]>,
}

const BLUE: [f32; 3] = [0.5, 0.7, 1.0];
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// What the tooltip shows of the hero's damages (`GetMagicSkillDamage`, `GetSkillDamage` +
/// `GetAttackDamage`, CharacterManager.cpp).
#[derive(Clone, Copy, Debug, Default)]
pub struct InfoStats {
    pub magic: (i32, i32),
    pub attack: (i32, i32),
    pub energy: i32,
    pub dexterity: i32,
    pub mana_max: i32,
}

/// `RenderSkillInfo` (ZzzInventory.cpp:6042-6488) for the skill `skill`: name, damage, range, mana, AG,
/// the attribute and the special notes. TODO(original): Jewel of Harmony skill-power options, the
/// Elf buffs' values, Plasma Storm's damage, mastery/set bonuses and the Dark Lord party note.
pub fn skill_info(skill: u16, table: &SkillTable, class: Class, st: &InfoStats) -> Vec<InfoLine> {
    let Some(def) = table.get(skill) else { return Vec::new() };
    let gap = || InfoLine { text: "\n".into(), color: WHITE, bold: false, bg: None };
    let white = |t: String| InfoLine { text: t, color: WHITE, bold: false, bg: None };
    let dark_red = |t: &str| InfoLine { text: t.into(), color: WHITE, bold: false, bg: Some([160.0 / 255.0, 0.0, 0.0, 1.0]) };
    let mut out = vec![gap(), InfoLine { text: def.name.clone(), color: BLUE, bold: true, bg: None }, gap()];
    let d = def.damage as i32;
    // The cases without the damage line (6208-6262): teleports, buffs and heals, summons, Swell Life,
    // Increase AG, Summon, Critical Damage, the Castle Siege skills, Brand of Skill, Plasma Storm, Recover.
    let no_damage = matches!(skill, 6 | 15 | 16 | 18 | 26 | 27 | 28 | 30..=37 | 48 | 53 | 63 | 64 | 67..=72 | 75 | 76 | 234)
        || (435..=439).contains(&skill)
        || (470..=489).contains(&skill);
    match class {
        Class::Wizard | Class::Summoner => {
            if skill == 16 || (435..=439).contains(&skill) {
                // `GlobalText[578]`, `[880]`, `[881]`.
                let shield = (10.0 + st.dexterity as f32 / 50.0 + st.energy as f32 / 200.0) as i32
                    + if skill >= 435 { (skill as i32 - 435 + 1) * 5 } else { 0 };
                out.push(white(format!("Absorb {shield}% of Damage")));
                out.push(white(format!("Mana Decrease: {}", (st.mana_max as f32 * 0.02) as i32)));
                out.push(white(format!("Duration: {}seconds", (60.0 + st.energy as f32 / 40.0) as i32)));
            } else if skill != skill::SWELL_OF_MAGICPOWER && skill != skill::ALICE_SLEEP && !(535..=539).contains(&skill) {
                let castle = (skill::STUN..=69).contains(&skill) || (skill::ALICE_THORNS..=skill::ALICE_ENERVATION).contains(&skill);
                if !castle && skill != skill::TELEPORT && skill != skill::TELEPORT_B {
                    if (skill::SUMMON_EXPLOSION..=skill::SUMMON_POLLUTION).contains(&skill) {
                        // TODO(original): `GetCurseSkillDamage` (`GlobalText[1692]`); the curse damage
                        // is not in the client's stats view for skills yet.
                    } else {
                        out.push(white(format!("Wizardry Dmg:{}~{}", st.magic.0 + d, st.magic.1 + d + d / 2)));
                    }
                }
            }
        }
        _ => {
            if !no_damage && !matches!(skill, 62 | 515..=519) {
                out.push(white(format!("Skill Damage: {}~{}", d + st.attack.0, d + d / 2 + st.attack.1)));
            }
            if matches!(skill, 62 | 515..=519) {
                out.push(dark_red("Check the details in pet information window"));
            }
        }
    }
    if skill != skill::SWELL_OF_MAGICPOWER && def.distance != 0 {
        out.push(white(format!("Range: {}", def.distance)));
    }
    out.push(white(format!("Mana: {}", def.mana)));
    if def.ability > 0 {
        out.push(white(format!("A G use: {}", def.ability)));
    }
    if def.mastery_type != 255 {
        let attr = ["Ice attribute", "Poison attribute", "Lightning attribute", "Fire attribute", "Earth attribute", "Wind attribute"];
        if let Some(a) = attr.get(def.mastery_type as usize) {
            out.push(InfoLine { text: a.to_string(), color: BLUE, bold: false, bg: None });
        }
    }
    if def.use_type == use_type::MASTER {
        out.push(dark_red("This is a master skill in Guild Battle and Castle Siege"));
        out.push(dark_red(&format!("Can be used when the {} KillCount is completed", def.kill_count)));
    }
    out.push(gap());
    // RUSH 44, JAVELIN 45, DEEPIMPACT 46, ONEFLASH 57, DEATH_CANNON 73, SPACE_SPLIT 74 (SkillManager.h).
    if matches!(skill, 44..=46 | 57 | 73 | 74) {
        out.push(dark_red("Use in Castle Siege"));
    }
    if is_castle_skill(skill) {
        out.push(dark_red("Can be used during Castle Siege with required Kill Count"));
    }
    if skill == 47 {
        out.push(dark_red("Can be used from the mount item"));
    }
    out
}

// --- Bevy ------------------------------------------------------------------------------------------

pub struct SkillsPlugin;

impl Plugin for SkillsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeroSkills>()
            .add_systems(Startup, load_table)
            .add_systems(OnEnter(GameScene::CharacterSelect), |mut s: ResMut<HeroSkills>| *s = HeroSkills::default())
            .add_systems(OnEnter(GameScene::Game), spawn_root)
            .add_systems(OnExit(GameScene::Game), crate::scene::despawn::<SkillBarRoot>)
            .add_systems(Update, (receive, tick_delays, keys, mouse, draw).chain());
    }
}

/// `SkillAttribute[]`.
#[derive(Resource, Default)]
pub struct Skills(pub SkillTable);

fn load_table(mut commands: Commands, args: Res<Args>) {
    let table = mu_data::skills::load_skills(&args.data).unwrap_or_else(|e| {
        warn!("habilidades: sin skill_eng.bmd ({e})");
        SkillTable::default()
    });
    commands.insert_resource(Skills(table));
}

fn receive(
    mut from: MessageReader<crate::net::FromServer>,
    table: Option<Res<Skills>>,
    mut skills: ResMut<HeroSkills>,
    mut world: ResMut<crate::fx::FxWorld>,
) {
    let Some(table) = table else { return };
    let mut changed = false;
    for m in from.read() {
        changed |= skills.apply(&m.0, &|s| table.0.get(s).map_or(0, |d| d.use_type));
    }
    if changed {
        // `CharacterAttribute->Skill[]` for the effects (`Ctx::hero_skill`).
        world.hero_skills = skills.slots[..MAX_MAGIC].iter().map(|&s| s as i32).collect();
        info!("habilidades: {:?}", skills.slots.iter().enumerate().filter(|(_, s)| **s != 0).collect::<Vec<_>>());
    }
}

fn tick_delays(time: Res<Time<Real>>, mut skills: ResMut<HeroSkills>, mut acc: Local<f32>) {
    *acc += time.delta_secs() * 1000.0;
    let ms = *acc as i32;
    if ms > 0 {
        *acc -= ms as f32;
        if skills.delay.iter().any(|&d| d > 0) {
            skills.calc_delay(ms);
        }
    }
}

const DIGITS: [KeyCode; 10] = [
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// `CNewUISkillList::UpdateKeyEvent` (1744-1795): `1`-`0` pick the hotkey's skill; Ctrl + number
/// over a skill of the list puts it on that key. (Shift + 1-4, the pet commands, need pets.)
fn keys(keys: Res<ButtonInput<KeyCode>>, game: Option<Res<State<GameScene>>>, mut skills: ResMut<HeroSkills>) {
    if game.is_none_or(|g| *g.get() != GameScene::Game) {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    for (n, k) in DIGITS.iter().enumerate() {
        if !keys.just_pressed(*k) {
            continue;
        }
        if ctrl && skills.event == Event::HoverList {
            if let Some((slot, ..)) = skills.info {
                skills.set_hotkey(n, slot);
                continue;
            }
        }
        skills.use_hotkey(n);
    }
    skills.update();
}

/// `CNewUISkillList::UpdateMouseEvent` (1395-1742): the current skill button opens the list, the
/// hotkey boxes and the list pick the current skill; hovering shows the tooltip.
#[allow(clippy::too_many_arguments)]
fn mouse(
    windows: Query<&Window>,
    buttons: Res<ButtonInput<MouseButton>>,
    game: Option<Res<State<GameScene>>>,
    table: Option<Res<Skills>>,
    game_ui: Res<crate::options::GameUi>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut skills: ResMut<HeroSkills>,
) {
    let (Some(table), Ok(window)) = (table, windows.single()) else { return };
    if game.is_none_or(|g| *g.get() != GameScene::Game) {
        return;
    }
    skills.info = None;
    if skills.count == 0 || game_ui.modal() {
        return;
    }
    let Some(c) = crate::mainframe::cursor_virtual(window) else { return };
    let push = buttons.pressed(MouseButton::Left);
    let inside = |x: f32, y: f32, w: f32, h: f32| Rect::new(x, y, x + w, y + h).contains(c);
    let s = &mut *skills;
    // The current skill.
    let cur = inside(385.0, 431.0, 32.0, 38.0);
    match s.event {
        Event::None if !push && cur => s.event = Event::HoverCurrent,
        Event::HoverCurrent if !push && !cur => s.event = Event::None,
        Event::HoverCurrent if push && cur => s.event = Event::DownCurrent,
        Event::DownCurrent if !push => {
            if cur {
                s.list_open = !s.list_open;
                sound.play("SOUND_CLICK01");
            }
            s.event = Event::None;
        }
        _ => {}
    }
    if s.event == Event::HoverCurrent {
        s.info = Some((s.current, 385.0 - 5.0, 431.0));
        return;
    }
    if s.event == Event::DownCurrent {
        return;
    }
    // The hotkey boxes.
    let bar = inside(222.0, 431.0, 160.0, 38.0);
    match s.event {
        Event::None if !push && bar => s.event = Event::HoverHotkey,
        Event::HoverHotkey if !push && !bar => s.event = Event::None,
        Event::HoverHotkey if push && bar => s.event = Event::DownHotkey,
        _ => {}
    }
    let start = if s.hotkey_up { 6 } else { 1 };
    for i in 0..5 {
        let x = 190.0 + 32.0 * (i + 1) as f32;
        let key = (start + i) % 10;
        if !inside(x, 431.0, 32.0, 38.0) {
            continue;
        }
        let slot = s.hotkeys[key];
        let t = if slot >= 0 { s.skill(slot as usize) } else { 0 };
        let usable = t != 0 && !is_castle_skill(t) && table.0.get(t).is_none_or(|d| d.use_type != use_type::MASTERLEVEL);
        if !usable {
            if s.event == Event::DownHotkey && !push {
                s.event = Event::None;
            }
            continue;
        }
        if s.event == Event::HoverHotkey {
            s.info = Some((slot as usize, x - 5.0, 431.0));
            return;
        }
        if s.event == Event::DownHotkey && !push {
            s.event = Event::None;
            s.prior = s.current_skill();
            s.current = slot as usize;
            sound.play("SOUND_CLICK01");
            return;
        }
    }
    if s.event == Event::DownHotkey {
        if !push && !bar {
            s.event = Event::None;
        }
        return;
    }
    if !s.list_open {
        return;
    }
    let mut on_list = false;
    for (slot, x, y) in list_entries(s, &table.0, false) {
        if !inside(x, y, 32.0, 38.0) {
            continue;
        }
        on_list = true;
        match s.event {
            Event::None if !push => s.event = Event::HoverList,
            Event::HoverList if push => s.event = Event::DownList,
            _ => {}
        }
        if s.event == Event::HoverList {
            s.info = Some((slot, x, y));
        }
        if s.event == Event::DownList && !push {
            s.event = Event::None;
            s.prior = s.current_skill();
            s.current = slot;
            s.list_open = false;
            sound.play("SOUND_CLICK01");
            return;
        }
        // Keep the hovered one for Ctrl + number.
        if s.event == Event::DownList {
            s.info = Some((slot, x, y));
        }
    }
    if !on_list && (s.event == Event::HoverList || (s.event == Event::DownList && !push)) {
        s.event = Event::None;
    }
}

// --- Drawing ---------------------------------------------------------------------------------------

#[derive(Component)]
struct SkillBarRoot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Tex {
    Atlas(Atlas, bool),
    Box,
    BoxUse,
    Number,
}

impl Tex {
    fn file(self) -> &'static str {
        match self {
            Tex::Atlas(Atlas::Skill1, false) => "newui_skill.jpg",
            Tex::Atlas(Atlas::Skill2, false) => "newui_skill2.jpg",
            Tex::Atlas(Atlas::Command, false) => "newui_command.jpg",
            Tex::Atlas(Atlas::Skill1, true) => "newui_non_skill.jpg",
            Tex::Atlas(Atlas::Skill2, true) => "newui_non_skill2.jpg",
            Tex::Atlas(Atlas::Command, true) => "newui_non_command.jpg",
            Tex::Box => "newui_skillbox.jpg",
            Tex::BoxUse => "newui_skillbox2.jpg",
            Tex::Number => "newui_number1.tga",
        }
    }

    const ALL: [Tex; 9] = [
        Tex::Atlas(Atlas::Skill1, false),
        Tex::Atlas(Atlas::Skill2, false),
        Tex::Atlas(Atlas::Command, false),
        Tex::Atlas(Atlas::Skill1, true),
        Tex::Atlas(Atlas::Skill2, true),
        Tex::Atlas(Atlas::Command, true),
        Tex::Box,
        Tex::BoxUse,
        Tex::Number,
    ];
}

#[derive(Resource)]
struct BarTextures(std::collections::HashMap<Tex, (Handle<Image>, Vec2)>);

#[derive(Clone, Debug, PartialEq)]
enum Piece {
    /// `RenderBitmap(tex, x, y, w, h, u, v, uw, vh)` with (u, v, uw, vh) in texture pixels, tinted.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, src: [f32; 4], tint: [f32; 4] },
    Color { x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4] },
    Text { x: f32, y: f32, w: f32, line: InfoLine },
}

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|h| (t, h)))
        .collect();
    commands.insert_resource(BarTextures(textures));
    commands.spawn((
        Name::new("Barra de habilidades"),
        SkillBarRoot,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(0.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-320.0)),
            width: Val::Px(640.0),
            height: Val::Px(480.0),
            ..default()
        },
        // Over the main frame (the windows use 1 too and come later in the tree).
        GlobalZIndex(1),
        Pickable::IGNORE,
    ));
}

/// `RenderSkillIcon(slot, x, y, 20, 28)` (2109-2528): the icon, its hotkey number and the cooldown cover.
fn push_icon(out: &mut Vec<Piece>, s: &HeroSkills, slot: usize, x: f32, y: f32, table: &SkillTable, gear: &HeroGear, textures: &BarTextures) {
    let t = s.skill(slot);
    if t == 0 {
        return;
    }
    let def = table.get(t);
    let (atlas, col, row) = icon_cell(t, def.map_or(0, |d| d.use_type), def.map_or(0, |d| d.magic_icon));
    let tex = Tex::Atlas(atlas, cant_skill(t, table, gear));
    // `RenderBitmap(..., fU, fV, width / 256, height / 256)`: the texture is drawn as if 256 wide.
    let k = textures.0.get(&tex).map_or(1.0, |(_, size)| size.x / 256.0);
    out.push(Piece::Image { tex, x, y, w: 20.0, h: 28.0, src: [col as f32 * 20.0 * k, row as f32 * 28.0 * k, 20.0 * k, 28.0 * k], tint: [1.0; 4] });
    if let Some(key) = s.hotkeys.iter().position(|&h| h == slot as i32) {
        // `glColor3f(1, 0.9, 0.8); RenderNumber(x + 20, y + 20, iHotKey)` (NewUIRenderNumber.cpp: 12×16 at
        // scale 1 − 0.3, the digit 12×14 of the 128×16 texture).
        let (w, h) = (12.0 * 0.7, 16.0 * 0.7);
        out.push(Piece::Image { tex: Tex::Number, x: x + 20.0 - w / 2.0, y: y + 20.0, w, h, src: [key as f32 * 12.0, 0.0, 12.0, 14.0], tint: [1.0, 0.9, 0.8, 1.0] });
    }
    // `RenderSkillDelay` (2530-2555): a red half-transparent cover, its height the part left.
    if t != skill::INFINITY_ARROW && t != skill::SWELL_OF_MAGICPOWER {
        if let Some(d) = def.filter(|d| d.delay > 0 && s.delay[slot] > 0) {
            let f = s.delay[slot] as f32 / d.delay as f32;
            let dh = 28.0 * f;
            out.push(Piece::Color { x, y: y + 28.0 - dh, w: 20.0, h: dh, rgba: [1.0, 0.5, 0.5, 0.5] });
        }
    }
}

/// The pieces of the bar, the open list and the tooltip.
fn pieces(s: &HeroSkills, table: &SkillTable, gear: &HeroGear, info: &InfoStats, textures: &BarTextures, metrics: &crate::chat::TextMetrics) -> Vec<Piece> {
    let mut out = Vec::new();
    if s.count == 0 {
        return out;
    }
    let boxed = |tex: Tex, x: f32, y: f32| Piece::Image { tex, x, y, w: 32.0, h: 38.0, src: [0.0, 0.0, 32.0, 38.0], tint: [1.0; 4] };
    // `RenderCurrentSkillAndHotSkillList` (1927-1976).
    let start = if s.hotkey_up { 6 } else { 1 };
    for i in 0..5 {
        let x = 190.0 + 32.0 * (i + 1) as f32;
        let slot = s.hotkeys[(start + i) % 10];
        if slot < 0 {
            continue;
        }
        if s.current == slot as usize {
            out.push(boxed(Tex::BoxUse, x, 431.0));
        }
        push_icon(&mut out, s, slot as usize, x + 6.0, 437.0, table, gear, textures);
    }
    push_icon(&mut out, s, s.current, 392.0, 437.0, table, gear, textures);
    // `Render` (1978-2061): the list.
    if s.list_open {
        for (slot, x, y) in list_entries(s, table, true) {
            out.push(boxed(if slot == s.current { Tex::BoxUse } else { Tex::Box }, x, y));
            push_icon(&mut out, s, slot, x + 6.0, y + 6.0, table, gear, textures);
        }
    }
    // `RenderSkillInfo(m_iRenderSkillInfoPosX + 15, m_iRenderSkillInfoPosY - 10, type)`.
    if let Some((slot, x, y)) = s.info {
        let class = gear.class.unwrap_or(Class::Knight);
        let lines = skill_info(s.skill(slot), table, class, info);
        push_tip(&mut out, &lines, x + 15.0, y - 10.0, metrics);
    }
    out
}

const FONT: f32 = crate::chat::FONT;

/// `sy -= Height` and `RenderTipTextList(sx, sy, ..., RT3_SORT_CENTER)` (ZzzInventory.cpp:6481-6487,
/// 285-453), like the inventory's.
fn push_tip(out: &mut Vec<Piece>, lines: &[InfoLine], sx: f32, sy: f32, metrics: &crate::chat::TextMetrics) {
    if lines.is_empty() {
        return;
    }
    let (texts, gaps) = lines.iter().fold((0, 0), |(t, g), l| if l.text == "\n" { (t, g + 1) } else { (t + 1, g) });
    let sy = sy - (texts as f32 * FONT + gaps as f32 * FONT / 2.0);
    let height = (FONT * texts as f32 + FONT / 2.0 * gaps as f32) * 1.1;
    let width = lines.iter().filter(|l| l.text != "\n").map(|l| metrics.width(&l.text) * if l.bold { 1.06 } else { 1.0 }).fold(0.0f32, f32::max).ceil() + 4.0;
    let mut x = (sx - width / 2.0).floor().max(0.0);
    if x + width > 640.0 {
        x = 640.0 - width - 1.0;
    }
    let black = [0.0, 0.0, 0.0, 1.0];
    out.push(Piece::Color { x: x - 1.0, y: sy - 1.0, w: width + 1.0, h: 1.0, rgba: black });
    out.push(Piece::Color { x: x - 1.0, y: sy - 1.0, w: 1.0, h: height + 1.0, rgba: black });
    out.push(Piece::Color { x: x + width, y: sy - 1.0, w: 1.0, h: height + 1.0, rgba: black });
    out.push(Piece::Color { x: x - 1.0, y: sy + height, w: width + 2.0, h: 1.0, rgba: black });
    out.push(Piece::Color { x, y: sy, w: width, h: height, rgba: [0.0, 0.0, 0.0, 0.8] });
    let mut y = sy;
    for l in lines {
        if l.text == "\n" {
            y += FONT / 2.0 * 1.1;
            continue;
        }
        out.push(Piece::Text { x: x + 1.0, y, w: width - 2.0, line: l.clone() });
        y += FONT * 1.1;
    }
}

/// The hero's gear and stats as the bar needs them.
pub fn hero_gear(
    hero: Option<&crate::account::Hero>,
    info: Option<&crate::account::HeroInfo>,
    eq: &crate::char_info::CharEquipment,
    inv: &crate::inventory::Inventory,
) -> (HeroGear, crate::char_stats::Derived) {
    let class = hero.map_or(Class::Knight, |h| h.class);
    let points = info.map_or_else(crate::char_stats::Points::default, |i| crate::char_stats::Points {
        level: hero.map_or(1, |h| h.level as u16),
        strength: i.0.strength,
        agility: i.0.agility,
        vitality: i.0.vitality,
        energy: i.0.energy,
        command: i.0.leadership,
    });
    let d = crate::char_stats::calculate(class, &points, &eq.0);
    let w = |s: usize| inv.equipment.get(s).copied().flatten().map(|i| (i.group, i.index));
    let gear = HeroGear {
        class: hero.map(|h| h.class),
        stats: HeroStats { level: points.level, strength: d.strength, dexterity: d.agility, vitality: d.vitality, energy: d.energy, charisma: d.command },
        right: w(0),
        left: w(1),
    };
    (gear, d)
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    skills: Res<HeroSkills>,
    table: Option<Res<Skills>>,
    textures: Option<Res<BarTextures>>,
    metrics: Res<crate::chat::TextMetrics>,
    (hero, info, eq, inv, stats): (
        Option<Res<crate::account::Hero>>,
        Option<Res<crate::account::HeroInfo>>,
        Res<crate::char_info::CharEquipment>,
        Res<crate::inventory::Inventory>,
        Res<crate::mainframe::CharacterStats>,
    ),
    roots: Query<Entity, With<SkillBarRoot>>,
    mut last: Local<Vec<Piece>>,
) {
    let (Some(table), Some(textures), Ok(root)) = (table, textures, roots.single()) else { return };
    let (gear, d) = hero_gear(hero.as_deref(), info.as_deref(), &eq, &inv);
    let st = InfoStats {
        magic: (d.magic_min as i32, d.magic_max as i32),
        attack: {
            let class = gear.class.unwrap_or(Class::Knight);
            let (a, b, _) = crate::char_stats::attack_damage(class, &d, eq.0.hand);
            (a, b)
        },
        energy: d.energy as i32,
        dexterity: d.agility as i32,
        mana_max: stats.mp_max as i32,
    };
    let now = pieces(&skills, &table.0, &gear, &st, &textures, &metrics);
    if now == *last {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &now {
            match piece {
                Piece::Image { tex, x, y, w, h, src, tint } => {
                    let Some((handle, _)) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone()).with_color(Color::srgba(tint[0], tint[1], tint[2], tint[3]));
                    image.rect = Some(Rect::new(src[0], src[1], src[0] + src[2], src[1] + src[3]));
                    p.spawn((absolute(*x, *y, *w, *h), image, Pickable::IGNORE));
                }
                Piece::Color { x, y, w, h, rgba } => {
                    p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3])), Pickable::IGNORE));
                }
                Piece::Text { x, y, w, line } => {
                    let mut font = TextFont { font_size: FontSize::Px(FONT), font_smoothing: FontSmoothing::AntiAliased, ..default() };
                    if line.bold {
                        font.weight = FontWeight::BOLD;
                    }
                    let c = line.color;
                    let mut node = absolute(*x, *y, *w, FONT);
                    node.justify_content = JustifyContent::Center;
                    let mut e = p.spawn((node, Pickable::IGNORE));
                    if let Some(bg) = line.bg {
                        e.insert(BackgroundColor(Color::srgba(bg[0], bg[1], bg[2], bg[3])));
                    }
                    e.with_children(|t| {
                        t.spawn((Text::new(line.text.clone()), font, LineHeight::Px(FONT), TextColor(Color::srgb(c[0], c[1], c[2])), TextLayout::no_wrap()));
                    });
                }
            }
        }
    });
    *last = now;
}

fn absolute(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Px(x), top: Val::Px(y), width: Val::Px(w), height: Val::Px(h), ..default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> SkillTable {
        // MasteryType 255 = no attribute, as in the real table.
        let mut t = vec![SkillDef { mastery_type: 255, ..Default::default() }; MAX_SKILLS];
        t[19] = SkillDef { name: "Falling Slash".into(), mana: 9, distance: 3, mastery_type: 255, ..Default::default() };
        t[1] = SkillDef { name: "Poison".into(), level: 30, mana: 42, energy: 100, mastery_type: 1, ..Default::default() };
        t[44] = SkillDef { name: "Crescent Moon Slash".into(), mana: 22, ability: 15, distance: 4, delay: 500, mastery_type: 255, ..Default::default() };
        SkillTable(t)
    }

    fn list(skills: &[(u8, u16)]) -> ServerMessage {
        ServerMessage::SkillList { list_type: 0, skills: skills.iter().map(|&(index, number)| SkillSlot { index, number, level: 0 }).collect() }
    }

    #[test]
    fn magic_list_rules() {
        let none = |_: u16| 0u8;
        let mut s = HeroSkills::default();
        // OpenMU's list for a fresh Dark Knight.
        assert!(s.apply(&list(&[(0, 44)]), &none));
        assert_eq!((s.count, s.current, s.current_skill()), (1, 0, 44));
        s.apply(&ServerMessage::SkillAdded(SkillSlot { index: 1, number: 19, level: 0 }), &none);
        assert_eq!(s.count, 2);
        s.current = 1;
        s.apply(&ServerMessage::SkillRemoved { index: 1, number: 19 }, &none);
        // `CurrentSkill >= SkillNumber` → 0.
        assert_eq!((s.count, s.current), (1, 0));
        // A Castle Siege skill first is skipped.
        s.apply(&list(&[(0, 67), (1, 19)]), &none);
        assert_eq!(s.current, 1);
        // Power Slash mastered: Ice Blade goes away.
        s.apply(&list(&[(0, 56), (1, 505)]), &none);
        assert_eq!(s.slots[0], 0);
        assert!(!s.apply(&ServerMessage::VaultClosed, &none));
    }

    #[test]
    fn hotkeys() {
        let mut s = HeroSkills::default();
        s.apply(&list(&[(0, 44), (1, 19)]), &|_| 0);
        s.set_hotkey(3, 1);
        s.set_hotkey(7, 1);
        assert_eq!((s.hotkeys[3], s.hotkeys[7]), (-1, 1), "a slot is on one key only");
        s.use_hotkey(7);
        assert_eq!((s.current, s.prior), (1, 44));
        s.update();
        assert!(s.hotkey_up, "key 7 is in the 6-0 half");
        s.use_hotkey(2);
        assert_eq!(s.current, 1, "an empty key does nothing");
    }

    #[test]
    fn cooldowns() {
        let t = table();
        let mut s = HeroSkills::default();
        s.apply(&list(&[(0, 44)]), &|_| 0);
        assert!(s.check_delay(0, t.get(44), 0));
        assert_eq!(s.delay[0], 500);
        assert!(!s.check_delay(0, t.get(44), 0), "cooling");
        s.calc_delay(300);
        assert_eq!(s.delay[0], 200);
        s.calc_delay(300);
        assert!(s.check_delay(0, t.get(44), 0));
        // Without a delay: always.
        assert!(s.check_delay(0, t.get(19), 0));
    }

    #[test]
    fn list_layout() {
        assert_eq!(list_position(0), (385.0, 390.0));
        assert_eq!(list_position(1), (353.0, 390.0));
        assert_eq!(list_position(2), (417.0, 390.0));
        assert_eq!(list_position(13), (385.0 - 7.0 * 32.0, 390.0));
        assert_eq!(list_position(14), (385.0 - 8.0 * 32.0, 390.0));
        assert_eq!(list_position(18), (385.0 - 12.0 * 32.0 + 32.0, 352.0));
    }

    #[test]
    fn icons() {
        // Poison (1): the first cell of newui_skill; Falling Slash (19): row 2, column 2.
        assert_eq!(icon_cell(1, 0, 0), (Atlas::Skill1, 0, 0));
        assert_eq!(icon_cell(19, 0, 0), (Atlas::Skill1, 2, 2));
        // From 57 on, newui_skill2.
        assert_eq!(icon_cell(57, 0, 0), (Atlas::Skill2, 0, 0));
        assert_eq!(icon_cell(skill::MULTI_SHOT, 0, 0), (Atlas::Skill2, 0, 8));
        assert_eq!(icon_cell(skill::PLASMA_STORM_FENRIR, 0, 0), (Atlas::Command, 4, 0));
        assert_eq!(icon_cell(300, use_type::MASTERACTIVE, 25), (Atlas::Skill2, 1, 6));
    }

    #[test]
    fn tooltip() {
        let t = table();
        let lines = skill_info(44, &t, Class::Knight, &InfoStats { attack: (10, 15), ..Default::default() });
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["\n", "Crescent Moon Slash", "\n", "Skill Damage: 10~15", "Range: 4", "Mana: 22", "A G use: 15", "\n", "Use in Castle Siege"]);
        let p = skill_info(1, &t, Class::Wizard, &InfoStats { magic: (5, 8), ..Default::default() });
        assert!(p.iter().any(|l| l.text == "Wizardry Dmg:5~8"));
    }

    #[test]
    fn grey_icons() {
        let t = table();
        let weak = HeroGear { stats: HeroStats { energy: 100, ..Default::default() }, ..Default::default() };
        assert!(cant_skill(1, &t, &weak), "Poison needs 20 + 30·100·0.04 = 140 energy");
        assert!(!cant_skill(1, &t, &HeroGear { stats: HeroStats { energy: 140, ..Default::default() }, ..weak }));
        assert!(!cant_skill(19, &t, &weak));
    }
}
