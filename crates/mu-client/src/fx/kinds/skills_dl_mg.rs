//! Character side of the Dark Lord and Magic Gladiator skills, and the Dark Knight ones
//! `skills_dk` does not have (Strike of Destruction, Rush), translated from MuMain:
//!
//! - on receiving the skill (`ReceiveMagic` / `ReceiveMagicContinue`, WSclient.cpp:4432-5482): the
//!   sound and the effects the packet itself creates, done here on the first tick (`AttackTime = 1`);
//! - every tick (`AttackStage`, ZzzCharacter.cpp:2778-3015, plus `AttackEffect`'s `rand()`);
//! - on reaching `g_iLimitAttackTime` (the `switch` of `MoveCharacter`, ZzzCharacter.cpp:4163-4827
//!   for the caster, 4902-5084 for those that need the target).
//!
//! The actions (`SetAction`) are set by combat.rs / remote.rs, not here.
//!
//! TODO(original): `o->PKKey = getTargetCharacterKey(...)` (-1 here, as in the other skill files);
//! the left-hand weapon of Critical Damage / Brand of Skill (`c->Weapon[1]`, not in `Actor`);
//! the buffs these skills register or remove (`g_CharacterRegisterBuff`, `UnRegisterBuffList`);
//! Fire Slash's Skull on the target and its `PlaySpeed` change (only with `SkillSuccess`);
//! Invisibility's `DeleteJoint(MODEL_SPEARSKILL, to, 4 / 9)`.

use bevy::math::Vec3;

use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    let k = SkillKind { stage: Some(stage), finish: Some(finish) };
    for s in [
        // Magic Gladiator.
        AT_SKILL_FIRE_SLASH,
        AT_SKILL_FIRE_SLASH_STR,
        AT_SKILL_POWER_SLASH,
        AT_SKILL_POWER_SLASH_STR,
        AT_SKILL_SPIRAL_SLASH,
        AT_SKILL_FLAME_STRIKE,
        AT_SKILL_GIGANTIC_STORM,
        // Dark Lord.
        AT_SKILL_FORCE,
        AT_SKILL_FORCE_WAVE,
        AT_SKILL_FORCE_WAVE_STR,
        AT_SKILL_FIREBURST,
        AT_SKILL_FIREBURST_STR,
        AT_SKILL_FIREBURST_MASTERY,
        AT_SKILL_EARTHSHAKE,
        AT_SKILL_EARTHSHAKE_STR,
        AT_SKILL_EARTHSHAKE_MASTERY,
        AT_SKILL_PARTY_TELEPORT,
        AT_SKILL_ADD_CRITICAL,
        AT_SKILL_ADD_CRITICAL_STR1,
        AT_SKILL_ADD_CRITICAL_STR2,
        AT_SKILL_ADD_CRITICAL_STR3,
        AT_SKILL_THUNDER_STRIKE,
        AT_SKILL_STUN,
        AT_SKILL_REMOVAL_STUN,
        AT_SKILL_MANA,
        AT_SKILL_INVISIBLE,
        AT_SKILL_REMOVAL_INVISIBLE,
        AT_SKILL_REMOVAL_BUFF,
        AT_SKILL_FIRE_SCREAM,
        AT_SKILL_FIRE_SCREAM_STR,
        AT_SKILL_CHAOTIC_DISEIER,
        AT_SKILL_RIDER,
        // Castle siege skills.
        AT_SKILL_DEATH_CANNON,
        AT_SKILL_SPACE_SPLIT,
        AT_SKILL_BRAND_OF_SKILL,
        AT_SKILL_JAVELIN,
        AT_SKILL_OCCUPY,
        // Elf.
        AT_SKILL_IMPROVE_AG,
        // Dark Knight.
        AT_SKILL_STRIKE_OF_DESTRUCTION,
        AT_SKILL_STRIKE_OF_DESTRUCTION_STR,
        AT_SKILL_RUSH,
    ] {
        r.skill(s, k);
    }
}

// --- Helpers ---------------------------------------------------------------------------------------

/// `c->LastAttackEffectTime` (0 = none; `AttackTime` is never 0 while it lasts).
const LAST_EFFECT: usize = 7;

