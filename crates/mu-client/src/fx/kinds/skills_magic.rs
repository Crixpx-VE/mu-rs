//! Character side of the classic magic skills, translated from `ZzzCharacter.cpp`: the default
//! `AttackStage` release (frame 5 of an attack action, 2835-2852), `AttackEffect`'s Lightning
//! (2183-2199) and the `switch` on reaching `g_iLimitAttackTime` (3932-4635 for the caster's own
//! effects, 4715-5070 for those that need the target): the Dark Wizard's spells, the Elf's heals and
//! buffs, and the Energy Ball/Fire Ball/Power Wave monsters use too.
//!
//! TODO(original): the buffs' state changes on the target (`g_CharacterRegisterBuff`), the hit blood
//! (`tc->Hit`, 4687-4698), the arrows (`CreateArrows`, 4637-4673), the
//! `m_bySkillSerialNum` on the joints, `AddWaterWave` in Hellas.

use bevy::math::Vec3;

use crate::fx::core::*;

/// The skill numbers handled here (SkillManager.h).
const POISON: i32 = 1;
const METEO: i32 = 2;
const THUNDER: i32 = 3;
const FIREBALL: i32 = 4;
const FLAME: i32 = 5;
const SLOW: i32 = 7;
const STORM: i32 = 8;
const EVIL: i32 = 9;
const HELL: i32 = 10;
const POWERWAVE: i32 = 11;
const FLASH: i32 = 12;
const BLAST: i32 = 13;
const INFERNO: i32 = 14;
const ENERGYBALL: i32 = 17;
const HEALING: i32 = 26;
const DEFENSE: i32 = 27;
const ATTACK: i32 = 28;
const BLAST_POISON: i32 = 38;
const BLAST_FREEZE: i32 = 39;
/// `AT_SKILL_HELL_FIRE_UP`, `EVIL_SPIRIT_UP`, `ICE_UP`, `HEAL_UP`, `DEF_POWER_UP`, `ATT_POWER_UP`,
/// `BLAST_UP`, `EVIL_SPIRIT_UP_M` (each +0..4).
const HELL_FIRE_UP: i32 = 440;
const EVIL_SPIRIT_UP: i32 = 445;
const ICE_UP: i32 = 450;
const HEAL_UP: i32 = 475;
const DEF_POWER_UP: i32 = 480;
const ATT_POWER_UP: i32 = 485;
const BLAST_UP: i32 = 510;
const EVIL_SPIRIT_UP_M: i32 = 530;

fn up(base: i32) -> std::ops::RangeInclusive<i32> {
    base..=base + 4
}

pub fn register(r: &mut Registry) {
    let k = SkillKind { stage: Some(stage), finish: Some(finish) };
    let mut all = vec![POISON, METEO, THUNDER, FIREBALL, FLAME, SLOW, STORM, EVIL, HELL, POWERWAVE, FLASH, BLAST, INFERNO, ENERGYBALL];
    all.extend([HEALING, DEFENSE, ATTACK, BLAST_POISON, BLAST_FREEZE]);
    for b in [HELL_FIRE_UP, EVIL_SPIRIT_UP, ICE_UP, HEAL_UP, DEF_POWER_UP, ATT_POWER_UP, BLAST_UP, EVIL_SPIRIT_UP_M] {
        all.extend(up(b));
    }
    for s in all {
        r.skill(s, k);
    }
}

/// `IsAttackAction`: a player in `PLAYER_ATTACK_FIST..=PLAYER_RIDE_SKILL`, a monster in
/// `MONSTER01_ATTACK1..=ATTACK2`.
fn attack_action(o: &Actor) -> bool {
    if o.ty == MODEL_PLAYER {
        (PLAYER_ATTACK_FIST..=PLAYER_RIDE_SKILL).contains(&(o.action as i32))
    } else {
        let a = o.action;
        a == mu_data::monsters::action::ATTACK1 || a == mu_data::monsters::action::ATTACK2
    }
}

/// `AttackStage`'s default case (ZzzCharacter.cpp:2835-2852) and `AttackEffect`'s Lightning.
fn stage(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let Some(o) = ctx.actor(owner).cloned() else { return };
    if c.skill == THUNDER {
        thunder(c, &o, ctx);
    }
    if o.frame >= 5.0 && attack_action(&o) {
        c.attack_time = LIMIT_ATTACK_TIME;
    }
}

