//! Code for each effect kind, translated from MuMain and Main 5.2. Each file is one family and has
//! `pub fn register(r: &mut Registry)`, which registers its kinds.
//!
//! Coverage (Main 5.2 with the Rage Fighter code, as MuMain): every `case` type of
//! `CreateParticle`/`MoveParticles` (98), `CreateJoint`/`MoveJoint` (31) and
//! `CreateEffect`/`MoveEffect` (447) is registered; `fx::smoke_tests` runs them all.
//! Client-side hits (`CheckCharacterRange`, `CheckClientArrow`, `CheckTargetRange`, the range
//! test of `AttackCharacterRange`), effect models' own bones/vertices/action lengths, the
//! `RENDER_*` modes and the per-map models are in. What remains is marked in the kinds and waits
//! for systems outside the effects engine:
//! - `TODO(combat)`: `SendRequestMagicAttack` (requests are queued in `FxWorld::attack_requests`),
//!   `CharacterMachine->PacketSerial`, `Hero->CurrentSkill`/`Object.Weapon`; the client does not
//!   fill `FxWorld::hero_skills` (`CharacterAttribute->Skill[]`) yet;
//! - data the client does not track yet (the plumbing reads it): buffs, `m_sTargetIndex`,
//!   `m_bySkillCount`, attack/magic speed, a character owner's `LifeTime`, prior action/frame for
//!   sub-frame poses, terrain light and the render-level option while drawing;
//! - writes to characters (`o->Owner->Alpha/Angle/Velocity`, `SetPlayerStop`, shock actions):
//!   actors are read-only snapshots;
//! - object blurs on effects (`CreateObjectBlur`/`RemoveObjectBlurs`), water waves and water-surface
//!   marks (`AddWaterWave`, `RenderWaterTerrain`), `StopBuffer`, positional sounds, Castle Siege and
//!   Kanturu event state, joints or map objects as effect owners, `RenderPartObjectEdge` bone scale,
//!   `RENDER_OIL` and `RENDER_DOPPELGANGER` culling;
//! - `MODEL_BOSS_ATTACK`, `MODEL_BLOOD` and `MODEL_MONSTER01+64`: no `.bmd` is loaded for them in
//!   either reference.

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
