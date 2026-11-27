//! Combat: the hero's attacks and skills, what the server says of everyone's, hits, deaths and the
//! damage numbers. Translated from Main 5.2's ZzzInterface.cpp (`MoveHero`, `Action`, `Attack`,
//! `AttackKnight/Wizard/Elf`, `UseSkillWarrior/Wizard/Elf/Summon`), ZzzCharacter.cpp (`SetPlayerAttack`,
//! `SetPlayerMagic`, `SetAttackSpeed`, `SetPlayerShock`, `SetPlayerDie`, `DeadCharacter`) and
//! WSclient.cpp (`ReceiveAttackDamage`, `ReceiveAction`, `ReceiveMagic`, `ReceiveMagicContinue`,
//! `ReceiveDie`, `ReceiveDieExp`, `ReceiveLife`, `ReceiveMana`, `ReceiveRevival`), with
//! `CreatePoint`/`MovePoints`/`RenderPoints` (ZzzEffectPoint.cpp) for the numbers.
//!
//! The client only *asks* (`Hit`, `TargetedSkill`, `AreaSkill`); it never says whom an area skill
//! hit (MuMain stubs `AttackCharacterRange`: the server finds the targets); damage, mana, AG,
//! life and deaths come from the server, as in the original. The hero's own skill effects start
//! when the server echoes the skill (`ReceiveMagic` / `ReceiveMagicContinue` set `AttackTime = 1`),
//! like the original; the animation starts at once on the click.
//!
//! Not yet (each marked where it goes): mounts and pets in the animation choices, Castle Siege /
//! Chaos Castle / Blood Castle rules, auto-attack, potions used when mana is short
//! (`SendRequestUse`), the party skills on other players, `ReloadArrow`, Chaotic Diseier and the
//! Summoner's curses, the release effects of the non-Dark-Knight skills (the
//! `switch` on `g_iLimitAttackTime`, ZzzCharacter.cpp:3932-5070, beyond what `fx` registers).
//!
//! Characters' `o->Alpha` ([`Fade`]): the wizard's Teleport fades out and back in
//! (`CreateTeleportBegin/End`), corpses fade and sink (`DeadCharacter`), and the hero fades in on
//! entering a map. The materials follow it through `model_light::set_alpha`.
//!
//! Nova (MuMain): holding the right button sends `AT_SKILL_NOVA_BEGIN` and loops
//! `PLAYER_SKILL_HELL_BEGIN`; letting go (or a left click) sends `AT_SKILL_NOVA`. The server counts
//! the stages, hits when it gets the release (or after the last stage) and echoes both skills; the
//! ring and its sound come with the release's echo. A corpse killed by Nova or Combo is thrown away
//! from its killer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use mu_protocol::game::{ClientMessage, OBJECT_ID_MASK, ServerMessage};

use crate::account::Class;
use crate::character::{Gear, Player};
use crate::fx::tables::ids;
use crate::monsters::{Monster, MonsterAction, Monsters};
use crate::scene::GameScene;
use crate::skills::{HeroSkills, Skills};
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeroAttack>()
            .init_resource::<Points>()
            .init_resource::<Dying>()
            .add_systems(OnEnter(GameScene::Game), spawn_points_root)
            .add_systems(OnExit(GameScene::Game), (crate::scene::despawn::<PointsRoot>, |mut p: ResMut<Points>, mut d: ResMut<Dying>| {
                p.0.clear();
                d.0.clear();
                d.1.clear();
            }))
            .add_systems(FixedUpdate, (move_points, (dying_tick, fade_tick).chain()).run_if(in_state(GameScene::Game)))
            .add_systems(
                Update,
                (stats_from_server, server_combat, hero_action, hero_cast)
                    .chain()
                    .after(crate::character::HeroWalk)
                    .run_if(in_state(GameScene::Game)),
            )
            .add_systems(PostUpdate, draw_points.after(crate::camera::CameraMoved).run_if(in_state(GameScene::Game)));
    }
}

// --- Attack speeds (`SetAttackSpeed`, ZzzCharacter.cpp:779-906) --------------------------------------

/// `Models[MODEL_PLAYER].Actions[].PlaySpeed` is shared by every player and `SetAttackSpeed` fills it
/// with the hero's `AttackSpeed` and `MagicSpeed`: so other players' attacks also play at the hero's
/// speed, as in the original. Stored as the two speeds.
static ATTACK_SPEED: AtomicU32 = AtomicU32::new(0);
static MAGIC_SPEED: AtomicU32 = AtomicU32::new(0);

/// `SetAttackSpeed`: remembers the hero's speeds for [`attack_play_speed`].
pub fn set_attack_speed(attack: u16, magic: u16) {
    ATTACK_SPEED.store(attack as u32, Ordering::Relaxed);
    MAGIC_SPEED.store(magic as u32, Ordering::Relaxed);
}

/// `CharacterAttribute->AttackSpeed` and `MagicSpeed` as [`set_attack_speed`] last stored them.
pub fn hero_speeds() -> (u16, u16) {
    (ATTACK_SPEED.load(Ordering::Relaxed) as u16, MAGIC_SPEED.load(Ordering::Relaxed) as u16)
}

/// The `PlaySpeed` `SetAttackSpeed` gives an attack or skill action of `player.bmd` (frames per tick),
/// or `None` for the actions it does not touch.
pub fn attack_play_speed(a: usize) -> Option<f32> {
    let a1 = ATTACK_SPEED.load(Ordering::Relaxed) as f32 * 0.004;
    let m1 = MAGIC_SPEED.load(Ordering::Relaxed) as f32 * 0.004;
    let m2 = MAGIC_SPEED.load(Ordering::Relaxed) as f32 * 0.002;
    let a = a as i32;
    let s = match a {
        _ if a == ids::PLAYER_ATTACK_FIST => 0.6 + a1,
        // 806-809 override the loop for the bows.
        _ if (ids::PLAYER_ATTACK_BOW..=ids::PLAYER_ATTACK_FLY_CROSSBOW).contains(&a) => 0.30 + a1,
        _ if (ids::PLAYER_ATTACK_SWORD_RIGHT1..=ids::PLAYER_ATTACK_RIDE_CROSSBOW).contains(&a) => {
            // RIDE_BOW..RIDE_CROSSBOW (58, 59) are 0.30 too.
            if a >= ids::PLAYER_ATTACK_RIDE_CROSSBOW - 1 { 0.30 + a1 } else { 0.25 + a1 }
        }
        _ if a == ids::PLAYER_ATTACK_SKILL_SWORD1 || a == ids::PLAYER_ATTACK_SKILL_SWORD1 + 1 || a == ids::PLAYER_ATTACK_SKILL_SWORD1 + 3 => 0.30 + a1,
        _ if a == ids::PLAYER_ATTACK_SKILL_SWORD1 + 2 => 0.27 + a1,
        _ if a == ids::PLAYER_ATTACK_SKILL_SWORD1 + 4 => 0.24 + a1,
        _ if a == ids::PLAYER_ATTACK_SKILL_WHEEL => 0.24 + a1,
        _ if a == ids::PLAYER_ATTACK_DEATHSTAB => 0.25 + a1,
        _ if a == ids::PLAYER_ATTACK_SKILL_SPEAR => 0.30 + a1,
        _ if a == ids::PLAYER_ATTACK_TWO_HAND_SWORD_TWO => 0.25 + a1,
        _ if a == ids::PLAYER_SKILL_ELF1 => 0.25 + m1,
        _ if (ids::PLAYER_SKILL_HAND1..=ids::PLAYER_SKILL_WEAPON2).contains(&a) => 0.29 + m2,
        _ if a == ids::PLAYER_SKILL_FLASH => 0.40 + m2,
        _ if a == ids::PLAYER_SKILL_INFERNO => 0.60 + m2,
        _ if a == ids::PLAYER_SKILL_HELL => 0.50 + m2,
        // MuMain ZzzCharacter.cpp:1002, 1064.
        _ if a == ids::PLAYER_SKILL_HELL_BEGIN => 0.50 + m2,
        _ if a == ids::PLAYER_SKILL_HELL_START => 0.30,
        _ if a == ids::PLAYER_RIDE_SKILL => 0.30 + m2,
        _ if a == ids::PLAYER_ATTACK_STRIKE => 0.25 + a1,
        _ if a == ids::PLAYER_ATTACK_RUSH => 0.3 + a1,
        _ if a == ids::PLAYER_ATTACK_DEATH_CANNON => 0.2 + a1,
        _ if a == ids::PLAYER_SKILL_SLEEP => 0.3 + m2,
        _ if a == ids::PLAYER_SKILL_LIGHTNING_ORB => 0.4 + m2,
        _ if a == ids::PLAYER_SKILL_CHAIN_LIGHTNING => 0.25 + m2,
        _ if a == ids::PLAYER_SKILL_DRAIN_LIFE => 0.25 + m2,
        _ if a == ids::PLAYER_SKILL_GIGANTICSTORM => 0.55 + m1,
        _ if a == ids::PLAYER_SKILL_FLAMESTRIKE => 0.69 + m2,
        _ if a == ids::PLAYER_SKILL_LIGHTNING_SHOCK => 0.35 + m2,
        _ if a == ids::PLAYER_SKILL_BLOW_OF_DESTRUCTION => 0.3,
        _ if a == ids::PLAYER_RECOVER_SKILL => 0.33,
        _ if a == ids::PLAYER_ATTACK_SKILL_FURY_STRIKE => 0.38,
        _ if a == ids::PLAYER_SKILL_VITALITY => 0.34,
        _ if a == ids::PLAYER_ATTACK_REMOVAL => 0.28,
        _ if a == ids::PLAYER_ATTACK_BOW_UP => 0.30 + a1,
        _ => return None,
    };
    Some(s)
}

/// `IsAttackAction`-like: the actions `PlayerStopAnimationSetting` ends in a stand
/// (`PLAYER_ATTACK_FIST..=PLAYER_RIDE_SKILL` and the later skill actions).
pub fn is_attack_action(a: usize) -> bool {
    let a = a as i32;
    (ids::PLAYER_ATTACK_FIST..=ids::PLAYER_RIDE_SKILL).contains(&a)
        || (ids::PLAYER_SKILL_SLEEP..=ids::PLAYER_SKILL_LIGHTNING_SHOCK).contains(&a)
        || a == ids::PLAYER_RECOVER_SKILL
        || a == ids::PLAYER_ATTACK_BOW_UP
        || a == ids::PLAYER_RUSH1
        || (ids::PLAYER_ATTACK_RUSH..=ids::PLAYER_ATTACK_REMOVAL).contains(&a)
}

// --- Animation choices -------------------------------------------------------------------------------

/// `SetPlayerAttack` for a player on foot (ZzzCharacter.cpp:1042-1104). `sword_count` is
/// `c->SwordCount` (incremented by the caller), `rand` the original's `rand()`.
/// TODO(original): Fenrir, the Dark Horse and Uniria/Dinorant (955-1039): mounts are not drawn yet.
pub fn player_attack_action(gear: &Gear, sword_count: u32, rand: i32) -> usize {
    use mu_data::items::group::*;
    let r = gear.weapons[0].map(|w| (w.group, w.index));
    let l = gear.weapons[1].map(|w| (w.group, w.index));
    let melee = |w: Option<(u8, u16)>| w.is_some_and(|(g, _)| (SWORD..=MACE).contains(&g));
    let a = match (r, l) {
        (None, None) => ids::PLAYER_ATTACK_FIST,
        (Some((g, i)), _) if (SWORD..=MACE).contains(&g) && !gear.two_hand => {
            if melee(l) {
                [ids::PLAYER_ATTACK_SWORD_RIGHT1, ids::PLAYER_ATTACK_SWORD_LEFT1, ids::PLAYER_ATTACK_SWORD_RIGHT1 + 1, ids::PLAYER_ATTACK_SWORD_LEFT1 + 1]
                    [(sword_count % 4) as usize]
            } else {
                let _ = i;
                ids::PLAYER_ATTACK_SWORD_RIGHT1 + (sword_count % 2) as i32
            }
        }
        (Some((SWORD, 21 | 31 | 23 | 25)), _) => ids::PLAYER_ATTACK_TWO_HAND_SWORD_TWO,
        (Some((g, _)), _) if (SWORD..=MACE).contains(&g) => ids::PLAYER_ATTACK_TWO_HAND_SWORD1 + (sword_count % 3) as i32,
        (None, Some((g, _))) if (SWORD..=MACE).contains(&g) => ids::PLAYER_ATTACK_SWORD_LEFT1 + rand % 2,
        (Some((STAFF, _)), _) if !gear.two_hand => ids::PLAYER_ATTACK_SWORD_RIGHT1 + rand % 2,
        (Some((STAFF, _)), _) => ids::PLAYER_SKILL_WEAPON1 + rand % 2,
        (Some((SPEAR, 1 | 2)), _) => ids::PLAYER_ATTACK_SPEAR1,
        (Some((SPEAR, _)), _) => ids::PLAYER_ATTACK_SCYTHE1 + (sword_count % 3) as i32,
        _ => match bow_type(gear) {
            Some(false) => if gear.wings { ids::PLAYER_ATTACK_FLY_BOW } else { ids::PLAYER_ATTACK_BOW },
            Some(true) => if gear.wings { ids::PLAYER_ATTACK_FLY_CROSSBOW } else { ids::PLAYER_ATTACK_CROSSBOW },
            None => ids::PLAYER_ATTACK_FIST,
        },
    };
    a as usize
}

/// `GetEquipedBowType`: `Some(false)` a bow on the left, `Some(true)` a crossbow on the right.
fn bow_type(gear: &Gear) -> Option<bool> {
    use mu_data::items::group::BOW;
    match gear.weapons {
        [_, Some(l)] if l.group == BOW && l.index != 7 => Some(false),
        [Some(r), _] if r.group == BOW && r.index >= 8 && r.index != 15 => Some(true),
        _ => None,
    }
}

/// `SetPlayerBow` (MuMain WSclient.cpp:4084-4136) on foot: only elves, not in a safe zone, by the
/// bow they hold. TODO(original): Fenrir, Uniria and Dinorant (mounts are not drawn yet).
pub fn player_bow_action(class: Class, gear: &Gear, safe_zone: bool) -> Option<usize> {
    if class != Class::Elf || safe_zone {
        return None;
    }
    let a = match bow_type(gear)? {
        false if gear.wings => ids::PLAYER_ATTACK_FLY_BOW,
        false => ids::PLAYER_ATTACK_BOW,
        true if gear.wings => ids::PLAYER_ATTACK_FLY_CROSSBOW,
        true => ids::PLAYER_ATTACK_CROSSBOW,
    };
    Some(a as usize)
}