/// `AttackEffect` with `Skill == AT_SKILL_THUNDER` and a target, the `default` case (2183-2199):
/// two lightning joints from the weapon bone to the target and a spark, every tick.
fn thunder(c: &Cast, o: &Actor, ctx: &mut Ctx) {
    let Some(to) = ctx.actor(c.target).map(|a| a.position) else { return };
    let p = if o.ty == MODEL_PLAYER { Vec3::ZERO } else { Vec3::new(0.0, -130.0, 0.0) };
    let position = o.bone_world(o.weapon_bone, p);
    let angle = Vec3::new(-60.0, 0.0, o.angle.z);
    ctx.create_joint(BITMAP_JOINT_THUNDER, position, to, angle, 0, c.target, 50.0);
    ctx.create_joint(BITMAP_JOINT_THUNDER, position, to, angle, 0, c.target, 10.0);
    ctx.create_particle(BITMAP_ENERGY, position, o.angle, Vec3::ONE, 0, 1.0, None);
}

/// `CalcAddPosition(o, x, y, z, Position)`: an offset turned by the character's angle.
fn add_position(o: &Actor, offset: Vec3) -> Vec3 {
    o.position + rotate(o.angle, offset)
}

/// The skill point `(SkillX + 0.5, SkillY + 0.5)` on the ground (the cast's target position).
fn skill_ground(c: &Cast, ctx: &Ctx) -> Vec3 {
    let p = c.target_position;
    Vec3::new(p.x, p.y, ctx.terrain_height(p.x, p.y))
}

/// `if (c == Hero) ++CharacterMachine->PacketSerial;` after the area spells' effects
/// (MuMain ZzzCharacter.cpp:4469-4472, 4488-4491, 4505-4508, 4586-4589, 4597-4600, 4618-4621).
fn hero_packet_serial(ctx: &mut Ctx, owner: Option<Target>) {
    if ctx.is_hero(owner) {
        ctx.next_packet_serial();
    }
}

