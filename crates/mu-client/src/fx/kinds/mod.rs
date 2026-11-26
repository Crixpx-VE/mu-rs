//! Code for each effect kind, translated from MuMain and Main 5.2. Each file is one family and has
//! `pub fn register(r: &mut Registry)`, which registers its kinds.
//!
//! Coverage (Main 5.2 with the Rage Fighter code, as MuMain): every `case` type of
//! `CreateParticle`/`MoveParticles` (98), `CreateJoint`/`MoveJoint` (31) and
//! `CreateEffect`/`MoveEffect` (447) is registered; `fx::smoke_tests` runs them all.
//! Client-side hits (`CheckCharacterRange`, `CheckClientArrow`, `CheckTargetRange`,
//! `AttackCharacterRange` with `CharacterMachine->PacketSerial` as `o->Weapon`), effect models' own
//! bones/vertices/action lengths, the `RENDER_*` modes, the per-map models, sounds (`PlayBuffer`
//! with an object, looping, `StopBuffer`), the targets' state (light, alpha, `StartPosition`,
//! `HeadAngle`, an effect's `EyeLeft`), the buffs the server reports (buffs.rs), `m_sTargetIndex`
//! of the Rage Fighter skills, the hero's attack/magic speeds, the terrain light and render level
//! while drawing, and the changes to characters that have a channel (`ActorChange`: the tornado
//! spin, `SetPlayerStop`) are in. MuMain's `AttackCharacterRange` is an empty stub; the hits are
//! kept (Main 5.2) because OpenMU still takes them (`AreaSkillHitHandlerPlugIn`).
//!
//! What remains is marked `TODO(original)` in the kinds:
//! - left for later on purpose: Castle Siege, Kanturu, Crywolf and other event or server state
//!   (`battleCastle::*`, `IsMayaScene`, the catapult); water waves and water-surface marks
//!   (`AddWaterWave`, `RenderWaterTerrain`); object blurs (`CreateObjectBlur`/`RemoveObjectBlurs`);
//!   `MODEL_BOSS_ATTACK`, `MODEL_BLOOD` and `MODEL_MONSTER01+64` (no `.bmd` in either reference);
//! - character state the client does not have: the monsters' eye/trail points (`EyeLeft`...,
//!   filled by their draw code), `PriorAction`/`PriorAnimationFrame` for sub-frame poses, a
//!   character's `LifeTime`, mounts and pets (`m_iChaIndex`, Fenrir), `getTargetCharacterKey`
//!   (PvP, duel and guild-war state), `m_bySkillSerialNum` on trails, the frustum `Visible`;
//! - character changes without a channel yet: `o->Owner->Alpha` (character materials have no
//!   alpha), `o->Owner->Velocity`, `HiddenMesh` of a followed object, moving another effect;
//! - engine gaps: joints or map objects as effect owners, the type of a followed effect, the
//!   "dummy" effects' models animated by trails, some `DescribeJoint` / `RenderParticles` draw
//!   details, texture sizes in the simulation, the effect being created as a target (its state
//!   is only visible from the next tick);
//! - the character side of skills that are not cast yet (arrows, the Rage Fighter skills, buffs
//!   given by the casts, the monsters' own projectiles).

pub mod common;
pub mod particles_1;
pub mod particles_2;
pub mod particles_3;
pub mod particles_4;
pub mod particles_5;
pub mod particles_6;
pub mod joints_1;
pub mod joints_2;
pub mod joints_3;
pub mod effects_dk1;
pub mod effects_dk2;
pub mod skills_dk;
pub mod skills_magic;
pub mod objects_1;
pub mod objects_2;
pub mod objects_login;
pub mod objects_elbeland;
pub mod objects_kalima;
pub mod objects_kanturu;
pub mod objects_raklion;
pub mod fire;

pub mod effects_elf;
pub mod effects_summoner;
pub mod effects_wizard;
pub mod effects_dl_mg1;
pub mod effects_dl_mg2;
pub mod effects_bitmaps1;
pub mod effects_bitmaps2;
pub mod effects_monk;
pub mod effects_monsters1;
pub mod effects_monsters2;
pub mod effects_events;

use super::core::Registry;

pub fn register_all(r: &mut Registry) {
    particles_1::register(r);
    particles_2::register(r);
    particles_3::register(r);
    particles_4::register(r);
    particles_5::register(r);
    particles_6::register(r);
    joints_1::register(r);
    joints_2::register(r);
    joints_3::register(r);
    effects_dk1::register(r);
    effects_dk2::register(r);
    skills_dk::register(r);
    skills_magic::register(r);
    objects_1::register(r);
    objects_2::register(r);
    objects_login::register(r);
    objects_elbeland::register(r);
    objects_kalima::register(r);
    objects_kanturu::register(r);
    objects_raklion::register(r);
    fire::register(r);
    effects_elf::register(r);
    effects_summoner::register(r);
    effects_wizard::register(r);
    effects_dl_mg1::register(r);
    effects_dl_mg2::register(r);
    effects_bitmaps1::register(r);
    effects_bitmaps2::register(r);
    effects_monk::register(r);
    effects_monsters1::register(r);
    effects_monsters2::register(r);
    effects_events::register(r);
}