/// `c->CheckAttackTime(n)`.
fn check_attack_time(c: &Cast, n: i32) -> bool {
    c.attack_time == n && c.user[LAST_EFFECT] as i32 != n
}

/// `c->SetLastAttackEffectTime()`.
fn set_last_attack_effect_time(c: &mut Cast) {
    c.user[LAST_EFFECT] = c.attack_time as f32;
}

fn is_player(o: &Actor) -> bool {
    o.ty == MODEL_PLAYER
}

fn action_is(o: &Actor, actions: &[i32]) -> bool {
    actions.contains(&(o.action as i32))
}

/// The Dark Lord's strike actions, on foot, on a horse and on Fenrir.
const STRIKES: [i32; 3] = [PLAYER_ATTACK_STRIKE, PLAYER_ATTACK_RIDE_STRIKE, PLAYER_FENRIR_ATTACK_DARKLORD_STRIKE];

/// `IsAttackImpactFrame(o)` for a player (`IsAttackAction` and frame 5).
fn is_attack_impact_frame(o: &Actor) -> bool {
    o.frame >= 5.0 && (PLAYER_ATTACK_FIST..=PLAYER_RIDE_SKILL).contains(&(o.action as i32))
}

/// `CalcAddPosition(o, x, y, z, Position)`: an offset turned by the character's angle.
fn add_position(o: &Actor, offset: Vec3) -> Vec3 {
    o.position + rotate(o.angle, offset)
}

/// The skill point `(SkillX + 0.5, SkillY + 0.5) * TERRAIN_SCALE` (the cast's target position).
fn skill_point(c: &Cast) -> Vec3 {
    c.target_position
}

/// The `MODEL_DARKLORD_SKILL` pair on the weapons (`AT_SKILL_ADD_CRITICAL`, ZzzCharacter.cpp:4388-4399,
/// and `AT_SKILL_BRAND_OF_SKILL`, 4718-4729): the right hand unless it holds arrows. The left hand
/// (unless a bolt or a shield) needs `c->Weapon[1]`, which `Actor` does not have yet.
fn darklord_weapon_light(o: &Actor, light: Vec3, ctx: &mut Ctx) {
    if o.weapon.map(|w| w.0) != Some(MODEL_ARROWS) {
        let position = o.bone_world(o.weapon_bone, Vec3::ZERO);
        ctx.create_effect(MODEL_DARKLORD_SKILL, position, o.angle, light, 0, None, -1, 0, 0, 0.0);
    }
}

// --- On receiving the skill (first tick) --------------------------------------------------------------