/// `CheckArrow` (MuMain ZzzInterface.cpp:1217-1239): a crossbow needs bolts on the left hand, a bow
/// arrows on the right, with durability left. `equipment` is `Equipment[WEAPON_RIGHT/LEFT]`.
/// TODO(original): `ReloadArrow` (it equips more from the inventory when they run out).
fn check_arrow(gear: &Gear, equipment: [Option<mu_data::items::Item>; 2]) -> bool {
    use mu_data::items::group::BOW;
    let has = |i: Option<mu_data::items::Item>, index: u16| i.is_some_and(|i| i.group == BOW && i.index == index && i.durability > 0);
    match bow_type(gear) {
        // `ITEM_BOLT` on the left.
        Some(true) => has(equipment[1], 7),
        // `ITEM_ARROWS` on the right.
        Some(false) => has(equipment[0], 15),
        None => true,
    }
}

/// The effects of Multi-Shot, made by the hero's `AttackElf` on casting (MuMain ClassAttack.cpp:358-404)
/// and by `ReceiveMagicContinue` for another player (WSclient.cpp:5483-5532): two `MODEL_MULTI_SHOT3`
/// and two `MODEL_MULTI_SHOT2` 20 ahead, three `MODEL_MULTI_SHOT1` on the archer and the
/// `MODEL_BLADE_SKILL` (subtype 1) that follows it, behind and above. `position` in MU units,
/// `angle` the MU heading in degrees.
pub fn multi_shot_fx(world: &mut crate::fx::FxWorld, owner: Entity, position: Vec3, angle: f32) {
    use crate::fx::core::rotate;
    let angle = Vec3::new(0.0, 0.0, angle);
    let light = Vec3::new(0.8, 0.9, 1.6);
    let ahead = position + rotate(angle, Vec3::new(0.0, 20.0, 0.0));
    for (ty, at, n) in [(ids::MODEL_MULTI_SHOT3, ahead, 2), (ids::MODEL_MULTI_SHOT1, position, 3), (ids::MODEL_MULTI_SHOT2, ahead, 2)] {
        for _ in 0..n {
            crate::fx::request_effect(world, ty, at, angle, light, 0, None);
        }
    }
    // TODO(original): its `PKKey` is the archer's index in `CharactersClient`, used only by the
    // arrows `MODEL_BLADE_SKILL` would shoot (`CreateArrow`, not ported).
    let behind = position + rotate(angle, Vec3::new(0.0, -120.0, 145.0));
    crate::fx::request_effect(world, ids::MODEL_BLADE_SKILL, behind, angle, light, 1, Some(crate::fx::Target::Actor(owner)));
}

/// The swing sound of `SetPlayerAttack` at frame 0 (ZzzCharacter.cpp:1171-1206) for a player.
fn attack_sound(gear: &Gear, rand: i32) -> Option<&'static str> {
    use mu_data::items::group::*;
    match bow_type(gear) {
        Some(false) => return Some("SOUND_BOW01"),
        Some(true) => return Some("SOUND_CROSSBOW01"),
        None => {}
    }
    let w = gear.weapons[0].or(gear.weapons[1])?;
    Some(match (w.group, w.index) {
        (BOW, 13 | 14) => "SOUND_MAGIC",
        (SWORD, 10) | (SPEAR, _) => "SOUND_BRANDISH_SWORD03",
        _ if rand % 2 == 0 => "SOUND_BRANDISH_SWORD01",
        _ => "SOUND_BRANDISH_SWORD02",
    })
}

/// `SetPlayerMagic` (ZzzCharacter.cpp:1211-1241) on foot: women `PLAYER_SKILL_ELF1`, the rest
/// `PLAYER_SKILL_HAND1 + rand() % 2`. TODO(original): the mounted ones.
pub fn player_magic_action(class: Class, rand: i32) -> usize {
    if class.female() { ids::PLAYER_SKILL_ELF1 as usize } else { (ids::PLAYER_SKILL_HAND1 + rand % 2) as usize }
}

/// The original's LCG `rand()` (0..32767), shared by the combat code.
#[derive(Default)]
struct Rng(u32);

impl Rng {
    fn rand(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7FFF) as i32
    }
}

// --- The hero's attack state ---------------------------------------------------------------------------

/// What the hero is doing toward a target: `Attacking`, `ActionTarget`, `MovementType` and
/// `g_MovementSkill`, `SwordCount`, the skill serials and `g_dwLatestMagicTick`.
#[derive(Resource, Default)]
pub struct HeroAttack {
    /// `ActionTarget` with `MOVEMENT_ATTACK`: the entity to hit when the walk ends.
    pub target: Option<Entity>,
    /// `MOVEMENT_SKILL`: the skill slot to cast on `target` when in range.
    pub skill: Option<usize>,
    sword_count: u32,
    /// `g_byLastSkillSerialNumber` (1..50, ZzzInterface.cpp:4626-4637).
    serial: u8,
    /// `g_dwLatestMagicTick`.
    last_magic: Option<Instant>,
    rng: Rng,
    /// `MouseRButtonPress`: frames the right button has been held since Nova's charge started
    /// (0 = not charging).
    nova_press: u32,
}

impl HeroAttack {
    /// `MakeSkillSerialNumber`: the next serial, 1..50.
    fn next_serial(&mut self) -> u8 {
        self.serial += 1;
        if self.serial > 50 {
            self.serial = 1;
        }
        self.serial
    }
}

/// `SendRequestMagic` is throttled to one per 300 ms except Nova's release and the Rage Fighter's
/// Chain Drive / Dark Side (`Type == 40 || 263 || 261`, MuMain ZzzInterface.cpp:1112-1123).
fn magic_allowed(last: &mut Option<Instant>, skill: u16, now: Instant) -> bool {
    if matches!(skill, 40 | 263 | 261) || last.is_none_or(|t| now.duration_since(t) > Duration::from_millis(300)) {
        *last = Some(now);
        true
    } else {
        false
    }
}

/// `GetDestValue(xPos, yPos, xDst, yDst)` (wsclientinline.h:603-617).
pub fn dest_value(from: (i32, i32), to: (i32, i32)) -> u8 {
    let dx = (to.0 - from.0).clamp(-8, 7);
    let dy = (to.1 - from.1).clamp(-8, 7);
    (((dx + 8) as u8) << 4) | (((dy + 8) as u8) & 0x0F)
}

/// `CalcTargetPos(x, y, Tx, Ty)` (ZzzAI.cpp:153-166) with tiles: `(8 + dx) | (8 + dy) << 4` in bytes.
pub fn calc_target_pos(from: (i32, i32), to: (i32, i32)) -> u8 {
    let dx = (8 + to.0 - from.0) as u8;
    let dy = (8 + to.1 - from.1) as u8;
    dx | dy.wrapping_shl(4)
}

/// The angle bytes of `SendRequestMagicContinue`: `(BYTE)(Angle / 360 * 256)`, `(BYTE)(Angle / 360 * 255)`
/// and `(BYTE)((Angle + 180) / 360 * 255)`, with `Angle` in degrees (0..360). The float → byte
/// conversion keeps the low byte, like MSVC's.
pub fn angle_bytes(deg: f32) -> (u8, u8, u8) {
    let b = |v: f32| (v as i32) as u8;
    (b(deg / 360.0 * 256.0), b(deg / 360.0 * 255.0), b((deg + 180.0) / 360.0 * 255.0))
}

/// The hero's heading in the original's degrees.
fn heading_deg(facing: f32) -> f32 {
    facing.to_degrees().rem_euclid(360.0)
}

/// `CheckTile(c, o, Range)` (ZzzInterface.cpp:1375-1389): the hero within `range` tiles of the centre
/// of the target tile.
pub fn check_tile(hero: Vec2, target_tile: (i32, i32), range: f32) -> bool {
    let t = Vec2::new(target_tile.0 as f32 + 0.5, target_tile.1 as f32 + 0.5);
    hero.distance(t) <= range
}