fn finish(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let Some(o) = ctx.actor(owner).cloned() else { return };
    // TODO(original): o->PKKey = getTargetCharacterKey(c, SelectedCharacter): -1 for monsters.
    let pk_key = -1;
    let hot_key = ctx.find_hot_key(c.skill);
    // `Light` of `MoveCharacter` (1, 1, 1) and the character's own `o->Light`.
    let light = Vec3::ONE;
    let s = c.skill;
    // The caster's effects (3932-4635).
    match s {
        _ if s == HELL || up(HELL_FIRE_UP).contains(&s) => {
            ctx.create_effect(MODEL_CIRCLE, o.position, o.angle, o.light, 0, owner, pk_key, 0, hot_key, 0.0);
            ctx.create_effect(MODEL_CIRCLE_LIGHT, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_HELLFIRE");
        }
        BLAST_POISON => {
            let pos = skill_ground(c, ctx);
            let l = Vec3::new(0.8, 0.5, 0.1);
            ctx.create_effect(MODEL_FIRE, pos, o.angle, l, 6, None, 0, 0, 0, 0.0);
            ctx.create_effect(MODEL_FIRE, pos, o.angle, l, 6, None, 0, 0, 0, 0.0);
            ctx.play_sound("SOUND_DEATH_POISON1");
            hero_packet_serial(ctx, owner);
        }
        _ if s == BLAST_FREEZE || up(ICE_UP).contains(&s) => {
            let target = skill_ground(c, ctx);
            for i in 0..10 {
                let mut pos = target;
                pos.z += (ctx.rand() % 50) as f32 * i as f32;
                ctx.create_effect(MODEL_BLIZZARD, pos, o.angle, light, 0, None, i + 1, 0, 0, 0.0);
            }
            hero_packet_serial(ctx, owner);
            ctx.play_sound("SOUND_SUDDEN_ICE1");
        }
        FLAME => {
            let pos = skill_ground(c, ctx);
            ctx.create_effect(BITMAP_FLAME, pos, o.angle, o.light, 0, owner, pk_key, 0, ctx.find_hot_key(FLAME), 0.0);
            hero_packet_serial(ctx, owner);
            ctx.play_sound("SOUND_FLAME");
        }
        STORM => {
            ctx.create_effect(MODEL_STORM, o.position, o.angle, light, 0, owner, pk_key, 0, ctx.find_hot_key(STORM), 0.0);
            ctx.play_sound("SOUND_STORM");
        }
        FLASH => {
            let pos = add_position(&o, Vec3::new(-20.0, -90.0, 100.0));
            ctx.create_effect(BITMAP_BOSS_LASER, pos, o.angle, light, 0, owner, pk_key, 0, ctx.find_hot_key(FLASH), 0.0);
            ctx.play_sound("SOUND_FLASH");
        }
        _ if s == BLAST || up(BLAST_UP).contains(&s) => {
            let pos = skill_ground(c, ctx);
            ctx.create_effect(MODEL_SKILL_BLAST, pos, o.angle, o.light, 0, owner, pk_key, 0, hot_key, 0.0);
            ctx.create_effect(MODEL_SKILL_BLAST, pos, o.angle, o.light, 0, owner, pk_key, 0, hot_key, 0.0);
            hero_packet_serial(ctx, owner);
        }
        INFERNO => {
            crate::fx::kinds::common::create_inferno(ctx, o.position, 0);
            ctx.create_effect(MODEL_SKILL_INFERNO, o.position, o.angle, o.light, 0, owner, pk_key, 0, ctx.find_hot_key(INFERNO), 0.0);
            hero_packet_serial(ctx, owner);
        }
        _ if s == EVIL || up(EVIL_SPIRIT_UP).contains(&s) || up(EVIL_SPIRIT_UP_M).contains(&s) => {
            let mut pos = o.position;
            pos.z += 100.0;
            for i in 0..4 {
                let angle = Vec3::new(0.0, 0.0, i as f32 * 90.0);
                ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, pos, o.position, angle, 0, owner, 80.0, pk_key, hot_key);
                ctx.create_joint(BITMAP_JOINT_SPIRIT, pos, o.position, angle, 0, owner, 20.0);
            }
            hero_packet_serial(ctx, owner);
            ctx.play_sound("SOUND_EVIL");
        }
        _ => {}
    }
    // The target's effects (4715-5070), with `Angle[2]` toward it.
    let Some(to) = ctx.actor(c.target).cloned() else { return };
    let target = c.target;
    let mut angle = o.angle;
    angle.z = create_angle(o.position.x, o.position.y, to.position.x, to.position.y);
    match s {
        _ if s == HEALING || up(HEAL_UP).contains(&s) => {
            ctx.create_effect(BITMAP_MAGIC + 1, to.position, to.angle, to.light, 1, target, -1, 0, 0, 0.0);
        }
        _ if s == DEFENSE || up(DEF_POWER_UP).contains(&s) => {
            ctx.create_effect(BITMAP_MAGIC + 1, to.position, to.angle, to.light, 2, target, -1, 0, 0, 0.0);
        }
        _ if s == ATTACK || up(ATT_POWER_UP).contains(&s) => {
            ctx.create_effect(BITMAP_MAGIC + 1, to.position, to.angle, to.light, 3, target, -1, 0, 0, 0.0);
        }
        SLOW => {
            ctx.create_effect(MODEL_ICE, to.position, o.angle, light, 0, None, -1, 0, 0, 0.0);
            for _ in 0..5 {
                ctx.create_effect(MODEL_ICE_SMALL, to.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
            }
            ctx.play_sound("SOUND_ICE");
        }
        POISON => {
            if o.ty == MODEL_PLAYER {
                ctx.create_effect(MODEL_POISON, to.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
            }
            let l = Vec3::new(0.4, 0.6, 1.0);
            for _ in 0..10 {
                ctx.create_particle(BITMAP_SMOKE, to.position, o.angle, l, 1, 1.0, None);
            }
            ctx.play_sound("SOUND_HEART");
        }
        METEO => {
            ctx.create_effect(MODEL_FIRE, to.position, to.angle, o.light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_METEORITE01");
        }
        FIREBALL => {
            ctx.create_effect(MODEL_FIRE, o.position, angle, o.light, 1, target, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_METEORITE01");
        }
        FLAME => {
            let pos = Vec3::new(to.position.x, to.position.y, ctx.terrain_height(to.position.x, to.position.y));
            ctx.create_effect(BITMAP_FLAME, pos, o.angle, o.light, 5, owner, pk_key, 0, ctx.find_hot_key(FLAME), 0.0);
            ctx.play_sound("SOUND_FLAME");
        }
        POWERWAVE => {
            if o.ty == MODEL_MONSTER01 + 18 {
                for dz in [10.0, -20.0] {
                    angle.z += dz;
                    ctx.create_effect(MODEL_MAGIC2, o.position, angle, o.light, 0, None, -1, 0, 0, 0.0);
                }
                angle.z += 10.0;
            }
            ctx.create_effect(MODEL_MAGIC2, o.position, angle, o.light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_MAGIC");
        }
        ENERGYBALL => {
            // TODO(original): the monsters with their own projectile (4943-4992 by `MonsterIndex`).
            if o.ty == MODEL_MONSTER01 + 12 {
                ctx.create_effect(MODEL_SNOW1, o.position, angle, o.light, 0, target, -1, 0, 0, 0.0);
            } else {
                ctx.create_effect(BITMAP_ENERGY, o.position, angle, o.light, 0, target, -1, 0, 0, 0.0);
                ctx.play_sound("SOUND_MAGIC");
            }
        }
        _ => {}
    }
}