/// What `ReceiveMagic` / `ReceiveMagicContinue` do besides the action and `AttackTime = 1`.
fn on_receive(c: &mut Cast, o: &Actor, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    match c.skill {
        // WSclient.cpp:4432-4449.
        AT_SKILL_FORCE | AT_SKILL_FORCE_WAVE | AT_SKILL_FORCE_WAVE_STR => ctx.play_sound("SOUND_SKILL_SWORD1"),
        // WSclient.cpp:4450-4467 and 4631-4646.
        AT_SKILL_FIREBURST | AT_SKILL_FIREBURST_STR | AT_SKILL_FIREBURST_MASTERY | AT_SKILL_SPACE_SPLIT => {
            ctx.play_sound("SOUND_ATTACK_FIRE_BUST");
        }
        // WSclient.cpp:4512-4517.
        AT_SKILL_POWER_SLASH | AT_SKILL_POWER_SLASH_STR => ctx.play_sound("SOUND_SKILL_SWORD4"),
        // WSclient.cpp:4601-4605 and 4667-4671.
        AT_SKILL_RUSH | AT_SKILL_SPIRAL_SLASH => ctx.play_sound("SOUND_SKILL_SWORD2"),
        // WSclient.cpp:4784-4807: the rune under the caster, `if (sc->SkillSuccess)`. The server
        // only sends the skill when it went through, so it is always drawn here.
        AT_SKILL_MANA => {
            ctx.create_effect(MODEL_MANA_RUNE, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
        }
        // WSclient.cpp:4842-4848.
        AT_SKILL_IMPROVE_AG => {
            if let Some(to) = ctx.actor(c.target).cloned() {
                ctx.create_effect(BITMAP_LIGHT, to.position, to.angle, to.light, 2, c.target, -1, 0, 0, 0.0);
            }
        }
        // WSclient.cpp:4949-4964 and 5344-5359.
        AT_SKILL_FIRE_SCREAM | AT_SKILL_FIRE_SCREAM_STR => ctx.play_sound("SOUND_FIRE_SCREAM"),
        // WSclient.cpp:5369-5374.
        AT_SKILL_EARTHSHAKE | AT_SKILL_EARTHSHAKE_STR | AT_SKILL_EARTHSHAKE_MASTERY => ctx.play_sound("SOUND_EARTH_QUAKE"),
        // WSclient.cpp:5070-5125 and 5433-5481.
        AT_SKILL_CHAOTIC_DISEIER => {
            // `P` is set to (0, -20, 0) and then to zero: the sparkles start at the caster.
            let light = Vec3::splat(0.5);
            for _ in 0..5 {
                ctx.create_effect(BITMAP_SHINY + 6, o.position, o.angle, light, 3, owner, -1, 0, 0, 0.5);
            }
            for _ in 0..8 {
                let mut position = o.position;
                position.x = (o.position.x - 119.0) + (ctx.rand() % 240) as f32;
                position.z = (o.position.z + 49.0) + (ctx.rand() % 60) as f32;
                ctx.create_joint_ex(BITMAP_2LINE_GHOST, position, o.position, o.angle, 0, owner, 20.0, -1, 0);
            }
            // `if (sc == Hero && SelectedCharacter != -1) CreateBomb(selected position, true)`.
            if ctx.is_hero(owner)
                && let Some(to) = ctx.actor(c.target).map(|a| a.position)
            {
                crate::fx::kinds::common::create_bomb(ctx, to, true, 0);
            }
            ctx.play_sound("SOUND_SKILL_CAOTIC");
        }
        _ => {}
    }
}

// --- Every tick (AttackStage + AttackEffect) -----------------------------------------------------------

fn stage(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    if let Some(o) = ctx.actor(owner).cloned() {
        if c.attack_time == 1 {
            on_receive(c, &o, ctx);
        }
        attack_stage(c, &o, ctx);
    }
    // AttackEffect: `Luminosity = rand() % 6 + 2` (does nothing else for these skills).
    let _ = ctx.rand();
}

/// `AttackStage`'s cases (ZzzCharacter.cpp:2778-3015). Setting `AttackTime = 15` releases the skill.
fn attack_stage(c: &mut Cast, o: &Actor, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let player = is_player(o);
    match c.skill {
        // 2778-2813.
        AT_SKILL_FIRE_SLASH | AT_SKILL_FIRE_SLASH_STR => {
            if player {
                if check_attack_time(c, 1) || check_attack_time(c, 2) {
                    ctx.create_effect(BITMAP_GATHERING, o.position, o.angle, o.light, 1, owner, -1, 0, 0, 0.0);
                    set_last_attack_effect_time(c);
                }
                if o.frame >= 3.0 && ctx.rand_fps_check(1) {
                    ctx.play_sound("SOUND_SKILL_SWORD3");
                    let sub = if c.skill == AT_SKILL_FIRE_SLASH_STR { 1 } else { 0 };
                    let hot_key = ctx.find_hot_key(c.skill);
                    ctx.create_effect(BITMAP_SWORD_FORCE, o.position, o.angle, o.light, sub, owner, -1, 0, hot_key, 0.0);
                    c.attack_time = LIMIT_ATTACK_TIME;
                }
            }
        }
        // 2815-2839: five waves fanned out.
        AT_SKILL_POWER_SLASH | AT_SKILL_POWER_SLASH_STR => {
            if player && o.action as i32 == PLAYER_ATTACK_TWO_HAND_SWORD_TWO && ctx.rand_fps_check(1) {
                let mut angle = o.angle;
                angle.z -= 40.0;
                for _ in 0..5 {
                    ctx.create_effect(MODEL_MAGIC2, o.position, angle, o.light, 2, owner, -1, 0, 0, 0.0);
                    angle.z += 20.0;
                }
                ctx.play_sound("SOUND_SKILL_SWORD3");
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2855-2860.
        AT_SKILL_IMPROVE_AG => {
            if is_attack_impact_frame(o) {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2861-2879 and 2976-2984.
        AT_SKILL_FORCE | AT_SKILL_FORCE_WAVE | AT_SKILL_FORCE_WAVE_STR | AT_SKILL_FIREBURST | AT_SKILL_FIREBURST_STR
        | AT_SKILL_FIREBURST_MASTERY | AT_SKILL_SPACE_SPLIT => {
            if o.frame >= 3.0 && player && action_is(o, &STRIKES) {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2880-2890: held at 10 until frame 5.5 of the flash.
        AT_SKILL_THUNDER_STRIKE => {
            if o.frame >= 5.5
                && player
                && action_is(o, &[PLAYER_SKILL_FLASH, PLAYER_ATTACK_RIDE_ATTACK_FLASH, PLAYER_FENRIR_ATTACK_DARKLORD_FLASH])
            {
                c.attack_time = LIMIT_ATTACK_TIME;
            } else {
                c.attack_time = 10;
            }
        }
        // 2891-2898.
        AT_SKILL_EARTHSHAKE | AT_SKILL_EARTHSHAKE_STR | AT_SKILL_EARTHSHAKE_MASTERY => {
            if o.frame >= 5.0 && player && o.action as i32 == PLAYER_ATTACK_DARKHORSE {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2899-2905.
        AT_SKILL_PARTY_TELEPORT => {
            if c.attack_time > 5
                && player
                && action_is(o, &[PLAYER_ATTACK_TELEPORT, PLAYER_ATTACK_RIDE_TELEPORT, PLAYER_FENRIR_ATTACK_DARKLORD_TELEPORT])
            {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2906-2911.
        AT_SKILL_RIDER => {
            if o.frame >= 5.0 && player && action_is(o, &[PLAYER_SKILL_RIDER, PLAYER_SKILL_RIDER_FLY]) {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2913-2915: they run their full time.
        AT_SKILL_STUN | AT_SKILL_MANA => {}
        // 2916-2922.
        AT_SKILL_INVISIBLE | AT_SKILL_REMOVAL_STUN | AT_SKILL_REMOVAL_INVISIBLE => c.attack_time = LIMIT_ATTACK_TIME,
        // 2923-2928.
        AT_SKILL_REMOVAL_BUFF => {
            if o.frame >= 3.5 {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2930-2955: sparks and fire at the feet every tick.
        AT_SKILL_RUSH | AT_SKILL_OCCUPY => {
            if o.frame > 5.0 {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
            if ctx.rand_fps_check(1) {
                let mut position = o.position;
                position.x += (ctx.rand() % 30) as f32 - 15.0;
                position.y += (ctx.rand() % 30) as f32 - 15.0;
                position.z += 20.0;
                let sub = if c.skill == AT_SKILL_OCCUPY { 18 } else { 2 };
                for _ in 0..4 {
                    let angle = Vec3::new((ctx.rand() % 60 + 60 + 90) as f32, 0.0, o.angle.z);
                    // `CreateJoint`'s default scale is 10 (ZzzCharacter.h:180).
                    ctx.create_joint(BITMAP_JOINT_SPARK, position, position, angle, 0, None, 10.0);
                    ctx.create_particle(BITMAP_FIRE, position, angle, o.light, sub, 1.5, None);
                }
            }
        }
        // 2956-2974: the flares of the one-flash slash.
        AT_SKILL_SPIRAL_SLASH => {
            if o.frame > 5.0 && ctx.rand_fps_check(1) {
                for pk in [0, 1, 4] {
                    ctx.create_joint_ex(BITMAP_FLARE, o.position, o.position, o.angle, 23, None, 40.0, pk, 0);
                }
                c.attack_time = LIMIT_ATTACK_TIME;
                ctx.play_sound("SOUND_BCS_ONE_FLASH");
            } else if o.frame > 2.3 && o.frame < 2.6 && ctx.rand_fps_check(1) {
                for pk in [2, 3] {
                    ctx.create_joint_ex(BITMAP_FLARE, o.position, o.position, o.angle, 23, None, 40.0, pk, 0);
                }
                ctx.play_sound("SOUND_BCS_ONE_FLASH");
            }
        }
        // 2986-2991.
        AT_SKILL_DEATH_CANNON => {
            if o.frame >= 3.0 && player && o.action as i32 == PLAYER_ATTACK_DEATH_CANNON {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // 2992-2996 and 3010-3015: released at once.
        AT_SKILL_FLAME_STRIKE | AT_SKILL_STRIKE_OF_DESTRUCTION | AT_SKILL_STRIKE_OF_DESTRUCTION_STR => {
            c.attack_time = LIMIT_ATTACK_TIME;
        }
        // 2997-3002.
        AT_SKILL_GIGANTIC_STORM => {
            if o.frame > 7.0 {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        // The `default` case (3040-3046): Fury Strike's first frame or the impact frame.
        _ => {
            let fury = o.frame >= 1.0 && player && o.action as i32 == PLAYER_ATTACK_SKILL_FURY_STRIKE;
            if fury || is_attack_impact_frame(o) {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
    }
}

// --- On reaching g_iLimitAttackTime ------------------------------------------------------------------

fn finish(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let Some(o) = ctx.actor(owner).cloned() else { return };
    c.user[LAST_EFFECT] = 0.0;
    let pk_key = -1;
    let hot_key = ctx.find_hot_key(c.skill);
    // `Light` of `MoveCharacter`.
    let light = Vec3::ONE;
    let target = ctx.actor(c.target).cloned();

    // The caster's effects (4163-4827).
    match c.skill {
        // 4187-4197: `o->Light` carries the skill point.
        AT_SKILL_STRIKE_OF_DESTRUCTION | AT_SKILL_STRIKE_OF_DESTRUCTION_STR => {
            let p = skill_point(c);
            let at = Vec3::new(p.x, p.y, o.position.z);
            ctx.create_effect(MODEL_BLOW_OF_DESTRUCTION, o.position, o.angle, at, 0, owner, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_SKILL_BLOWOFDESTRUCTION");
        }
        // 4232-4237.
        AT_SKILL_STUN => {
            ctx.create_joint(BITMAP_FLASH, o.position, o.position, o.angle, 7, None, 10.0);
            ctx.play_sound("SOUND_BMS_STUN");
        }
        // 4239-4255 and 4295-4312: a flash falling on the target.
        AT_SKILL_REMOVAL_STUN | AT_SKILL_REMOVAL_INVISIBLE => {
            if let Some(to) = target.as_ref()
                && c.target != owner
            {
                let mut position = to.position;
                position.z += 1200.0;
                let sub = if c.skill == AT_SKILL_REMOVAL_STUN { 0 } else { 1 };
                ctx.create_joint(BITMAP_FLASH, position, position, to.angle, sub, c.target, 120.0);
                ctx.play_sound("SOUND_BMS_STUN_REMOVAL");
            }
        }
        // 4257-4276.
        AT_SKILL_MANA => {
            for i in 0..36 {
                let angle = Vec3::new(-10.0, 0.0, i as f32 * 10.0);
                let mut position = o.position;
                position.z += 100.0;
                ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, position, position, angle, 21, owner, 60.0, 0, 0);
                if i % 20 == 0 {
                    ctx.create_effect(BITMAP_MAGIC + 1, o.position, angle, light, 10, owner, -1, 0, 0, 0.0);
                }
            }
            ctx.play_sound("SOUND_BMS_MANA");
        }
        // 4278-4294.
        AT_SKILL_INVISIBLE => {
            if let Some(to) = target.as_ref() {
                ctx.create_effect(BITMAP_MAGIC + 1, to.position, to.angle, to.light, 6, c.target, -1, 0, 0, 0.0);
                ctx.play_sound("SOUND_BMS_INVISIBLE");
            }
        }
        // 4313-4360: six spears around the caster.
        AT_SKILL_REMOVAL_BUFF => {
            let mut position = o.position;
            position.z += 100.0;
            for (yaw, sub, lower) in [(45.0, 5, false), (135.0, 6, true), (225.0, 7, true), (90.0, 5, false), (180.0, 6, true), (270.0, 7, true)] {
                if lower {
                    position.z -= 10.0;
                }
                ctx.create_joint(MODEL_SPEARSKILL, position, position, Vec3::new(0.0, 0.0, yaw), sub, owner, 170.0);
            }
            ctx.play_sound("SOUND_BMS_MAGIC_REMOVAL");
        }
        // 4361-4383 (`sinf(45 * 0.1f)`: radians, as written).
        AT_SKILL_IMPROVE_AG => {
            let mut angle = Vec3::new(-45.0, 0.0, 45.0);
            let mut position = Vec3::new(
                o.position.x + (45.0f32 * 0.1).sin() * 80.0,
                o.position.y + (45.0f32 * 0.1).cos() * 80.0,
                o.position.z + 300.0,
            );
            ctx.create_joint(BITMAP_JOINT_HEALING, position, position, angle, 10, owner, 15.0);
            angle.z = 405.0;
            ctx.create_joint(BITMAP_JOINT_HEALING, position, position, angle, 10, owner, 15.0);
            angle.z = 225.0;
            position = Vec3::new(
                o.position.x + (225.0f32 * 0.1).cos() * 80.0,
                o.position.y + (225.0f32 * 0.1).sin() * 80.0,
                o.position.z + 300.0,
            );
            ctx.create_joint(BITMAP_JOINT_HEALING, position, position, angle, 10, owner, 15.0);
            angle.z = 450.0;
            ctx.create_joint(BITMAP_JOINT_HEALING, position, position, angle, 10, owner, 15.0);
            ctx.play_sound("SOUND_SWELLLIFE");
        }
        // 4384-4401.
        AT_SKILL_ADD_CRITICAL | AT_SKILL_ADD_CRITICAL_STR1 | AT_SKILL_ADD_CRITICAL_STR2 | AT_SKILL_ADD_CRITICAL_STR3 => {
            darklord_weapon_light(&o, Vec3::new(1.0, 0.6, 0.3), ctx);
            ctx.play_sound("SOUND_CRITICAL");
        }
        // 4402-4407.
        AT_SKILL_PARTY_TELEPORT => {
            ctx.create_effect(MODEL_CIRCLE, o.position, o.angle, o.light, 2, owner, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_CIRCLE_LIGHT, o.position, o.angle, o.light, 3, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_PART_TELEPORT");
        }
        // 4408-4422.
        AT_SKILL_THUNDER_STRIKE => {
            let mut position = add_position(&o, Vec3::new(0.0, -90.0, -50.0));
            if o.action as i32 == PLAYER_ATTACK_RIDE_ATTACK_FLASH {
                position.z += 80.0;
            } else if o.action as i32 == PLAYER_FENRIR_ATTACK_DARKLORD_FLASH {
                position.z += 40.0;
            }
            ctx.create_effect(BITMAP_FLARE_FORCE, position, o.angle, o.light, 0, owner, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_ELEC_STRIKE");
        }
        // 4423-4427 (`SkillIndex` = `CurrentSkill`, the hot key slot).
        AT_SKILL_RIDER => {
            ctx.create_effect(BITMAP_SHOTGUN, o.position, o.angle, o.light, 0, owner, pk_key, 0, hot_key, 0.0);
            ctx.play_sound("SOUND_SKILL_SWORD3");
        }
        // 4517-4567: three screams, straight and turned ±10° from 80 units aside.
        AT_SKILL_FIRE_SCREAM | AT_SKILL_FIRE_SCREAM_STR => {
            let scream = |ctx: &mut Ctx, position: Vec3, angle: Vec3| {
                ctx.create_effect(MODEL_DARK_SCREAM, position, angle, o.light, 0, owner, pk_key, 0, hot_key, 0.0);
                ctx.create_effect(MODEL_DARK_SCREAM_FIRE, position, angle, o.light, 0, owner, pk_key, 0, hot_key, 0.0);
            };
            scream(ctx, o.position, o.angle);
            let mut angle = o.angle;
            angle.z += 10.0;
            scream(ctx, o.position + rotate(angle, Vec3::new(80.0, 0.0, 0.0)), angle);
            angle.z -= 20.0;
            scream(ctx, o.position + rotate(angle, Vec3::new(-80.0, 0.0, 0.0)), angle);
            ctx.play_sound("SOUND_FIRE_SCREAM");
        }
        // 4704-4713.
        AT_SKILL_RUSH => {
            ctx.create_effect(MODEL_SWORD_FORCE, o.position, o.angle, light, 0, owner, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_BCS_RUSH");
        }
        AT_SKILL_OCCUPY => {
            ctx.create_effect(MODEL_SWORD_FORCE, o.position, o.angle, light, 2, owner, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_BCS_RUSH");
        }
        // 4717-4733.
        AT_SKILL_BRAND_OF_SKILL => {
            darklord_weapon_light(&o, Vec3::ONE, ctx);
            ctx.create_effect(MODEL_MANA_RUNE, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_BCS_BRAND_OF_SKILL");
        }
        // 4772-4778.
        AT_SKILL_FLAME_STRIKE => {
            ctx.create_effect(MODEL_EFFECT_FLAME_STRIKE, o.position, o.angle, o.light, 0, owner, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_SKILL_FLAME_STRIKE");
        }
        // 4779-4797: five bolts in a circle of 200 around the caster.
        AT_SKILL_GIGANTIC_STORM => {
            for i in 0..5 {
                let turn = Vec3::new(0.0, 0.0, o.angle.z + i as f32 * 72.0);
                let position = o.position + rotate(turn, Vec3::new(0.0, 200.0, 0.0));
                ctx.create_effect(BITMAP_JOINT_THUNDER, position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
            }
            ctx.play_sound("SOUND_SKILL_GIGANTIC_STORM");
        }
        _ => {}
    }

    // The target's effects (4902-5084), only with a target.
    let Some(to) = target else { return };
    match c.skill {
        // 5010-5016.
        AT_SKILL_JAVELIN => {
            for sub in 0..3 {
                ctx.create_effect(MODEL_SKILL_JAVELIN, o.position, o.angle, o.light, sub, c.target, -1, 0, 0, 0.0);
            }
            ctx.play_sound("SOUND_BCS_JAVELIN");
        }
        // 5017-5025.
        AT_SKILL_DEATH_CANNON => {
            let mut position = o.position;
            position.z += 130.0;
            ctx.create_joint(BITMAP_JOINT_FORCE, position, position, Vec3::new(0.0, 0.0, o.angle.z), 4, None, 40.0);
            ctx.play_sound("SOUND_BCS_DEATH_CANON");
        }
        // 5026-5029.
        AT_SKILL_SPACE_SPLIT => {
            ctx.create_effect(MODEL_PIER_PART, o.position, o.angle, o.light, 2, c.target, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_BCS_SPACE_SPLIT");
        }
        // 5055-5062.
        AT_SKILL_FORCE | AT_SKILL_FORCE_WAVE | AT_SKILL_FORCE_WAVE_STR => {
            ctx.create_effect(MODEL_WAVES, o.position, o.angle, o.light, 1, None, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_WAVES, o.position, o.angle, o.light, 1, None, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_PIERCING2, o.position, o.angle, o.light, 0, None, -1, 0, 0, 0.0);
            ctx.play_sound("SOUND_ATTACK_SPEAR");
        }
        // 5063-5084: three homing spirits from the caster's root bone to the target's height
        // (`Light` carries the offset), and the Dark Lord sparks.
        AT_SKILL_FIREBURST | AT_SKILL_FIREBURST_STR | AT_SKILL_FIREBURST_MASTERY => {
            let position = o.bone_world(0, Vec3::new(40.0, 0.0, 10.0));
            let mut height = Vec3::new(0.0, 0.0, to.bbox_max.z);
            let yaw = |d: f32| Vec3::new(0.0, 0.0, o.angle.z + d);
            ctx.create_effect(MODEL_PIER_PART, position, yaw(90.0), height, 0, c.target, -1, 0, 0, 0.0);
            height.z -= to.bbox_max.z / 2.0;
            ctx.create_effect(MODEL_PIER_PART, position, yaw(0.0), height, 0, c.target, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_PIER_PART, position, yaw(-90.0), height, 0, c.target, -1, 0, 0, 0.0);
            let spark = Vec3::new(1.0, 0.6, 0.3);
            ctx.create_effect(MODEL_DARKLORD_SKILL, position, o.angle, spark, 0, None, -1, 0, 0, 0.0);
            ctx.create_effect(MODEL_DARKLORD_SKILL, position, o.angle, spark, 1, None, -1, 0, 0, 0.0);
        }
        _ => {}
    }
}