/// `CheckWall(sx, sy, tx, ty)` (ZzzInterface.cpp:1391-1420): Bresenham over the tiles; false at the
/// first `>= TW_NOMOVE` one without `TW_ACTION`, `TW_HEIGHT` or `TW_CAMERA_UP`.
pub fn check_wall(t: &mu_data::terrain::Terrain, from: (i32, i32), to: (i32, i32)) -> bool {
    use mu_data::terrain::attr;
    let (mut x, mut y) = from;
    let (dx, dy) = ((to.0 - x).abs(), -(to.1 - y).abs());
    let (sx, sy) = ((to.0 - x).signum(), (to.1 - y).signum());
    let mut err = dx + dy;
    loop {
        let w = t.attribute(x.clamp(0, 255) as usize, y.clamp(0, 255) as usize);
        if w >= attr::NO_MOVE && w & (attr::ACTION | attr::HEIGHT | attr::CAMERA_UP) == 0 {
            return false;
        }
        if (x, y) == to {
            return true;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// What the click/skill code needs of a character under the mouse.
#[derive(Clone, Copy, Debug)]
pub struct TargetInfo {
    pub entity: Entity,
    pub key: u16,
    pub pos: Vec2,
    pub monster: bool,
}

/// The monster the click would attack (`SelectedCharacter` of a monster, `CheckAttack` is true for
/// them). Used by `character::hero_input`.
#[derive(SystemParam)]
pub struct AttackInput<'w, 's> {
    hovered: Res<'w, crate::names::Hovered>,
    monsters: Query<'w, 's, (&'static Monster, &'static crate::names::Selectable)>,
    pub attack: ResMut<'w, HeroAttack>,
}

impl AttackInput<'_, '_> {
    /// The hovered live monster: entity and tile.
    pub fn clicked_monster(&self) -> Option<(Entity, (usize, usize))> {
        let e = self.hovered.character?;
        let (m, s) = self.monsters.get(e).ok()?;
        (s.kind == crate::names::Kind::Monster && !m.dead).then(|| (e, (m.pos.x.max(0.0) as usize, m.pos.y.max(0.0) as usize)))
    }

    /// `Attacking = 1`, `MovementType = MOVEMENT_ATTACK`, `ActionTarget = SelectedCharacter`.
    pub fn start(&mut self, target: Entity) {
        self.attack.target = Some(target);
        self.attack.skill = None;
    }
}

// --- Server → client ------------------------------------------------------------------------------------

/// `ReceiveLife`, `ReceiveMana`, `ReceiveDamage`, `ReceiveLevelUp`, `ReceiveRevival` and the experience:
/// the main frame's numbers.
fn stats_from_server(
    mut from: MessageReader<crate::net::FromServer>,
    mut stats: ResMut<crate::mainframe::CharacterStats>,
    hero_key: Res<crate::remote::HeroKey>,
    mut info: Option<ResMut<crate::account::HeroInfo>>,
    mut hero: Option<ResMut<crate::account::Hero>>,
    mut players: Query<&mut Player>,
    mut chat: ResMut<crate::chat::Chat>,
    metrics: Res<crate::chat::TextMetrics>,
) {
    for m in from.read() {
        match &m.0 {
            ServerMessage::CurrentHealth { health, shield } => (stats.hp, stats.sd) = (*health as u32, *shield as u32),
            ServerMessage::MaximumHealth { health, shield } => (stats.hp_max, stats.sd_max) = (*health as u32, *shield as u32),
            ServerMessage::CurrentMana { mana, ability } => (stats.mp, stats.ag) = (*mana as u32, *ability as u32),
            ServerMessage::MaximumMana { mana, ability } => (stats.mp_max, stats.ag_max) = (*mana as u32, *ability as u32),
            // `ReceiveDamage` (WSclient.cpp:2778-2792): note `Shield = ShieldDamage`, as the original.
            ServerMessage::PoisonDamage { damage, shield } => {
                stats.hp = stats.hp.saturating_sub(*damage as u32);
                stats.sd = *shield as u32;
            }
            ServerMessage::ObjectHit { id, damage, shield_damage, .. } if id & OBJECT_ID_MASK == hero_key.0 => {
                stats.hp = stats.hp.saturating_sub(*damage as u32);
                stats.sd = stats.sd.saturating_sub(*shield_damage as u32);
            }
            ServerMessage::LevelUp(l) => {
                if let Some(h) = hero.as_mut() {
                    h.level = l.level as u32;
                }
                if let Some(i) = info.as_mut() {
                    i.0.level_up_points = l.points;
                    (i.0.max_health, i.0.max_mana, i.0.max_shield, i.0.max_ability) = (l.max_health, l.max_mana, l.max_shield, l.max_ability);
                }
                (stats.hp_max, stats.mp_max, stats.sd_max, stats.ag_max) = (l.max_health as u32, l.max_mana as u32, l.max_shield as u32, l.max_ability as u32);
            }
            ServerMessage::ExperienceGained { experience, .. } => gain_exp(*experience as u32, info.as_deref_mut(), &mut chat, &metrics),
            ServerMessage::ExperienceGainedLarge { experience, .. } => gain_exp(*experience, info.as_deref_mut(), &mut chat, &metrics),
            // `ReceiveRevival` (WSclient.cpp:1014-1143): back in town with these values.
            ServerMessage::Respawn(r) => {
                (stats.hp, stats.mp, stats.sd, stats.ag) = (r.health as u32, r.mana as u32, r.shield as u32, r.ability as u32);
                if let Some(i) = info.as_mut() {
                    i.0.experience = r.experience;
                    i.0.money = r.money;
                }
                for mut p in &mut players {
                    // TODO(original): a respawn on another map reloads the world (1071-1143).
                    p.pos = Vec2::new(r.x as f32 + 0.5, r.y as f32 + 0.5);
                    p.path.clear();
                    p.dead = false;
                    // `ReceiveRevival` makes the hero again (`CreateCharacter`: `Alpha = AlphaTarget = 1`,
                    // `Teleport = TELEPORT_NONE`, MuMain ZzzCharacter.cpp:12630-12631, 12684), unsunk.
                    p.fade = Fade::default();
                    p.fade.reapply();
                    p.casting = None;
                    p.action = p.stop;
                    p.frame = 0.0;
                    p.facing = crate::character::direction_facing(r.direction);
                }
            }
            _ => {}
        }
    }
}

/// `CharacterAttribute->Experience += Exp` and `GlobalText[486]` "Obtained %d Exp" (WSclient.cpp:5340-5362).
fn gain_exp(exp: u32, info: Option<&mut crate::account::HeroInfo>, chat: &mut crate::chat::Chat, metrics: &crate::chat::TextMetrics) {
    if let Some(i) = info {
        i.0.experience += exp as u64;
    }
    if exp > 0 {
        chat.add("", &format!("Obtained {exp} Exp"), crate::chat_log::MsgType::System, crate::chat_log::MsgType::All, metrics);
    }
}

/// Dead characters: `c->Dead` counts ticks (`MoveCharacter`, ZzzCharacter.cpp:3866-3877): at 15 the
/// death animation (`SetPlayerDie`); `DeadCharacter` (2996-3033) fades it from `Rot >= 1` (50 ticks) and
/// sets `Live = false` when the alpha is gone (100 ticks).
#[derive(Resource, Default)]
struct Dying(HashMap<u16, u32>, HashMap<u16, Knock>);

impl Dying {
    fn forget(&mut self, key: u16) {
        self.0.remove(&key);
        self.1.remove(&key);
    }
}

const DIE_AT: u32 = 15;
const GONE_AT: u32 = 100;

/// `m_byDieType` `AT_SKILL_NOVA` / `AT_SKILL_COMBO`: `ReceiveDie` turns the corpse toward its killer
/// and gives it `Velocity` and `Direction` (MuMain WSclient.cpp:6055-6079); while `c->Dead < 15`
/// `DeadCharacter` throws it away from the killer (ZzzCharacter.cpp:3285-3300).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Knock {
    /// `o->Direction[1]`.
    direction: f32,
    /// `o->Velocity`.
    velocity: f32,
    /// `o->HeadAngle[2]` (= `o->Angle[2]`, toward the killer), in degrees.
    angle: f32,
    /// `m_byDieType == AT_SKILL_NOVA` (the blue lights, 3302-3312).
    nova: bool,
}

impl Knock {
    /// `Velocity = (rand() % 5 + 10) * 0.1`, then `Direction = (0, rand() % 15 + 40, 0)`.
    fn new(rand_velocity: i32, rand_direction: i32, angle: f32, nova: bool) -> Self {
        Self { direction: (rand_direction % 15) as f32 + 40.0, velocity: ((rand_velocity % 5) as f32 + 10.0) * 0.1, angle, nova }
    }

    /// One tick of the throw: `Direction[1] -= Velocity; Velocity += 1` (not below 0), then
    /// `MoveParticle(o, HeadAngle)`. Returns the move in MU units.
    fn tick(&mut self) -> Vec2 {
        self.direction -= self.velocity;
        self.velocity += 1.0;
        if self.direction < 0.0 {
            self.direction = 0.0;
        }
        crate::fx::core::rotate(Vec3::new(0.0, 0.0, self.angle), Vec3::new(0.0, self.direction, 0.0)).truncate()
    }
}

/// `DeadCharacter` (MuMain ZzzCharacter.cpp:3220-3257) at `c->Dead` = `t`: `Rot += 0.02` per tick; from
/// `Rot >= 1` the alpha is `1 - (Rot - 1)` and, while it is at least 0.01, the body sinks 0.4 per tick.
/// Returns (alpha, sink).
pub fn corpse_fade(t: u32) -> (f32, f32) {
    let alpha_at = |k: u32| {
        let rot = 0.02 * k as f32;
        if rot >= 1.0 { 1.0 - (rot - 1.0) } else { 1.0 }
    };
    let sinking = (1..=t).filter(|&k| 0.02 * k as f32 >= 1.0 && alpha_at(k) >= 0.01).count();
    (alpha_at(t).max(0.0), 0.4 * sinking as f32)
}

/// `o->Teleport` (`_define.h:121-124`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Teleport {
    #[default]
    None,
    Begin,
    /// `TELEPORT`: faded out, waiting for the server's `MapChanged`.
    Hidden,
    End,
}

/// A character's `o->Alpha` / `o->AlphaTarget` / `o->Teleport` and how far its corpse sank, with the
/// alpha its materials have now (each module applies it with `model_light::set_alpha`).
#[derive(Clone, Copy, Debug)]
pub struct Fade {
    pub alpha: f32,
    pub target: f32,
    pub teleport: Teleport,
    pub sink: f32,
    applied: f32,
}

impl Default for Fade {
    fn default() -> Self {
        Self { alpha: 1.0, target: 1.0, teleport: Teleport::None, sink: 0.0, applied: 1.0 }
    }
}

impl Fade {
    /// `Alpha = 0` with `AlphaTarget = 1`: it fades in.
    pub fn appearing() -> Self {
        Self { alpha: 0.0, ..Default::default() }
    }

    /// `Alpha(o)` with `AlphaEnable` (on for characters, MuMain ZzzCharacter.cpp:12612; ZzzAI.cpp:83-104):
    /// 0.05 per tick toward `AlphaTarget`.
    pub fn tick(&mut self) {
        if self.target > self.alpha {
            self.alpha = (self.alpha + 0.05).min(1.0);
        } else if self.target < self.alpha {
            self.alpha = (self.alpha - 0.05).max(0.0);
        }
    }

    /// `CreateTeleportBegin` (MuMain ZzzEffectMagicSkill.cpp:162-170): fading out.
    pub fn teleport_begin(&mut self) {
        self.target = 0.0;
        self.teleport = Teleport::Begin;
    }

    /// `CreateTeleportEnd` (ZzzEffectMagicSkill.cpp:172-181): fading back in.
    pub fn teleport_end(&mut self) {
        self.target = 1.0;
        self.teleport = Teleport::End;
    }

    /// `o->Teleport != TELEPORT_BEGIN && o->Teleport != TELEPORT && o->Alpha >= 0.7f`
    /// (ClassAttack.cpp:1550): it can teleport again.
    pub fn can_teleport(&self) -> bool {
        !matches!(self.teleport, Teleport::Begin | Teleport::Hidden) && self.alpha >= 0.7
    }

    /// The alpha to give the materials, if it changed since the last call.
    pub fn take_change(&mut self) -> Option<f32> {
        (self.alpha != self.applied).then(|| {
            self.applied = self.alpha;
            self.alpha
        })
    }

    /// New materials (the model was rebuilt): give them the alpha again.
    pub fn reapply(&mut self) {
        self.applied = f32::NAN;
    }
}

/// `Points[]` (`MAX_POINTS` 100): the floating numbers.
#[derive(Resource, Default)]
struct Points(Vec<Point>);

#[derive(Clone, Debug, PartialEq)]
struct Point {
    /// `o->Type`: the number, -1 "MISS", -2 the Chaos Castle hit mark.
    value: i32,
    /// MU position.
    position: Vec3,
    prev_z: f32,
    color: Vec3,
    gravity: f32,
    scale: f32,
    life_time: i32,
    moves: bool,
}

const MAX_POINTS: usize = 100;

impl Points {
    /// `CreatePoint(Position, Value, Color, scale, bMove)` (ZzzEffectPoint.cpp:25-48).
    fn create(&mut self, position: Vec3, value: i32, color: Vec3, scale: f32) {
        if self.0.len() >= MAX_POINTS {
            return;
        }
        let p = position + Vec3::new(0.0, 0.0, 140.0);
        self.0.push(Point { value, position: p, prev_z: p.z, color, gravity: 10.0, scale, life_time: 0, moves: true });
    }
}

/// `MovePoints` (ZzzEffectPoint.cpp:80-111), each 25 Hz tick.
fn move_points(mut points: ResMut<Points>) {
    for o in points.0.iter_mut() {
        o.prev_z = o.position.z;
        o.life_time -= 1;
        if o.life_time < 0 {
            if o.moves {
                o.position.z += o.gravity;
            }
            o.gravity -= 0.3;
            if o.value != -2 {
                o.scale = (o.scale - 5.0).max(15.0);
            }
        }
    }
    points.0.retain(|o| o.gravity > 0.0);
}

/// `ReceiveAttackDamage`'s colour and scale for a number (WSclient.cpp:2999-3045), without combo or
/// double (handled by the caller).
pub fn damage_color(damage_type: u8, damage: u16, hero: bool) -> (Vec3, f32) {
    if damage == 0 {
        return (if hero { Vec3::ONE } else { Vec3::splat(0.5) }, 15.0);
    }
    match damage_type & 0x0F {
        0 if hero => (Vec3::new(1.0, 0.0, 0.0), 15.0),
        0 => (Vec3::new(1.0, 0.6, 0.0), 15.0),
        1 => (Vec3::new(0.0, 1.0, 1.0), 50.0),
        2 => (Vec3::new(0.0, 1.0, 0.6), 50.0),
        3 => (Vec3::new(0.0, 0.6, 1.0), 50.0),
        4 => (Vec3::new(1.0, 0.0, 1.0), 15.0),
        5 => (Vec3::new(0.0, 1.0, 0.0), 15.0),
        6 => (Vec3::new(0.7, 0.4, 1.0), 15.0),
        _ => (Vec3::ONE, 15.0),
    }
}

/// The `CreatePoint` calls of `ReceiveAttackDamage` (without `Success`: Chaos Castle) for one hit.
fn hit_points(points: &mut Points, pos: Vec3, damage: u16, damage_type: u8, shield: u16, hero: bool) {
    let (light, mut scale) = damage_color(damage_type, damage, hero);
    let combo = damage_type & mu_protocol::combat::damage_type::COMBO != 0;
    let double = damage_type & mu_protocol::combat::damage_type::DOUBLE != 0;
    if damage == 0 {
        points.create(pos, -1, light, 15.0);
    } else {
        if combo {
            scale = 50.0;
        }
        if combo || double {
            let l1 = light - Vec3::splat(0.4);
            let l2 = light - Vec3::splat(0.2);
            points.create(pos, damage as i32, l1, scale);
            points.create(pos + Vec3::new(0.0, 0.0, 10.0), damage as i32, l2, scale + 5.0);
            if combo {
                points.create(pos + Vec3::new(0.0, 0.0, 20.0), damage as i32, light, scale + 10.0);
            }
        }
        points.create(pos, damage as i32, light, scale);
    }
    if shield > 0 {
        points.create(pos + Vec3::new(0.0, 0.0, 25.0), shield as i32, Vec3::new(0.8, 1.0, 0.0), 15.0);
    }
}

/// Everything the server tells about fights: hits, attack animations, skills, deaths.
#[allow(clippy::too_many_arguments)]
fn server_combat(
    mut from: MessageReader<crate::net::FromServer>,
    hero_key: Res<crate::remote::HeroKey>,
    terrain: Option<Res<LoadedTerrain>>,
    mut points: ResMut<Points>,
    mut dying: ResMut<Dying>,
    mut attack: ResMut<HeroAttack>,
    mut monsters_orders: Monsters,
    monsters: Query<(Entity, &Monster)>,
    mut remotes: Query<(Entity, &mut crate::remote::RemotePlayer)>,
    mut heroes: Query<(Entity, &mut Player)>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut selectables: Query<&mut crate::names::Selectable>,
    args: Res<crate::Args>,
) {
    let height = |p: Vec2| terrain.as_ref().map_or(0.0, |t| t.0.height_at(p.x, p.y));
    let mu = |p: Vec2| Vec3::new(p.x * MU_SCALE, p.y * MU_SCALE, height(p));
    for m in from.read() {
        match &m.0 {
            // `ReceiveAttackDamage` (WSclient.cpp:2911-3118).
            ServerMessage::ObjectHit { id, damage, damage_type, shield_damage } => {
                let key = id & OBJECT_ID_MASK;
                let is_hero = key == hero_key.0;
                let pos = if is_hero {
                    heroes.iter().next().map(|(_, p)| p.pos)
                } else {
                    monsters.iter().find(|(_, m)| m.key == key).map(|(_, m)| m.pos).or_else(|| remotes.iter().find(|(_, r)| r.id == key).map(|(_, r)| r.position()))
                };
                let Some(pos) = pos else { continue };
                // `if (MonsterIndex == 275) ...; else if (rand() % 2 == 0) SetPlayerShock(c, Damage)`;
                // `SetPlayerShock` only acts with `Hit > 0`, and never on a dying character
                // (`if (c->Dead > 0) return`, ZzzCharacter.cpp:1387-1402): a late hit would make
                // the corpse get up and fall again.
                if !is_hero && attack.rng.rand() % 2 == 0 && *damage > 0 && !dying.0.contains_key(&key) {
                    if let Some((_, mo)) = monsters.iter().find(|(_, m)| m.key == key) {
                        if mo.number != 275 {
                            monsters_orders.set_action(key, MonsterAction::Shock);
                        }
                    } else if let Some((_, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == key) {
                        // Not while charging Nova (MuMain ZzzCharacter.cpp:1396-1400).
                        if r.pose_state().0 != ids::PLAYER_SKILL_HELL_BEGIN as usize {
                            r.set_action(ids::PLAYER_SHOCK as usize);
                        }
                    }
                }
                hit_points(&mut points, mu(pos), *damage, *damage_type, *shield_damage, is_hero);
            }
            // `ReceiveAction` with `AT_ATTACK1/2` (WSclient.cpp:3148-3157): `SetPlayerAttack` with
            // `AttackTime = 1` toward the hero (`TargetCharacter = HeroIndex`, always).
            ServerMessage::ObjectAnimation { id, direction, action, .. }
                if matches!(*action, mu_protocol::game::action::ATTACK1 | mu_protocol::game::action::ATTACK2) =>
            {
                let key = id & OBJECT_ID_MASK;
                if monsters.iter().any(|(_, m)| m.key == key) {
                    // Monsters: `SwordCount % 3 == 0` → ATTACK1, else ATTACK2; `SwordCount` goes up twice
                    // per call for them (ZzzCharacter.cpp:1155-1162, 1207). One count for all of them here.
                    let a = if attack.sword_count.is_multiple_of(3) { MonsterAction::Attack1 } else { MonsterAction::Attack2 };
                    attack.sword_count += 2;
                    monsters_orders.turn(key, *direction);
                    monsters_orders.set_action(key, a);
                }
                // Players: remote.rs plays it (`player_attack_action`).
            }
            // `ReceiveMagic` (WSclient.cpp:3678-4723).
            ServerMessage::SkillAnimation { skill, source, target } => {
                let (src, tgt) = (source & OBJECT_ID_MASK, target & OBJECT_ID_MASK);
                let target_entity = monsters
                    .iter()
                    .find(|(_, m)| m.key == tgt)
                    .map(|(e, _)| e)
                    .or_else(|| remotes.iter().find(|(_, r)| r.id == tgt).map(|(e, _)| e))
                    .or_else(|| (tgt == hero_key.0).then(|| heroes.iter().next().map(|(e, _)| e)).flatten());
                let tpos = target_entity
                    .and_then(|e| monsters.get(e).map(|(_, m)| m.pos).ok().or_else(|| remotes.get(e).map(|(_, r)| r.position()).ok()))
                    .unwrap_or_default();
                let caster = if src == hero_key.0 {
                    // The cases of `ReceiveMagic` that set the action for the hero too (on foot): the
                    // Dark Lord's strikes (WSclient.cpp:4432-4467, 4631-4646) and hand skills
                    // (4648-4665, 4860-4877). The others are already playing from `cast_skill`.
                    let a = match *skill {
                        60 | 61 | 66 | 74 => Some(ids::PLAYER_ATTACK_STRIKE),
                        64 | 75 => Some(ids::PLAYER_SKILL_HAND1),
                        _ => None,
                    };
                    heroes.iter_mut().next().map(|(e, mut p)| {
                        if let Some(a) = a {
                            set_action(&mut p, a as usize);
                        }
                        e
                    })
                } else if let Some((e, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == src) {
                    if let Some(a) = remote_magic_action(*skill, r.class(), attack.rng.rand()) {
                        r.set_action(a);
                    }
                    Some(e)
                } else if let Some((e, mo)) = monsters.iter().find(|(_, m)| m.key == src) {
                    let a = if attack.sword_count.is_multiple_of(3) { MonsterAction::Attack1 } else { MonsterAction::Attack2 };
                    attack.sword_count += 2;
                    monsters_orders.set_action(mo.key, a);
                    Some(e)
                } else {
                    None
                };
                // `so->m_sTargetIndex = TargetIndex` for Killing Blow (MuMain WSclient.cpp:5127-5133);
                // Chain Drive and Beast Uppercut copy `TargetCharacter` in `AttackStage`
                // (ZzzCharacter.cpp:3031-3038), the same target. The effects read it as `Actor::target`.
                if let (Some(e), Some(t)) = (caster, target_entity)
                    && matches!(*skill, 260 | 551 | 554 | 261 | 552 | 555 | 262 | 558)
                {
                    world.actor_targets.insert(e, t);
                }
                // `sc->Skill = MagicNumber` and `AttackTime = 1`: the skill's per-tick and release code.
                // Nova's start sets no `AttackTime` (MuMain WSclient.cpp:4932-4936).
                if let Some(e) = caster.filter(|_| *skill != NOVA_BEGIN) {
                    start_cast(&mut world, e, *skill, target_entity.map(crate::fx::Target::Actor), Vec3::new(tpos.x * MU_SCALE, tpos.y * MU_SCALE, 0.0));
                }
                // `case AT_SKILL_NOVA_BEGIN` / `case AT_SKILL_NOVA` (MuMain WSclient.cpp:4932-4948): the
                // charge (with `SOUND_NUKE1`, stage 0) and the release, for the hero too.
                if matches!(*skill, NOVA_BEGIN | NOVA) {
                    let a = if *skill == NOVA_BEGIN { ids::PLAYER_SKILL_HELL_BEGIN } else { ids::PLAYER_SKILL_HELL_START } as usize;
                    if src == hero_key.0 {
                        for (_, mut p) in &mut heroes {
                            set_action(&mut p, a);
                            if *skill == NOVA_BEGIN {
                                p.skill_count = 0;
                            }
                        }
                        if *skill == NOVA {
                            // `MouseRButtonPop/Push/MouseRButton = false; MouseRButtonPress = 0`.
                            attack.nova_press = 0;
                        }
                    } else if let Some((_, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == src) {
                        r.keep_or_set_action(a);
                        if *skill == NOVA_BEGIN {
                            r.skill_count = 0;
                        }
                    }
                    if *skill == NOVA_BEGIN {
                        sound.play("SOUND_NUKE1");
                    }
                }
                if *skill == 3 {
                    // AT_SKILL_THUNDER: `PlayBuffer(SOUND_THUNDER01)` (3840-3845).
                    sound.play_object("SOUND_THUNDER01");
                }
                // `case AT_SKILL_TELEPORT: CreateTeleportBegin(so)` (MuMain WSclient.cpp:4418-4430).
                // TODO(original): Kanturu 3's Dark Skull Soldier 5 fades with its own sound instead.
                if *skill == crate::skills::skill::TELEPORT {
                    if src == hero_key.0 {
                        for (_, mut p) in &mut heroes {
                            // `SetAction` keeps the frame of an action already playing.
                            if p.action != ids::PLAYER_SKILL_TELEPORT as usize {
                                play(&mut p, ids::PLAYER_SKILL_TELEPORT as usize);
                            }
                            p.fade.teleport_begin();
                            teleport_fx(&mut world, &mut sound, mu(p.pos), p.facing, Vec3::ONE);
                        }
                    } else if let Some((_, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == src) {
                        r.set_action(ids::PLAYER_SKILL_TELEPORT as usize);
                        r.fade.teleport_begin();
                        let (pos, facing, light) = (r.position(), r.facing(), r.light());
                        teleport_fx(&mut world, &mut sound, mu(pos), facing, light);
                    }
                }
            }
            // `ReceiveMagicContinue` (WSclient.cpp:4725-5188): the angle for everyone (the hero too),
            // the action for other players and monsters, then `AttackTime = 1` with `SkillX/Y`.
            ServerMessage::AreaSkillAnimation { skill, source, x, y, rotation } => {
                let src = source & OBJECT_ID_MASK;
                let facing = (*rotation as f32 / 255.0 * 360.0).to_radians();
                let caster = if src == hero_key.0 {
                    heroes.iter_mut().next().map(|(e, mut p)| {
                        p.facing = facing;
                        e
                    })
                } else if let Some((e, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == src) {
                    r.set_facing(facing);
                    if let Some(a) = remote_area_action(*skill, r.class(), attack.rng.rand()) {
                        r.set_action(a);
                    }
                    // `case AT_SKILL_MULTI_SHOT` (MuMain WSclient.cpp:5483-5532).
                    if *skill == MULTI_SHOT {
                        let pos = r.position();
                        let safe = crate::character::in_safe_zone(pos, terrain.as_deref());
                        if let Some(a) = player_bow_action(r.class(), r.gear(), safe) {
                            r.set_action(a);
                        }
                        multi_shot_fx(&mut world, e, mu(pos), facing.to_degrees());
                    }
                    Some(e)
                } else if let Some((e, mo)) = monsters.iter().find(|(_, m)| m.key == src) {
                    let a = if attack.sword_count.is_multiple_of(3) { MonsterAction::Attack1 } else { MonsterAction::Attack2 };
                    attack.sword_count += 2;
                    monsters_orders.set_action(mo.key, a);
                    Some(e)
                } else {
                    None
                };
                if let Some(e) = caster {
                    let at = Vec3::new((*x as f32 + 0.5) * MU_SCALE, (*y as f32 + 0.5) * MU_SCALE, 0.0);
                    start_cast(&mut world, e, *skill, None, at);
                }
            }
            // `ReceiveDie` (WSclient.cpp:5494-5556): `Dead = true`, `Movement = false`.
            ServerMessage::ObjectKilled { killed, skill, killer } => {
                let key = killed & OBJECT_ID_MASK;
                dying.0.entry(key).or_insert(1);
                // `m_byDieType`: Nova and Combo throw the corpse away from the killer (MuMain
                // WSclient.cpp:6034-6079), except in Blood Castle and Chaos Castle, where it falls
                // (`FallingStartCharacter`). TODO(original): that fall.
                let world_active = args.world as i32 - 1;
                let castle = crate::names::in_chaos_castle(args.world) || (11..=17).contains(&world_active) || world_active == 52;
                if matches!(*skill, NOVA | COMBO) && !castle {
                    let killer = killer & OBJECT_ID_MASK;
                    let pos_of = |k: u16| {
                        if k == hero_key.0 {
                            heroes.iter().next().map(|(_, p)| p.pos)
                        } else {
                            monsters.iter().find(|(_, m)| m.key == k).map(|(_, m)| m.pos).or_else(|| remotes.iter().find(|(_, r)| r.id == k).map(|(_, r)| r.position()))
                        }
                    };
                    if let (Some(from), Some(to)) = (pos_of(key), pos_of(killer)) {
                        // `o->Angle[2] = CreateAngle2D(o->Position, to->Position)`; `HeadAngle = Angle`.
                        let angle = crate::fx::core::create_angle(from.x, from.y, to.x, to.y);
                        let (rv, rd) = (attack.rng.rand(), attack.rng.rand());
                        dying.1.insert(key, Knock::new(rv, rd, angle, *skill == NOVA));
                        let facing = angle.to_radians();
                        if key == hero_key.0 {
                            for (_, mut p) in &mut heroes {
                                p.facing = facing;
                            }
                        } else if let Some((_, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == key) {
                            r.set_facing(facing);
                        } else {
                            monsters_orders.thrown(key, Some(facing), Vec2::ZERO);
                        }
                    }
                }
                // `c->Dead`: no longer picked by the mouse (`SelectCharacter` skips the dead).
                let entity = monsters.iter().find(|(_, m)| m.key == key).map(|(e, _)| e).or_else(|| remotes.iter().find(|(_, r)| r.id == key).map(|(e, _)| e));
                if let Some(mut s) = entity.and_then(|e| selectables.get_mut(e).ok()) {
                    s.dead = true;
                }
                if key == hero_key.0 {
                    for (_, mut p) in &mut heroes {
                        p.dead = true;
                        p.path.clear();
                    }
                    attack.target = None;
                    attack.skill = None;
                } else if monsters.iter().any(|(_, m)| m.key == key) {
                    monsters_orders.walk_path(key, Vec::new());
                }
            }
            // `ReceiveDieExp` (WSclient.cpp:5305-5375): the last hit's number in (1, 0.6, 0).
            ServerMessage::ExperienceGained { killed, damage, .. } | ServerMessage::ExperienceGainedLarge { killed, damage, .. } => {
                let key = killed & OBJECT_ID_MASK;
                if let Some((_, mo)) = monsters.iter().find(|(_, m)| m.key == key) {
                    points.create(mu(mo.pos), *damage as i32, Vec3::new(1.0, 0.6, 0.0), 15.0);
                }
                dying.0.entry(key).or_insert(1);
            }
            ServerMessage::Respawn(_) => {
                dying.0.remove(&hero_key.0);
                dying.1.remove(&hero_key.0);
            }
            // A key back in scope is a new `CreateCharacter` (the respawned monster reuses its
            // index): its old corpse timer must not make it vanish.
            ServerMessage::AddMonstersToScope(list) => {
                list.iter().for_each(|s| dying.forget(s.id & OBJECT_ID_MASK));
            }
            ServerMessage::AddCharactersToScope(list) => {
                list.iter().for_each(|s| dying.forget(s.id & OBJECT_ID_MASK));
            }
            // `ReceiveSkillCount` (MuMain WSclient.cpp:5778-5794): only `AT_SKILL_NOVA` sets the stage;
            // `AT_SKILL_NOVA_BEGIN` does nothing. (OpenMU reports the stages as skill 58, so with it
            // the charge does not grow, as in MuMain.)
            ServerMessage::SkillStage { id, skill, stage } if *skill as u16 == NOVA => {
                let key = id & OBJECT_ID_MASK;
                if key == hero_key.0 {
                    for (_, mut p) in &mut heroes {
                        p.skill_count = *stage;
                    }
                } else if let Some((_, mut r)) = remotes.iter_mut().find(|(_, r)| r.id == key) {
                    r.skill_count = *stage;
                }
            }
            _ => {}
        }
    }
}

/// What `CreateTeleportBegin` / `CreateTeleportEnd` (MuMain ZzzEffectMagicSkill.cpp:162-181) add to
/// the action and `Fade`: `CreateEffect(BITMAP_SPARK + 1, Position, Angle, Light)` and
/// `PlayBuffer(SOUND_MAGIC)`. `position` in MU units, `facing` the MU heading in radians.
pub fn teleport_fx(world: &mut crate::fx::FxWorld, sound: &mut crate::sound::SoundQueue, position: Vec3, facing: f32, light: Vec3) {
    crate::fx::request_effect(world, ids::BITMAP_SPARK + 1, position, Vec3::new(0.0, 0.0, facing.to_degrees()), light, 0, None);
    sound.play_object("SOUND_MAGIC");
}

/// The skills whose character side `fx` has (`registry.skills`): their `AttackTime` starts here.
fn start_cast(world: &mut crate::fx::FxWorld, caster: Entity, skill: u16, target: Option<crate::fx::Target>, at: Vec3) {
    world.casts.retain(|c| c.owner != caster);
    world.casts.push(crate::fx::core::Cast::new(caster, skill as i32, target, at));
}

/// `ReceiveMagic`'s action for another player (3795-4545), on foot. `None`: no action change.
pub fn remote_magic_action(skill: u16, class: Class, rand: i32) -> Option<usize> {
    let a = match skill {
        // Buffs, heals, summons, soul barrier and the targeted magic: `SetPlayerMagic`.
        26..=36 | 16 | 435..=439 | 475..=489 | 1..=5 | 7 | 11 | 17 | 45 => return Some(player_magic_action(class, rand)),
        19..=22 => ids::PLAYER_ATTACK_SKILL_SWORD1 + skill as i32 - 19,
        // SWORD5: `SwordCount` even → SKILL_SWORD5, odd → TWO_HAND_SWORD1 + 2 (one count here).
        23 => ids::PLAYER_ATTACK_SKILL_SWORD1 + 4,
        56 | 505..=509 => ids::PLAYER_ATTACK_TWO_HAND_SWORD_TWO,
        47 => ids::PLAYER_ATTACK_SKILL_SPEAR,
        43 | 460..=464 => ids::PLAYER_ATTACK_DEATHSTAB,
        44 => ids::PLAYER_ATTACK_RUSH,
        73 => ids::PLAYER_ATTACK_DEATH_CANNON,
        60 | 61 | 66 | 74 | 520..=524 | 78 | 525..=529 => ids::PLAYER_ATTACK_STRIKE,
        48 | 470..=474 | 67 | 69 | 70 | 72 => ids::PLAYER_SKILL_VITALITY,
        68 | 71 => ids::PLAYER_ATTACK_REMOVAL,
        64 | 75 => ids::PLAYER_SKILL_HAND1,
        234 => ids::PLAYER_RECOVER_SKILL,
        217..=220 | 535..=539 => ids::PLAYER_SKILL_SLEEP,
        _ => return None,
    };
    Some(a as usize)
}

/// `ReceiveMagicContinue`'s action for another player (4746-5170). Default `SetPlayerMagic`.
pub fn remote_area_action(skill: u16, class: Class, rand: i32) -> Option<usize> {
    let a = match skill {
        41 | 455..=459 | 495..=499 | 55 | 500..=504 => ids::PLAYER_ATTACK_SKILL_WHEEL,
        42 | 465..=469 => ids::PLAYER_ATTACK_SKILL_FURY_STRIKE,
        12 => ids::PLAYER_SKILL_FLASH,
        10 | 440..=444 => ids::PLAYER_SKILL_HELL,
        14 => ids::PLAYER_SKILL_INFERNO,
        78 | 525..=529 => ids::PLAYER_ATTACK_STRIKE,
        65 => ids::PLAYER_SKILL_FLASH,
        232 => ids::PLAYER_SKILL_BLOW_OF_DESTRUCTION,
        236 => ids::PLAYER_SKILL_FLAMESTRIKE,
        237 => ids::PLAYER_SKILL_GIGANTICSTORM,
        230 | 545..=549 => ids::PLAYER_SKILL_LIGHTNING_SHOCK,
        56 | 505..=509 => ids::PLAYER_ATTACK_TWO_HAND_SWORD_TWO,
        214 | 550..=554 => ids::PLAYER_SKILL_DRAIN_LIFE,
        216 => ids::PLAYER_SKILL_LIGHTNING_ORB,
        221 | 222 => ids::PLAYER_SKILL_SLEEP,
        67 => ids::PLAYER_SKILL_VITALITY,
        // Bows: `SetPlayerAttack` (the bow actions need the caster's gear: remote.rs knows it).
        24 | 490..=494 | 52 | 54 => return None,
        // Multi-Shot: `SetPlayerBow` with its gear (`server_combat`).
        235 => return None,
        _ => return Some(player_magic_action(class, rand)),
    };
    Some(a as usize)
}

/// The dead: the death action at 15 ticks, gone at 100 (monsters and other players).
#[allow(clippy::too_many_arguments)]
fn dying_tick(
    mut commands: Commands,
    mut dying: ResMut<Dying>,
    hero_key: Res<crate::remote::HeroKey>,
    mut monsters_orders: Monsters,
    mut remotes: Query<(Entity, &mut crate::remote::RemotePlayer, &crate::shadow::Shadow)>,
    mut heroes: Query<&mut Player>,
    mut world: ResMut<crate::fx::FxWorld>,
    actors: Res<crate::fx::sim::Actors>,
) {
    let mut gone = Vec::new();
    let Dying(timers, knocks) = &mut *dying;
    for (&key, t) in timers.iter_mut() {
        // `DeadCharacter` runs with `c->Dead` before this tick's `Dead += 1` (MuMain
        // ZzzCharacter.cpp:4082-4094).
        let dead = *t;
        if let Some(k) = knocks.get_mut(&key) {
            if dead < 15 {
                let d = k.tick() / MU_SCALE;
                if key == hero_key.0 {
                    for mut p in &mut heroes {
                        p.pos += d;
                    }
                } else if let Some((_, mut r, _)) = remotes.iter_mut().find(|(_, r, _)| r.id == key) {
                    r.push(d);
                } else {
                    monsters_orders.thrown(key, None, d);
                }
            }
            // Ten blue lights on random bones (of the first 32) every tick (3302-3312).
            if k.nova && dead <= 30 {
                let actor = if key == hero_key.0 { actors.hero.and_then(|e| actors.map.get(&e)) } else { actors.map.values().find(|a| a.key == key as i32) };
                if let Some(o) = actor {
                    use crate::fx::core::{BITMAP_LIGHT, Particle, Spawn};
                    for _ in 0..10 {
                        let bone = (world.rng.rand() % 32) as usize;
                        let position = o.bone_world(bone, Vec3::ZERO);
                        let scale = 0.5 + (world.rng.rand() % 100) as f32 / 50.0;
                        world.pending.push(Spawn::Particle(Particle::new(BITMAP_LIGHT, position, o.angle, Vec3::new(0.3, 0.3, 1.0), 5, scale, None)));
                    }
                }
            }
        }
        *t += 1;
        if *t == DIE_AT {
            if key == hero_key.0 {
                for mut p in &mut heroes {
                    // `SetPlayerDie`: `PLAYER_DIE1` (ZzzCharacter.cpp:1349-1472).
                    p.casting = None;
                    p.action = ids::PLAYER_DIE1 as usize;
                    p.frame = 0.0;
                }
            } else if let Some((_, mut r, _)) = remotes.iter_mut().find(|(_, r, _)| r.id == key) {
                r.set_action(ids::PLAYER_DIE1 as usize);
            } else {
                monsters_orders.set_action(key, MonsterAction::Die);
            }
        }
        // `DeadCharacter`: the corpse fades and sinks (the hero's too, ZzzCharacter.cpp:3247).
        let (alpha, sink) = corpse_fade(*t);
        if sink > 0.0 {
            if key == hero_key.0 {
                for mut p in &mut heroes {
                    (p.fade.alpha, p.fade.sink) = (alpha, sink);
                }
            } else if let Some((_, mut r, _)) = remotes.iter_mut().find(|(_, r, _)| r.id == key) {
                (r.fade.alpha, r.fade.sink) = (alpha, sink);
            } else {
                // `Alpha(o)` runs right after `DeadCharacter` with `AlphaTarget` 1 (`fade_tick` does
                // it for the hero and other players).
                monsters_orders.corpse(key, (alpha + 0.05).min(1.0), sink);
            }
        }
        // The hero stays dead until `ReceiveRevival`.
        if *t >= GONE_AT && key != hero_key.0 {
            gone.push(key);
        }
    }
    for key in gone {
        timers.remove(&key);
        // `m_byDieType = 0` (3250).
        knocks.remove(&key);
        if let Some((e, _, shadow)) = remotes.iter().find(|(_, r, _)| r.id == key) {
            commands.entity(e).despawn();
            commands.entity(shadow.1).despawn();
        } else {
            monsters_orders.despawn(key);
        }
    }
}

/// Every tick, after `dying_tick`: `MoveHero`'s teleport fade-out (MuMain ZzzInterface.cpp:2644-2651:
/// 0.1 per tick until below 0.1, then `TELEPORT`) and `Alpha(o)` (`MoveCharacter`, ZzzCharacter.cpp:4083)
/// for the hero and the other players.
fn fade_tick(mut heroes: Query<&mut Player>, mut remotes: Query<&mut crate::remote::RemotePlayer>) {
    for mut p in &mut heroes {
        let f = &mut p.fade;
        if f.teleport == Teleport::Begin {
            f.alpha -= 0.1;
            if f.alpha < 0.1 {
                f.teleport = Teleport::Hidden;
            }
        }
        f.tick();
    }
    for mut r in &mut remotes {
        r.fade.tick();
    }
}

// --- The hero's normal attack (`Action` with `MOVEMENT_ATTACK`) -------------------------------------

/// `Action(c, o, ...)` with `MOVEMENT_ATTACK` (ZzzInterface.cpp:3106-3180) once the hero stands: if the
/// target is within reach, `SetPlayerAttack`, face it, `LetHeroStop` and `SendRequestAttack(Key, Dir)`;
/// with `MOVEMENT_SKILL`, the skill (3181-3367).
#[allow(clippy::too_many_arguments)]
fn hero_action(
    mut attack: ResMut<HeroAttack>,
    mut heroes: Query<&mut Player>,
    monsters: Query<&Monster>,
    terrain: Option<Res<LoadedTerrain>>,
    mut net: ResMut<crate::net::Net>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut cast_ctx: CastContext,
) {
    // `SetAttackSpeed` with the hero's current speeds (`CharacterMachine->Character.AttackSpeed`).
    let (_, d) = crate::skills::hero_gear(cast_ctx.hero.as_deref(), cast_ctx.info.as_deref(), &cast_ctx.eq, &cast_ctx.inv);
    set_attack_speed(d.attack_speed, d.magic_speed);
    let Ok(mut p) = heroes.single_mut() else { return };
    let Some(target) = attack.target else { return };
    if !p.path.is_empty() || p.casting.is_some() {
        return;
    }
    let Ok(m) = monsters.get(target) else {
        attack.target = None;
        return;
    };
    let tile = (m.pos.x as i32, m.pos.y as i32);
    let safe = crate::character::in_safe_zone(p.pos, terrain.as_deref());
    if let Some(slot) = attack.skill.take() {
        attack.target = None;
        if !m.dead && !safe {
            let t = TargetInfo { entity: target, key: m.key, pos: m.pos, monster: true };
            cast_skill(&mut attack, &mut p, slot, None, Some(t), m.pos, &mut cast_ctx, &mut net, &mut sound, terrain.as_deref(), true);
        }
        return;
    }
    attack.target = None;
    if safe || m.dead {
        return;
    }
    // `Range` 1.8, 2.2 with a spear on the right, 6 with a bow (3103-3119).
    use mu_data::items::group::{BOW, SPEAR};
    let range = if p.gear.weapons.iter().flatten().any(|w| w.group == BOW) {
        6.0
    } else if p.gear.weapons[0].is_some_and(|w| w.group == SPEAR) {
        2.2
    } else {
        1.8
    };
    if !check_tile(p.pos, tile, range) {
        return;
    }
    let rand = attack.rng.rand();
    let a = player_attack_action(&p.gear, attack.sword_count, rand);
    attack.sword_count += 1;
    if let Some(s) = attack_sound(&p.gear, attack.rng.rand()) {
        sound.play_object(s);
    }
    play(&mut p, a);
    let d = m.pos - p.pos;
    if d.length() > 1e-3 {
        p.facing = d.x.atan2(-d.y);
    }
    let dir = crate::character::facing_direction(p.facing);
    if net.connected() {
        let (x, y) = (p.pos.x as u8, p.pos.y as u8);
        // `LetHeroStop` (1899-1916): a one-tile `SendCharacterMove`.
        net.send(ClientMessage::Walk { source_x: x, source_y: y, rotation: dir, directions: Vec::new() });
        net.send(ClientMessage::Hit { target: m.key, action: mu_protocol::game::action::ATTACK1, direction: dir });
    }
}

/// `CreateTeleportEnd` for the hero (`ReceiveTeleport` with `Flag` 0, MuMain WSclient.cpp:2352-2355):
/// `PLAYER_SKILL_TELEPORT` from frame 5, fading back in, the spark and the sound.
pub fn hero_teleport_end(p: &mut Player, world: &mut crate::fx::FxWorld, sound: &mut crate::sound::SoundQueue, terrain: Option<&LoadedTerrain>) {
    play(p, ids::PLAYER_SKILL_TELEPORT as usize);
    p.frame = 5.0;
    p.fade.teleport_end();
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    teleport_fx(world, sound, Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, h), p.facing, Vec3::ONE);
}

/// Starts an attack or skill action on the hero with its `SetAttackSpeed` speed.
fn play(p: &mut Player, a: usize) {
    p.action = a;
    p.frame = 0.0;
    let mut speed = attack_play_speed(a).unwrap_or_else(|| crate::character::play_speed(a));
    // `CharacterAnimation` halves Nova's charge (MuMain ZzzCharacter.cpp:2483-2486).
    if a == ids::PLAYER_SKILL_HELL_BEGIN as usize {
        speed /= 2.0;
    }
    p.casting = Some(speed * crate::fx::sim::TICKS_PER_SECOND);
    p.path.clear();
}

/// `SetAction(o, a)` on the hero: an action already playing keeps its frame.
fn set_action(p: &mut Player, a: usize) {
    if p.action != a || p.casting.is_none() {
        play(p, a);
    }
}

/// `AT_SKILL_NOVA` (the release), `AT_SKILL_NOVA_BEGIN` (the charge) and `AT_SKILL_COMBO`.
const NOVA: u16 = ids::AT_SKILL_NOVA as u16;
const NOVA_BEGIN: u16 = ids::AT_SKILL_NOVA_BEGIN as u16;
const COMBO: u16 = ids::AT_SKILL_COMBO as u16;
const MULTI_SHOT: u16 = ids::AT_SKILL_MULTI_SHOT as u16;

// --- The hero's skills (`Attack`) --------------------------------------------------------------------

/// What casting needs besides the hero.
#[derive(SystemParam)]
pub struct CastContext<'w> {
    skills: ResMut<'w, HeroSkills>,
    table: Option<Res<'w, Skills>>,
    stats: Res<'w, crate::mainframe::CharacterStats>,
    hero_key: Res<'w, crate::remote::HeroKey>,
    hero: Option<Res<'w, crate::account::Hero>>,
    info: Option<Res<'w, crate::account::HeroInfo>>,
    eq: Res<'w, crate::char_info::CharEquipment>,
    inv: Res<'w, crate::inventory::Inventory>,
    gate: Res<'w, crate::gate::GateState>,
    real: Res<'w, Time<Real>>,
    world: ResMut<'w, crate::fx::FxWorld>,
    actors: Res<'w, crate::fx::sim::Actors>,
}

/// How a skill goes to the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Send {
    /// `SendRequestMagic(Skill, HeroKey)`.
    MagicSelf,
    /// `SendRequestMagic(Skill, target Key)`: needs a target in range.
    MagicTarget,
    /// `SendRequestMagicContinue(Skill, x, y, angle, dest, tpos, key, serial)`.
    Continue { at_target: bool, angle255: bool, dest: bool, tpos: Tpos, target_key: bool, serial: bool },
    /// The wizard's Teleport: `SendEnterGateRequest(0, TargetX, TargetY)` (MuMain ClassAttack.cpp:1518-1588).
    Teleport,
    /// Nova (MuMain ClassAttack.cpp:1162-1183): the charge `SendRequestMagic(AT_SKILL_NOVA_BEGIN,
    /// HeroKey)`, the release `SendRequestMagic(AT_SKILL_NOVA, getTargetCharacterKey or HeroKey)`.
    Nova { begin: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tpos {
    Zero,
    /// `CalcTargetPos`.
    Pos,
    /// `(BYTE)((Angle + 180) / 360 * 255)`.
    Back,
}

/// The hero's action for a cast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anim {
    None,
    Magic,
    Attack,
    /// `SetPlayerBow`.
    Bow,
    Action(i32),
}

/// One skill's casting rules (from `AttackKnight/Wizard/Elf` and `UseSkill*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub send: Send,
    pub anim: Anim,
    /// `CheckTile(Distance × range_k)` before casting (0 = none).
    pub range_k: f32,
    /// A target out of range: walk to it (`MOVEMENT_SKILL`).
    pub walk: bool,
    /// Turn toward the target point first.
    pub face: bool,
}

/// The rules of a (base) skill for a class, or `None` if not handled (yet).
pub fn plan(skill: u16, class: Class) -> Option<Plan> {
    let s = crate::skills::base_skill(skill);
    let p = |send, anim, range_k, walk, face| Some(Plan { send, anim, range_k, walk, face });
    let cont = |at_target, angle255, dest, tpos, target_key, serial| Send::Continue { at_target, angle255, dest, tpos, target_key, serial };
    let wizardish = matches!(class, Class::Wizard | Class::Gladiator | Class::Summoner);
    let knightish = matches!(class, Class::Knight | Class::Gladiator | Class::Lord);
    match s {
        // AttackWizard (5652-6136).
        9 | 8 if wizardish => p(cont(false, false, false, Tpos::Zero, false, true), Anim::Magic, 1.0, true, true),
        12 if wizardish => p(cont(false, false, false, Tpos::Zero, false, false), Anim::Action(ids::PLAYER_SKILL_FLASH), 1.0, true, true),
        5 if wizardish => p(cont(true, false, false, Tpos::Zero, false, false), Anim::Magic, 1.0, true, true),
        38 | 39 if wizardish => p(cont(true, false, false, Tpos::Zero, true, false), Anim::Magic, 1.0, true, true),
        10 if wizardish => p(cont(false, false, false, Tpos::Zero, false, false), Anim::Action(ids::PLAYER_SKILL_HELL), 0.0, false, false),
        14 if wizardish => p(cont(false, false, false, Tpos::Zero, false, false), Anim::Action(ids::PLAYER_SKILL_INFERNO), 0.0, false, false),
        16 if wizardish => p(Send::MagicSelf, Anim::Magic, 0.0, false, false),
        // `SetPlayerTeleport`: `PLAYER_SKILL_TELEPORT` (MuMain ZzzCharacter.cpp:1374-1385); no range
        // check on the client (the server decides).
        6 if wizardish => p(Send::Teleport, Anim::Action(ids::PLAYER_SKILL_TELEPORT), 0.0, false, false),
        // Nova: no range, no turn (MuMain ClassAttack.cpp:1162-1183).
        58 if wizardish => p(Send::Nova { begin: true }, Anim::Action(ids::PLAYER_SKILL_HELL_BEGIN), 0.0, false, false),
        40 if wizardish => p(Send::Nova { begin: false }, Anim::Action(ids::PLAYER_SKILL_HELL_START), 0.0, false, false),
        1 | 2 | 3 | 17 | 11 | 7 | 4 | 45 if wizardish => p(Send::MagicTarget, Anim::Magic, 1.0, true, true),
        73 if wizardish => p(Send::MagicTarget, Anim::Action(ids::PLAYER_ATTACK_DEATH_CANNON), 1.0, true, true),
        13 if wizardish => p(cont(true, false, false, Tpos::Zero, true, false), Anim::Magic, 1.0, true, true),
        // AttackKnight (5024-5650).
        41 if knightish => p(cont(false, false, false, Tpos::Zero, true, false), Anim::Action(ids::PLAYER_ATTACK_SKILL_WHEEL), 0.0, false, true),
        55 if knightish => p(cont(false, false, true, Tpos::Zero, true, false), Anim::Action(ids::PLAYER_ATTACK_SKILL_WHEEL), 1.0, false, true),
        56 if knightish => p(cont(false, true, true, Tpos::Back, true, false), Anim::Action(ids::PLAYER_ATTACK_TWO_HAND_SWORD_TWO), 1.0, false, true),
        42 if knightish => p(cont(false, false, false, Tpos::Pos, true, false), Anim::Action(ids::PLAYER_ATTACK_SKILL_FURY_STRIKE), 1.0, false, true),
        48 if knightish => p(Send::MagicSelf, Anim::Action(ids::PLAYER_SKILL_VITALITY), 0.0, false, false),
        64 if knightish => p(Send::MagicSelf, Anim::None, 0.0, false, false),
        65 if knightish => p(cont(false, false, true, Tpos::Pos, true, false), Anim::Action(ids::PLAYER_SKILL_FLASH), 1.0, false, true),
        78 if knightish => p(cont(false, false, true, Tpos::Pos, true, false), Anim::Action(ids::PLAYER_ATTACK_STRIKE), 1.0, false, true),
        232 if knightish => p(cont(true, false, false, Tpos::Pos, true, false), Anim::Action(ids::PLAYER_SKILL_BLOW_OF_DESTRUCTION), 1.0, false, false),
        236 if knightish => p(cont(false, true, true, Tpos::Back, true, false), Anim::Action(ids::PLAYER_SKILL_FLAMESTRIKE), 1.0, false, true),
        237 if knightish => p(cont(false, true, true, Tpos::Back, true, false), Anim::Action(ids::PLAYER_SKILL_GIGANTICSTORM), 1.0, false, true),
        // `SkillWarrior` → `CastWarriorSkill` → `UseSkillWarrior` (1937-2280): a target within
        // Distance × 1.2.
        19..=23 if knightish => p(Send::MagicTarget, Anim::Action(ids::PLAYER_ATTACK_SKILL_SWORD1 + s as i32 - 19), 1.2, true, true),
        43 if knightish => p(Send::MagicTarget, Anim::Action(ids::PLAYER_ATTACK_DEATHSTAB), 1.2, true, true),
        44 if knightish => p(Send::MagicTarget, Anim::Action(ids::PLAYER_ATTACK_RUSH), 1.2, true, true),
        60 | 61 | 74 if knightish => p(Send::MagicTarget, Anim::None, 1.2, true, true),
        // AttackElf (4639-5022), SkillElf (3000-3096), UseSkillElf (2346-2401).
        26..=28 if class == Class::Elf => p(Send::MagicSelf, Anim::Magic, 0.0, false, false),
        30..=36 if class == Class::Elf => p(Send::MagicSelf, Anim::Magic, 0.0, false, false),
        53 if class == Class::Elf => p(Send::MagicSelf, Anim::Magic, 0.0, false, false),
        77 if class == Class::Elf => p(Send::MagicSelf, Anim::Action(ids::PLAYER_RUSH1), 0.0, false, false),
        24 | 52 if class == Class::Elf => p(cont(false, false, false, Tpos::Zero, true, false), Anim::Attack, 1.0, false, true),
        54 if class == Class::Elf => p(cont(false, true, true, Tpos::Back, true, false), Anim::Attack, 1.0, false, true),
        // Multi-Shot (MuMain ClassAttack.cpp:324-407): within reach only, no walk.
        235 if class == Class::Elf => p(cont(false, true, true, Tpos::Back, true, false), Anim::Bow, 1.0, false, true),
        51 if class == Class::Elf => p(Send::MagicTarget, Anim::Attack, 1.0, true, true),
        46 if class == Class::Elf => p(Send::MagicTarget, Anim::Action(ids::PLAYER_ATTACK_BOW_UP), 1.0, true, true),
        // AttackWizard's Summoner part (6138-6346) and UseSkillSummon (2403-2553).
        214 | 215 | 216 if class == Class::Summoner => {
            let a = match s {
                214 => ids::PLAYER_SKILL_DRAIN_LIFE,
                215 => ids::PLAYER_SKILL_CHAIN_LIGHTNING,
                _ => ids::PLAYER_SKILL_LIGHTNING_ORB,
            };
            p(cont(true, false, false, Tpos::Zero, true, false), Anim::Action(a), 1.0, true, false)
        }
        219 | 220 if class == Class::Summoner => p(Send::MagicTarget, Anim::None, 1.0, true, true),
        217 | 218 if class == Class::Summoner => p(Send::MagicSelf, Anim::Action(ids::PLAYER_SKILL_SLEEP), 0.0, false, false),
        221 | 222 if class == Class::Summoner => p(cont(false, false, false, Tpos::Zero, false, false), Anim::Action(ids::PLAYER_SKILL_SLEEP), 0.0, false, false),
        230 if class == Class::Summoner => p(cont(false, true, true, Tpos::Back, true, false), Anim::Action(ids::PLAYER_SKILL_LIGHTNING_SHOCK), 1.0, false, true),
        _ => None,
    }
}

/// The safe-zone whitelist of `Attack` (ZzzInterface.cpp:6789-6805).
fn allowed_in_safe_zone(s: u16) -> bool {
    matches!(s, 26 | 27 | 28 | 16 | 48 | 77 | 233 | 234 | 218 | 53 | 64 | 63 | 67..=72 | 75)
        || (480..=489).contains(&s)
        || (435..=439).contains(&s)
        || (470..=479).contains(&s)
}

/// `Attack(Hero)` (ZzzInterface.cpp:6611-7016) for the right button: the current skill toward the
/// character under the mouse or the ground point.
#[allow(clippy::too_many_arguments)]
fn hero_cast(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), (With<Camera3d>, With<IsDefaultUiCamera>)>,
    terrain: Option<Res<LoadedTerrain>>,
    hovered: Res<crate::names::Hovered>,
    targets: Query<(&crate::names::Selectable, Option<&Monster>, Option<&crate::remote::RemotePlayer>)>,
    mut attack: ResMut<HeroAttack>,
    mut heroes: Query<&mut Player>,
    mut net: ResMut<crate::net::Net>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut ctx: CastContext,
) {
    let Ok(mut p) = heroes.single_mut() else { return };
    // A left click on the UI drops the right button's state (MuMain ZzzInterface.cpp:2627-2635).
    if over_ui.0 && buttons.just_pressed(MouseButton::Left) {
        attack.nova_press = 0;
        return;
    }
    if p.dead {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let slot = ctx.skills.current;
    let current = ctx.skills.skill(slot);
    let skill = if attack.nova_press != 0 {
        // Charging Nova (`MouseRButtonPress != 0`, 2660-2675): released (`MouseRButtonPop`) or a left
        // click (`SkillKeyPush`), Nova goes; otherwise it keeps counting.
        if !buttons.pressed(MouseButton::Right) || (current == NOVA && buttons.just_pressed(MouseButton::Left)) {
            attack.nova_press = 0;
            current
        } else {
            attack.nova_press += 1;
            return;
        }
    } else {
        // `(MouseRButtonPush || MouseRButton) && !(MouseLButtonPush || MouseLButton) && !c->Movement`.
        if !buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left) || !p.path.is_empty() || over_ui.0 {
            return;
        }
        if crate::mainframe::cursor_virtual(window).is_none_or(|c| c.y >= 429.0) {
            return;
        }
        if current == NOVA {
            // Nova starts charging (2678-2688): not while teleporting, not without its energy.
            if !matches!(p.fade.teleport, Teleport::None | Teleport::End) {
                return;
            }
            let (gear, _) = crate::skills::hero_gear(ctx.hero.as_deref(), ctx.info.as_deref(), &ctx.eq, &ctx.inv);
            let energy = ctx.table.as_ref().and_then(|t| t.0.get(NOVA)).map_or(0, |d| d.required_energy(false, false));
            if (gear.stats.energy as i32) < energy {
                return;
            }
            attack.nova_press = 1;
            p.skill_count = 0;
            NOVA_BEGIN
        } else {
            current
        }
    };
    // `IsSkillActionReady` (MuMain SkillExecution.cpp:190-217, 316-321): standing, or charging Nova.
    let ready = p.action == ids::PLAYER_SKILL_HELL_BEGIN as usize || (p.casting.is_none() && crate::character::action::standing(p.action));
    if !ready {
        attack.nova_press = 0;
        return;
    }
    // `SelectedCharacter`: a monster, or a player with Ctrl (`CheckAttack`, 1693-1813).
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let target = hovered.character.and_then(|e| {
        let (s, m, r) = targets.get(e).ok()?;
        match (m, r) {
            (Some(m), _) if !m.dead && s.kind == crate::names::Kind::Monster => Some(TargetInfo { entity: e, key: m.key, pos: m.pos, monster: true }),
            (_, Some(r)) if ctrl => Some(TargetInfo { entity: e, key: r.id, pos: r.position(), monster: false }),
            _ => None,
        }
    });
    // `CheckTarget`: the selected one or the ground under the mouse.
    let ground = cameras.single().ok().zip(terrain.as_ref()).and_then(|((cam, tf), t)| {
        let ray = cam.viewport_to_world(tf, window.cursor_position()?).ok()?;
        crate::character::raycast_terrain(&t.0, ray)
    });
    // Nova's release needs no point (it goes off where the hero stands).
    let Some(point) = target.map(|t| t.pos).or(ground).or((skill == NOVA).then_some(p.pos)) else { return };
    cast_skill(&mut attack, &mut p, slot, Some(skill), target, point, &mut ctx, &mut net, &mut sound, terrain.as_deref(), false);
}

/// The common part of `Attack` and the class functions: the checks, then the request and the action.
/// `skill`: what `Attack` casts instead of the slot's skill (Nova's charge); `arrived`: called from
/// `Action` after a `MOVEMENT_SKILL` walk (no new walk then).
#[allow(clippy::too_many_arguments)]
fn cast_skill(
    attack: &mut HeroAttack,
    p: &mut Player,
    slot: usize,
    skill: Option<u16>,
    target: Option<TargetInfo>,
    point: Vec2,
    ctx: &mut CastContext,
    net: &mut crate::net::Net,
    sound: &mut crate::sound::SoundQueue,
    terrain: Option<&LoadedTerrain>,
    arrived: bool,
) {
    let Some(table) = ctx.table.as_ref() else { return };
    let skill = skill.unwrap_or_else(|| ctx.skills.skill(slot));
    if skill == 0 {
        return;
    }
    let Some(def) = table.0.get(skill).cloned() else { return };
    let nova_def = table.0.get(NOVA).cloned();
    let class = p.class;
    let (gear, _) = crate::skills::hero_gear(ctx.hero.as_deref(), ctx.info.as_deref(), &ctx.eq, &ctx.inv);
    // `DemendConditionCheckSkill` (6776) and the grey-icon checks.
    if crate::skills::cant_skill(skill, &table.0, &gear) {
        return;
    }
    let safe = crate::character::in_safe_zone(p.pos, terrain);
    if safe && !allowed_in_safe_zone(skill) {
        return;
    }
    let Some(plan) = plan(skill, class) else {
        debug!("habilidad {skill} ({}) sin lanzar todavía", def.name);
        return;
    };
    if !arrived && matches!(plan.send, Send::Nova { .. }) {
        // `CheckMana(Skill)` (MuMain SkillExecution.cpp:169-188), then `AttackWizard`'s own checks with
        // Nova's mana and AG (none for the release) in its order: energy, mana, AG (which also ends
        // the charge) and `CheckSkillDelay` of the slot (ClassAttack.cpp:1109-1157).
        // TODO(original): drink a mana potion when short (`FindManaItemIndex` + `SendRequestUse`).
        if def.mana as u32 > ctx.stats.mp || def.ability as u32 > ctx.stats.ag {
            return;
        }
        let Some(n) = nova_def else { return };
        let ag = if skill == NOVA { 0 } else { n.ability as u32 };
        if (gear.stats.energy as i32) < def.required_energy(false, false) || n.mana as u32 > ctx.stats.mp {
            return;
        }
        if ag > ctx.stats.ag {
            attack.nova_press = 0;
            return;
        }
        if !ctx.skills.check_delay(slot, Some(&n), gear.stats.charisma) {
            return;
        }
    } else if !arrived {
        // The class functions' checks, in their order: mana (TODO(original): drink a potion,
        // `SendRequestUse`), AG, `CheckSkillDelay` (which already starts the cooldown) and the energy
        // (AttackElf 4651-4677, AttackKnight 5057-5137, AttackWizard 5655-5706).
        if def.mana as u32 > ctx.stats.mp || def.ability as u32 > ctx.stats.ag {
            return;
        }
        if !ctx.skills.check_delay(slot, Some(&def), gear.stats.charisma) {
            return;
        }
        let energy = def.required_energy(class == Class::Knight, matches!(skill, 223 | 224));
        if (gear.stats.energy as i32) < energy {
            return;
        }
    }
    let multi_shot = skill == MULTI_SHOT && plan.anim == Anim::Bow;
    if multi_shot {
        // `case AT_SKILL_MULTI_SHOT` (MuMain ClassAttack.cpp:326-331): arrows and a bow, then it
        // faces the point even when out of reach.
        if !check_arrow(&p.gear, [ctx.inv.equipment[0], ctx.inv.equipment[1]]) || bow_type(&p.gear).is_none() {
            return;
        }
        let d = point - p.pos;
        if d.length() > 1e-3 {
            p.facing = d.x.atan2(-d.y);
        }
    }
    let hero_tile = (p.pos.x as i32, p.pos.y as i32);
    let target_tile = (point.x as i32, point.y as i32);
    let summon_target = matches!(skill, 214..=216 | 540..=544 | 550..=554);
    let needs_target = plan.send == Send::MagicTarget || summon_target;
    if needs_target && target.is_none() {
        return;
    }
    let distance = def.distance as f32;
    if plan.range_k > 0.0 && !check_tile(p.pos, target_tile, distance * plan.range_k) {
        // `PathFinding2(..., Distance)` → `MovementType = MOVEMENT_SKILL` + `SendMove`. (The Summoner's
        // re-path only sets `Movement = true`, 6257-6296; here it walks the same way.)
        if plan.walk && !arrived {
            if let (Some(t), Some(ter)) = (target, terrain) {
                if let Some(path) = mu_data::path::find_path(hero_tile_usize(p.pos), (t.pos.x as usize, t.pos.y as usize), |x, y| ter.0.walkable(x, y), 20_000) {
                    let path: Vec<(usize, usize)> = path.into_iter().take(15).collect();
                    if path.len() > 1 {
                        send_walk(net, &path);
                        p.path = path.into_iter().skip(1).collect();
                        attack.target = Some(t.entity);
                        attack.skill = Some(slot);
                    }
                }
            }
        }
        return;
    }
    if needs_target && terrain.is_some_and(|ter| !check_wall(&ter.0, hero_tile, target_tile)) {
        return;
    }
    if plan.face {
        let d = point - p.pos;
        if d.length() > 1e-3 {
            p.facing = d.x.atan2(-d.y);
        }
    }
    let deg = heading_deg(p.facing);
    let (a256, a255, back) = angle_bytes(deg);
    let connected = net.connected();
    // `LetHeroStop` (`UseSkill*`) and the one-tile `SendCharacterMove` some casts send first (Twisting
    // Slash, Power Slash, Flame Strike, Gigantic Storm, Lightning Shock, Multi-Shot).
    let stops = needs_target || matches!(crate::skills::base_skill(skill), 41 | 56 | 236 | 237 | 230 | 235);
    if connected && stops {
        let dir = crate::character::facing_direction(p.facing);
        net.send(ClientMessage::Walk { source_x: hero_tile.0 as u8, source_y: hero_tile.1 as u8, rotation: dir, directions: Vec::new() });
    }
    // `getTargetCharacterKey` is -1 for monsters: `TKey` stays 0xFFFF for them.
    let tkey = |t: Option<TargetInfo>| t.filter(|t| !t.monster).map_or(0xFFFF, |t| t.key);
    let sent = match plan.send {
        Send::MagicSelf | Send::MagicTarget => {
            let key = if plan.send == Send::MagicSelf { ctx.hero_key.0 } else { target.map_or(0, |t| t.key) };
            let ok = magic_allowed(&mut attack.last_magic, skill, Instant::now());
            if ok && connected {
                net.send(ClientMessage::TargetedSkill { skill, target: key });
            }
            ok
        }
        Send::Continue { at_target, angle255, dest, tpos, target_key, serial } => {
            let (x, y) = if at_target { target_tile } else { hero_tile };
            let tpos = match tpos {
                Tpos::Zero => 0,
                Tpos::Pos => calc_target_pos(hero_tile, target_tile),
                Tpos::Back => back,
            };
            let dest = if dest { dest_value(hero_tile, target_tile) } else { 0 };
            let key = if summon_target { target.map_or(0xFFFF, |t| t.key) } else if target_key { tkey(target) } else { 0xFFFF };
            let serial = if serial { attack.next_serial() } else { 0 };
            if connected {
                net.send(ClientMessage::AreaSkill {
                    skill,
                    x: x.clamp(0, 255) as u8,
                    y: y.clamp(0, 255) as u8,
                    rotation: if angle255 { a255 } else { a256 },
                    dest,
                    target_position: tpos,
                    target: key,
                    serial,
                });
            }
            true
        }
        Send::Teleport => teleport(p, target_tile, ctx, net, terrain),
        Send::Nova { begin } => {
            // TODO(original): `getTargetCharacterKey`'s PvP rules (duel, guild war, PK); a player
            // picked with Ctrl stands in for them, as for the other skills.
            let key = if begin { ctx.hero_key.0 } else { target.filter(|t| !t.monster).map_or(ctx.hero_key.0, |t| t.key) };
            let ok = magic_allowed(&mut attack.last_magic, skill, Instant::now());
            if ok && connected {
                net.send(ClientMessage::TargetedSkill { skill, target: key });
            }
            ok
        }
    };
    if !sent {
        return;
    }
    let rand = attack.rng.rand();
    match plan.anim {
        Anim::None => {}
        Anim::Magic => play(p, player_magic_action(class, rand)),
        Anim::Attack => {
            let a = player_attack_action(&p.gear, attack.sword_count, rand);
            attack.sword_count += 1;
            if let Some(s) = attack_sound(&p.gear, attack.rng.rand()) {
                sound.play_object(s);
            }
            play(p, a);
        }
        Anim::Bow => {
            if let Some(a) = player_bow_action(class, &p.gear, safe) {
                play(p, a);
            }
        }
        Anim::Action(a) => play(p, a as usize),
    }
    if multi_shot {
        let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
        if let Some(e) = ctx.actors.hero {
            multi_shot_fx(&mut ctx.world, e, Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, h), p.facing.to_degrees());
        }
        sound.play("SOUND_SKILL_MULTI_SHOT");
    }
    p.path.clear();
}

/// `case AT_SKILL_TELEPORT` of `AttackWizard` (MuMain ClassAttack.cpp:1518-1588): not with an item
/// on the cursor, not while already teleporting (the action, `Teleport` BEGIN/TELEPORT or `Alpha` below
/// 0.7), only onto a free tile (`TerrainWall` without `TW_ACTION`/`TW_HEIGHT` must be 0: no wall, no
/// safe zone, nobody there) and not while changing map; then face the point and
/// `SendEnterGateRequest(0, TargetX, TargetY)`. The fade-out starts when the server echoes the skill
/// (`ReceiveMagic` → `CreateTeleportBegin`), the fade-in with its `MapChanged` (`CreateTeleportEnd`).
/// The first Teleport after switching from Nova (`HeroPriorSkill == AT_SKILL_NOVA`, 1528-1537) sends
/// Nova's release on the hero instead and plays the charge.
/// TODO(original): the stun/sleep checks and their removal (the client does not track buffs yet).
fn teleport(p: &mut Player, target: (i32, i32), ctx: &mut CastContext, net: &mut crate::net::Net, terrain: Option<&LoadedTerrain>) -> bool {
    if ctx.inv.picked.is_some() {
        return false;
    }
    if ctx.skills.prior == NOVA {
        ctx.skills.prior = crate::skills::skill::TELEPORT;
        if net.connected() {
            net.send(ClientMessage::TargetedSkill { skill: NOVA, target: ctx.hero_key.0 });
        }
        set_action(p, ids::PLAYER_SKILL_HELL_BEGIN as usize);
        return false;
    }
    if p.action == ids::PLAYER_SKILL_TELEPORT as usize || !p.fade.can_teleport() {
        return false;
    }
    let Some(t) = terrain else { return false };
    let (x, y) = (target.0.clamp(0, 255) as usize, target.1.clamp(0, 255) as usize);
    use mu_data::terrain::attr;
    let wall = t.0.attribute(x, y) & !(attr::ACTION | attr::HEIGHT);
    if wall != 0 {
        return false;
    }
    let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - p.pos;
    if d.length() > 1e-3 {
        p.facing = d.x.atan2(-d.y);
    }
    if ctx.gate.zone_moving(ctx.real.elapsed_secs_f64()) {
        return false;
    }
    if net.connected() {
        net.send(ClientMessage::EnterGate { gate: 0, x: x as u8, y: y as u8 });
    }
    true
}

fn hero_tile_usize(p: Vec2) -> (usize, usize) {
    (p.x.max(0.0) as usize, p.y.max(0.0) as usize)
}

/// `SendCharacterMove` of a path.
fn send_walk(net: &mut crate::net::Net, path: &[(usize, usize)]) {
    if !net.connected() {
        return;
    }
    let tiles: Vec<(u8, u8)> = path.iter().map(|&(x, y)| (x.min(255) as u8, y.min(255) as u8)).collect();
    let directions: Vec<u8> = tiles.windows(2).filter_map(|w| mu_protocol::game::direction_between(w[0], w[1])).collect();
    if let (Some(&(sx, sy)), Some(&first)) = (tiles.first(), directions.first()) {
        net.send(ClientMessage::Walk { source_x: sx, source_y: sy, rotation: first, directions });
    }
}

// --- Drawing the numbers (`RenderPoints`, `RenderNumber`) --------------------------------------------

#[derive(Component)]
struct PointsRoot;

#[derive(Resource)]
struct PointTextures {
    /// `BITMAP_FONT + 1` (`Interface/FontTest.tga`) and its width.
    font: Option<(Handle<Image>, Vec2)>,
}

fn spawn_points_root(mut commands: Commands, args: Res<crate::Args>, mut images: ResMut<Assets<Image>>) {
    let font = crate::scene::interface_image(&args, &mut images, "FontTest.tga");
    commands.insert_resource(PointTextures { font });
    commands.spawn((
        Name::new("Números de daño"),
        PointsRoot,
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        // `RenderPoints` comes before the interface: under every window.
        GlobalZIndex(-1),
        Pickable::IGNORE,
    ));
}

/// `RenderNumber(Position, Num, Color, Alpha = Gravity · 0.4, Scale)` (ZzzOpenglUtil.cpp:1056-1104): each
/// digit a `Scale`-sized sprite, the string shifted along world x and y; "MISS" (−1) a 45×20 one.
#[allow(clippy::too_many_arguments)]
fn draw_points(
    mut commands: Commands,
    points: Res<Points>,
    time: Res<Time<Fixed>>,
    textures: Option<Res<PointTextures>>,
    ui_scale: Res<bevy::ui::UiScale>,
    cameras: Query<(&Camera, &GlobalTransform, &Projection), (With<Camera3d>, With<IsDefaultUiCamera>)>,
    roots: Query<Entity, With<PointsRoot>>,
) {
    let (Some(textures), Ok(root), Ok((camera, cam_tf, projection))) = (textures, roots.single(), cameras.single()) else { return };
    let Some((font, size)) = textures.font.clone() else { return };
    commands.entity(root).despawn_children();
    if points.0.is_empty() {
        return;
    }
    let Projection::Perspective(persp) = projection else { return };
    let Some(viewport) = camera.logical_viewport_size() else { return };
    let focal = viewport.y / 2.0 / (persp.fov / 2.0).tan();
    let view = cam_tf.to_matrix().inverse();
    let k = size.x / 256.0;
    let alpha_t = time.overstep_fraction();
    let to_bevy = |m: Vec3| Vec3::new(m.x / MU_SCALE, m.z / MU_SCALE, -m.y / MU_SCALE);
    commands.entity(root).with_children(|ui| {
        for o in &points.0 {
            let z = o.prev_z + (o.position.z - o.prev_z) * alpha_t;
            let base = Vec3::new(o.position.x, o.position.y, z);
            let alpha = (o.gravity * 0.4).clamp(0.0, 1.0);
            let color = Color::srgba(o.color.x.clamp(0.0, 1.0), o.color.y.clamp(0.0, 1.0), o.color.z.clamp(0.0, 1.0), alpha);
            let mut quad = |center: Vec3, w: f32, h: f32, src: Rect| {
                let world = to_bevy(center);
                let depth = -view.transform_point3(world).z;
                if depth <= 0.01 {
                    return;
                }
                let Ok(screen) = camera.world_to_viewport(cam_tf, world) else { return };
                let (pw, ph) = (w / MU_SCALE * focal / depth, h / MU_SCALE * focal / depth);
                let s = ui_scale.0.max(1e-3);
                let mut image = ImageNode::new(font.clone()).with_color(color);
                image.rect = Some(src);
                ui.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px((screen.x - pw / 2.0) / s),
                        top: Val::Px((screen.y - ph / 2.0) / s),
                        width: Val::Px(pw / s),
                        height: Val::Px(ph / s),
                        ..default()
                    },
                    image,
                    Pickable::IGNORE,
                ));
            };
            match o.value {
                // TEXCOORD(0..32/256, 17/32..32/32).
                -1 => quad(base, 45.0, 20.0, Rect::new(0.0, size.y * 17.0 / 32.0, 32.0 * k, size.y)),
                // TODO(original): -2 is Chaos Castle's `BITMAP_FONT_HIT` mark.
                -2 => {}
                v => {
                    let text = v.to_string();
                    let n = text.len() as f32;
                    let s = o.scale;
                    let mut p = base;
                    p.x -= n * 5.0;
                    p.x -= n * s * 0.125;
                    p.y -= n * s * 0.125;
                    for ch in text.bytes() {
                        let d = (ch - b'0') as f32;
                        let src = Rect::new(d * 16.0 * k, 0.0, (d + 1.0) * 16.0 * k, size.y / 2.0);
                        quad(p, s, s, src);
                        p.x += s * 0.5;
                        p.y += s * 0.5;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DeadCharacter`: whole for 49 ticks, then 0.02 less per tick and 0.4 lower while the alpha is
    /// at least 0.01; gone by `GONE_AT`.
    #[test]
    fn corpse_fades_and_sinks() {
        assert_eq!(corpse_fade(DIE_AT), (1.0, 0.0));
        assert_eq!(corpse_fade(49), (1.0, 0.0));
        let (a, s) = corpse_fade(50);
        assert!((a - 1.0).abs() < 1e-5 && (s - 0.4).abs() < 1e-5);
        let (a, s) = corpse_fade(75);
        assert!((a - 0.5).abs() < 1e-4 && (s - 0.4 * 26.0).abs() < 1e-3);
        let (a, s) = corpse_fade(GONE_AT);
        assert!(a < 0.01, "{a}");
        // Tick 100 (alpha 0) no longer sinks: 50..=99.
        assert!((s - 0.4 * 50.0).abs() < 1e-3, "{s}");
    }

    /// `CreateTeleportBegin` → `Alpha(o)` 0.05 per tick to 0 (the hero also loses 0.1 per tick in
    /// `MoveHero`); `CreateTeleportEnd` → back to 1; no new teleport until then.
    #[test]
    fn teleport_fade_cycle() {
        let mut f = Fade::default();
        assert!(f.can_teleport());
        f.teleport_begin();
        assert!(!f.can_teleport());
        for _ in 0..20 {
            f.tick();
        }
        assert_eq!(f.alpha, 0.0);
        f.teleport_end();
        assert!(!f.can_teleport(), "still see-through");
        for _ in 0..15 {
            f.tick();
        }
        assert!(f.alpha >= 0.7 && f.can_teleport());
        for _ in 0..10 {
            f.tick();
        }
        assert_eq!(f.alpha, 1.0);
    }

    /// The materials get the alpha only when it changed, or again after `reapply` (new materials).
    #[test]
    fn fade_changes_reach_the_materials_once() {
        let mut f = Fade::default();
        assert_eq!(f.take_change(), None);
        f.alpha = 0.5;
        assert_eq!(f.take_change(), Some(0.5));
        assert_eq!(f.take_change(), None);
        f.reapply();
        assert_eq!(f.take_change(), Some(0.5));
        assert_eq!(Fade::appearing().take_change(), Some(0.0));
    }

    #[test]
    fn teleport_plan() {
        let p = plan(6, Class::Wizard).unwrap();
        assert_eq!(p.send, Send::Teleport);
        assert_eq!(p.anim, Anim::Action(ids::PLAYER_SKILL_TELEPORT));
        // Not a knight skill, and not usable in a safe zone (`Attack`'s whitelist).
        assert!(plan(6, Class::Knight).is_none());
        assert!(!allowed_in_safe_zone(6));
    }

    #[test]
    fn packet_bytes() {
        // `GetDestValue`: dx 2, dy -1 → (10 << 4) | 7.
        assert_eq!(dest_value((10, 10), (12, 9)), 0xA7);
        assert_eq!(dest_value((10, 10), (30, 0)), (15 << 4) | 0);
        // `CalcTargetPos`: dx in the low nibble, dy in the high one.
        assert_eq!(calc_target_pos((10, 10), (12, 9)), 10 | (7 << 4));
        // Angle bytes at 90°.
        assert_eq!(angle_bytes(90.0), (64, 63, 191));
        // 359°: (Angle + 180) / 360 · 255 = 381 → the low byte.
        assert_eq!(angle_bytes(359.0).2, (381.0f32 as i32) as u8);
    }

    #[test]
    fn reach() {
        assert!(check_tile(Vec2::new(10.5, 10.5), (11, 11), 1.8));
        assert!(!check_tile(Vec2::new(10.5, 10.5), (12, 12), 1.8));
        assert!(check_tile(Vec2::new(10.5, 10.5), (12, 12), 6.0));
    }

    #[test]
    fn serials_cycle() {
        let mut a = HeroAttack::default();
        let first: Vec<u8> = (0..52).map(|_| a.next_serial()).collect();
        assert_eq!((first[0], first[49], first[50], first[51]), (1, 50, 1, 2));
    }

    #[test]
    fn magic_throttle() {
        let mut last = None;
        let t = Instant::now();
        assert!(magic_allowed(&mut last, 17, t));
        assert!(!magic_allowed(&mut last, 17, t + Duration::from_millis(200)));
        assert!(magic_allowed(&mut last, 40, t + Duration::from_millis(250)), "Nova is never throttled");
        assert!(magic_allowed(&mut last, 263, t + Duration::from_millis(260)), "nor Chain Drive");
        assert!(magic_allowed(&mut last, 261, t + Duration::from_millis(270)), "nor Dark Side");
        assert!(!magic_allowed(&mut last, 58, t + Duration::from_millis(280)), "Nova's charge is");
        assert!(magic_allowed(&mut last, 17, t + Duration::from_millis(600)));
    }

    /// Nova (MuMain ClassAttack.cpp:1162-1183): the charge goes to the hero with
    /// `PLAYER_SKILL_HELL_BEGIN`, the release with `PLAYER_SKILL_HELL_START`; wizards only, at any range.
    #[test]
    fn nova_plan() {
        let begin = plan(NOVA_BEGIN, Class::Wizard).unwrap();
        assert_eq!(begin.send, Send::Nova { begin: true });
        assert_eq!(begin.anim, Anim::Action(ids::PLAYER_SKILL_HELL_BEGIN));
        let release = plan(NOVA, Class::Gladiator).unwrap();
        assert_eq!(release.send, Send::Nova { begin: false });
        assert_eq!(release.anim, Anim::Action(ids::PLAYER_SKILL_HELL_START));
        assert_eq!((release.range_k, release.walk, release.face), (0.0, false, false));
        assert!(plan(NOVA, Class::Elf).is_none());
        assert!(!allowed_in_safe_zone(NOVA) && !allowed_in_safe_zone(NOVA_BEGIN));
    }

    /// Multi-Shot (MuMain ClassAttack.cpp:324-407): elves only, within reach without walking, the
    /// bow action of the bow held (none in a safe zone), and only with arrows or bolts left.
    #[test]
    fn multi_shot_rules() {
        use mu_data::items::Item;
        let p = plan(MULTI_SHOT, Class::Elf).unwrap();
        assert_eq!(p.send, Send::Continue { at_target: false, angle255: true, dest: true, tpos: Tpos::Back, target_key: true, serial: false });
        assert_eq!((p.anim, p.range_k, p.walk), (Anim::Bow, 1.0, false));
        assert!(plan(MULTI_SHOT, Class::Wizard).is_none());
        let with = |item: Item, wings: bool| {
            let mut w = [None, None];
            if item.index >= 8 { w[0] = Some(item) } else { w[1] = Some(item) }
            Gear { weapons: w, two_hand: false, wings, ..Gear::default() }
        };
        let bow = with(Item::new(4, 0), false);
        let crossbow = with(Item::new(4, 8), true);
        assert_eq!(player_bow_action(Class::Elf, &bow, false), Some(ids::PLAYER_ATTACK_BOW as usize));
        assert_eq!(player_bow_action(Class::Elf, &crossbow, false), Some(ids::PLAYER_ATTACK_FLY_CROSSBOW as usize));
        assert_eq!(player_bow_action(Class::Elf, &bow, true), None);
        assert_eq!(player_bow_action(Class::Knight, &bow, false), None);
        let ammo = |index: u16, durability: u8| Some(Item { durability, ..Item::new(4, index) });
        assert!(check_arrow(&bow, [ammo(15, 50), bow.weapons[1]]));
        assert!(!check_arrow(&bow, [ammo(15, 0), bow.weapons[1]]), "no arrows left");
        assert!(!check_arrow(&bow, [None, bow.weapons[1]]));
        assert!(check_arrow(&crossbow, [crossbow.weapons[0], ammo(7, 1)]));
        assert!(!check_arrow(&crossbow, [crossbow.weapons[0], ammo(15, 9)]), "arrows are no bolts");
    }

    /// The effects `multi_shot_fx` asks for: 2 + 3 + 2 shots and the blade that follows the archer.
    #[test]
    fn multi_shot_effects() {
        let mut world = crate::fx::FxWorld::default();
        let archer = Entity::from_bits(1 << 32 | 3);
        multi_shot_fx(&mut world, archer, Vec3::new(100.0, 100.0, 0.0), 90.0);
        let types: Vec<(i32, Option<crate::fx::Target>)> = world
            .pending
            .iter()
            .filter_map(|s| match s {
                crate::fx::core::Spawn::Effect(e) => Some((e.ty, e.owner)),
                _ => None,
            })
            .collect();
        let n = |ty: i32| types.iter().filter(|(t, _)| *t == ty).count();
        assert_eq!((n(ids::MODEL_MULTI_SHOT1), n(ids::MODEL_MULTI_SHOT2), n(ids::MODEL_MULTI_SHOT3)), (3, 2, 2));
        assert_eq!(types.last(), Some(&(ids::MODEL_BLADE_SKILL, Some(crate::fx::Target::Actor(archer)))));
    }

    /// `DeadCharacter` with `m_byDieType == AT_SKILL_NOVA`: the corpse slides away from its killer,
    /// slower every tick, and stops.
    #[test]
    fn nova_corpse_is_thrown_away() {
        // The killer straight to the -Y side of the corpse: the corpse faces it (`CreateAngle` 0)
        // and goes toward +Y.
        let angle = crate::fx::core::create_angle(0.0, 0.0, 0.0, -100.0);
        // rand() 2 and 7: `Velocity` 1.2, `Direction[1]` 47.
        let mut k = Knock::new(2, 7, angle, true);
        assert!((k.velocity - 1.2).abs() < 1e-5 && k.direction == 47.0, "{k:?}");
        let first = k.tick();
        assert!((first.y - 45.8).abs() < 1e-3 && first.x.abs() < 1e-3, "{first:?}");
        let total: Vec2 = first + (1..14).map(|_| k.tick()).sum::<Vec2>();
        assert!(total.y > 200.0 && total.y < 300.0, "{total:?}");
        assert_eq!(k.tick(), Vec2::ZERO, "it has stopped");
    }

    #[test]
    fn numbers_move() {
        let mut p = Points::default();
        p.create(Vec3::ZERO, 23, Vec3::ONE, 50.0);
        assert_eq!(p.0[0].position.z, 140.0);
        let mut ticks = 0;
        let mut app = App::new();
        app.insert_resource(p);
        let mut s = IntoSystem::into_system(move_points);
        s.initialize(app.world_mut());
        while !app.world().resource::<Points>().0.is_empty() {
            s.run((), app.world_mut()).unwrap();
            ticks += 1;
        }
        // Gravity 10 down to 0 by 0.3: 34 ticks; the scale shrinks to 15.
        assert_eq!(ticks, 34);
    }

    #[test]
    fn hit_colours() {
        assert_eq!(damage_color(0, 5, true).0, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(damage_color(0, 5, false).0, Vec3::new(1.0, 0.6, 0.0));
        assert_eq!(damage_color(2, 5, false), (Vec3::new(0.0, 1.0, 0.6), 50.0));
        assert_eq!(damage_color(3, 0, false).0, Vec3::splat(0.5));
        let mut p = Points::default();
        hit_points(&mut p, Vec3::ZERO, 10, mu_protocol::combat::damage_type::DOUBLE, 3, false);
        assert_eq!(p.0.len(), 4, "two ghosts, the number and the shield");
        let mut miss = Points::default();
        hit_points(&mut miss, Vec3::ZERO, 0, 0, 0, false);
        assert_eq!(miss.0[0].value, -1);
    }

    #[test]
    fn attack_actions() {
        use mu_data::items::Item;
        let gear = |r: Option<Item>, l: Option<Item>, two: bool| Gear { weapons: [r, l], two_hand: two, ..Gear::default() };
        assert_eq!(player_attack_action(&gear(None, None, false), 0, 0), ids::PLAYER_ATTACK_FIST as usize);
        let sword = Some(Item::new(0, 1));
        assert_eq!(player_attack_action(&gear(sword, None, false), 1, 0), ids::PLAYER_ATTACK_SWORD_RIGHT1 as usize + 1);
        assert_eq!(player_attack_action(&gear(sword, sword, false), 1, 0), ids::PLAYER_ATTACK_SWORD_LEFT1 as usize);
        assert_eq!(player_attack_action(&gear(Some(Item::new(0, 21)), None, true), 0, 0), ids::PLAYER_ATTACK_TWO_HAND_SWORD_TWO as usize);
        assert_eq!(player_attack_action(&gear(None, Some(Item::new(4, 0)), false), 0, 0), ids::PLAYER_ATTACK_BOW as usize);
        set_attack_speed(25, 50);
        assert!((attack_play_speed(ids::PLAYER_ATTACK_SWORD_RIGHT1 as usize).unwrap() - (0.25 + 0.1)).abs() < 1e-5);
        assert!((attack_play_speed(ids::PLAYER_SKILL_HAND1 as usize).unwrap() - (0.29 + 0.1)).abs() < 1e-5);
        assert!(attack_play_speed(ids::PLAYER_STOP_MALE as usize).is_none());
        // Nova (MuMain ZzzCharacter.cpp:1002, 1064): the charge follows the magic speed, the release
        // does not.
        assert!((attack_play_speed(ids::PLAYER_SKILL_HELL_BEGIN as usize).unwrap() - (0.50 + 0.1)).abs() < 1e-5);
        assert!((attack_play_speed(ids::PLAYER_SKILL_HELL_START as usize).unwrap() - 0.30).abs() < 1e-5);
    }

    #[test]
    fn plans() {
        let p = plan(41, Class::Knight).unwrap();
        assert!(matches!(p.send, Send::Continue { at_target: false, target_key: true, .. }));
        assert_eq!(p.anim, Anim::Action(ids::PLAYER_ATTACK_SKILL_WHEEL));
        assert_eq!(plan(17, Class::Wizard).unwrap().send, Send::MagicTarget);
        assert!(plan(17, Class::Knight).is_none(), "Energy Ball is the wizard's");
        // Master skills follow their base: Hellfire Strengthener (440) is Hellfire.
        assert_eq!(plan(440, Class::Wizard), plan(10, Class::Wizard));
        assert!(allowed_in_safe_zone(26) && !allowed_in_safe_zone(41));
    }
}