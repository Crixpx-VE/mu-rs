//! Character side of the Dark Knight skills, translated from `ZzzCharacter.cpp`
//! (`AttackStage`, `AttackEffect` and the `switch` on reaching `g_iLimitAttackTime`), with the visuals of
//! `UseSkillWarrior` (`SkillCast.cpp`) and `AttackKnight` (`ClassAttack.cpp`) on cast:
//! Twisting Slash, Rageful Blow, Death Stab, Impale, Swell Life and the sword skills (19-23).

use bevy::math::Vec3;

use crate::fx::core::*;

pub fn register(r: &mut Registry) {
    let k = SkillKind { stage: Some(stage), finish: Some(finish) };
    for s in [
        AT_SKILL_TWISTING_SLASH,
        AT_SKILL_RAGEFUL_BLOW,
        AT_SKILL_DEATHSTAB,
        AT_SKILL_IMPALE,
        AT_SKILL_SWELL_LIFE,
        AT_SKILL_FALLING_SLASH,
        AT_SKILL_LUNGE,
        AT_SKILL_UPPERCUT,
        AT_SKILL_CYCLONE,
        AT_SKILL_SLASH,
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

/// `GetNearRandomPos(Pos, iRange, NewPos)`.
fn near_random_pos(ctx: &mut Ctx, pos: Vec3, range: i32) -> Vec3 {
    let m = range * 2 + 1;
    let x = (ctx.rand() % m - range) as f32;
    let y = (ctx.rand() % m - range) as f32;
    let z = (ctx.rand() % m - range) as f32;
    pos + Vec3::new(x, y, z)
}

/// `Engine::Object::IsAttackAction`.
fn is_attack_action(action: usize) -> bool {
    (PLAYER_ATTACK_FIST..=PLAYER_RIDE_SKILL).contains(&(action as i32))
}

/// `IsAttackImpactFrame(o)` for a player.
fn is_attack_impact_frame(a: &Actor) -> bool {
    a.frame >= 5.0 && is_attack_action(a.action)
}

/// `(sin, cos)` of the heading (`o->Angle[2]`).
fn heading(a: &Actor) -> (f32, f32) {
    let r = a.angle.z.to_radians();
    (r.sin(), r.cos())
}

/// `c->Weapon[Hand].Type` (-1 without a weapon).
fn weapon_type(a: &Actor) -> i32 {
    a.weapon.map_or(-1, |w| w.0)
}

/// Skills that go through `UseSkillWarrior` (the rest go through `AttackKnight`).
fn uses_skill_warrior(skill: i32) -> bool {
    matches!(
        skill,
        AT_SKILL_FALLING_SLASH | AT_SKILL_LUNGE | AT_SKILL_UPPERCUT | AT_SKILL_CYCLONE | AT_SKILL_SLASH | AT_SKILL_DEATHSTAB | AT_SKILL_IMPALE
    )
}

// --- Every tick (AttackStage + AttackEffect) -----------------------------------------------------------

fn stage(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let actor = ctx.actor(owner).cloned();

    if let Some(o) = actor.as_ref() {
        // On cast: the visuals of UseSkillWarrior / AttackKnight.
        if c.attack_time == 1 {
            launch(c, o, ctx);
        }
        attack_stage(c, o, ctx);
    }

    // AttackEffect: `Luminosity = rand() % 6 + 2` (does nothing else for a player).
    let _ = ctx.rand();

    // The original does `AttackTime += 1` and checks the limit in the same tick: so `finish`
    // runs in the tick where AttackStage saw 14 (or the jump to 15), not one later.
    if c.attack_time + 1 >= LIMIT_ATTACK_TIME {
        c.attack_time = LIMIT_ATTACK_TIME;
    }
}

/// What `UseSkillWarrior` / `AttackKnight` do visually on cast.
fn launch(c: &mut Cast, o: &Actor, ctx: &mut Ctx) {
    // TODO(original): SetAttackSpeed/SetAction, turning toward TargetPosition, LetHeroStop and
    // SendRequestMagic(Continue) are handled by character.rs / the network.
    if uses_skill_warrior(c.skill) {
        let owner = Some(Target::Actor(c.owner));
        ctx.create_particle(BITMAP_SHINY + 2, o.position, o.angle, Vec3::ONE, 0, 0.0, owner);
        let swing = if ctx.rand() % 2 == 0 { "SOUND_BRANDISH_SWORD01" } else { "SOUND_BRANDISH_SWORD02" };
        ctx.play_sound(swing);
    }
}

fn attack_stage(c: &mut Cast, o: &Actor, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    // GetHandOfWeapon: skill actions are never left-hand actions → Hand = 0.
    let bone = o.weapon_bone;
    let (s, co) = heading(o);

    match c.skill {
        AT_SKILL_DEATHSTAB => {
            // TODO(original): also bails out if the bone is Dummy (unknown here).
            if bone >= o.bones.len() {
                return;
            }
            if check_attack_time(c, 8) {
                ctx.play_sound("SOUND_SKILL_SWORD2");
                set_last_attack_effect_time(c);
            }
            if (2..=8).contains(&c.attack_time) && ctx.rand_fps_check(1) {
                // Wind gathering on the sword from behind.
                for _ in 0..3 {
                    let mut cur = o.position;
                    cur.z += 120.0;
                    let mut temp = near_random_pos(ctx, cur, 300);
                    let dist = 1400.0;
                    temp.x += -dist * s;
                    temp.y += dist * co;
                    ctx.create_joint(MODEL_SPEARSKILL, temp, temp, o.angle, 2, owner, 40.0);
                }
            }
            if c.attack_time <= 8 && ctx.rand_fps_check(1) {
                // Point where the wind gathers (m_vPosSword).
                c.pos_sword = o.bone_world(bone, Vec3::ZERO);
                let dist = 300.0;
                c.pos_sword.x += dist * s;
                c.pos_sword.y += -dist * co;
            }
            if (6..=12).contains(&c.attack_time) {
                // Thrust cones.
                if ctx.rand_fps_check(1) {
                    let mut pos = o.bone_world(bone, Vec3::ZERO);
                    let dist = 100.0 + (c.attack_time - 8) as f32 * 10.0;
                    pos.x += dist * s;
                    pos.y += -dist * co;
                    ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 1, owner, -1, 0, 0, 0.0);
                    ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 1, owner, -1, 0, 0, 0.0);
                }
                // TODO(original): if there is a living TargetCharacter and AttackTime >= 10,
                // to->m_byHurtByDeathstab = 35 (lightning along the target's bones; no monsters here).
            }
            if c.attack_time >= 12 {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        AT_SKILL_IMPALE => {
            if check_attack_time(c, 10) {
                ctx.play_sound("SOUND_RIDINGSPEAR");
                set_last_attack_effect_time(c);
            } else if check_attack_time(c, 4) {
                // Wind-up: the spear in the hand.
                let p = o.bone_world(bone, Vec3::ZERO);
                ctx.create_effect(MODEL__SPEAR, p, o.angle, Vec3::new(1.0, 1.0, 0.5), weapon_type(o), owner, -1, 0, 0, 0.0);
                set_last_attack_effect_time(c);
            } else if check_attack_time(c, 8) {
                // Cones in front.
                let mut pos = o.position;
                pos.x += 50.0 * s;
                pos.y += -50.0 * co;
                pos.z += 110.0;
                ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 0, owner, -1, 0, 0, 0.0);
                ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 0, owner, -1, 0, 0, 0.0);
                set_last_attack_effect_time(c);
            }
            if (13..=14).contains(&c.attack_time) && ctx.rand_fps_check(1) {
                // Rapid spear thrusts.
                for _ in 0..3 {
                    let mut pos = o.position;
                    pos.x += 145.0 * s;
                    pos.y += -145.0 * co;
                    pos.z += 110.0;
                    let light = Vec3::splat(0.3);
                    if check_attack_time(c, 11) {
                        // (empty in the original)
                    } else {
                        pos.x += (ctx.rand() % 60 - 30) as f32;
                        pos.y += (ctx.rand() % 60 - 30) as f32;
                        pos.z += (ctx.rand() % 60 - 30) as f32;
                    }
                    ctx.create_effect(MODEL_SPEARSKILL, pos, o.angle, light, weapon_type(o), owner, -1, 0, 0, 0.0);
                }
            }
        }
        AT_SKILL_SWELL_LIFE => {
            if c.attack_time > 9 && o.action == PLAYER_SKILL_VITALITY as usize {
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
        _ => {
            // default: Rageful Blow, Twisting Slash and the sword skills.
            if o.frame >= 1.0 && o.action == PLAYER_ATTACK_SKILL_FURY_STRIKE as usize {
                c.attack_time = LIMIT_ATTACK_TIME;
            } else if is_attack_impact_frame(o) {
                // TODO(original): BITMAP_SPARK+1 sparks with the ITEM_SYLPH_WIND_BOW bow (not a DK thing).
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
    // TODO(original): o->PKKey = getTargetCharacterKey(c, SelectedCharacter) (no targets → -1).
    let pk_key = -1;
    // `FindHotKey(Skill)`: the slot the hits are reported with.
    let hot_key = ctx.find_hot_key(c.skill);
    // The character's o->Light: not available, white.
    let light = Vec3::ONE;

    match c.skill {
        AT_SKILL_RAGEFUL_BLOW => {
            // o->Weapon/WeaponLevel = right-hand weapon: the effect reads it from the owner's `Actor::weapon`.
            ctx.create_effect(MODEL_SKILL_FURY_STRIKE, o.position, o.angle, light, 0, owner, pk_key, 0, hot_key, 0.0);
            ctx.play_sound("SOUND_FURY_STRIKE1");
        }
        AT_SKILL_SWELL_LIFE => {
            let count = 36;
            for i in 0..count {
                let angle = Vec3::new(-10.0, 0.0, i as f32 * 10.0);
                let mut pos = o.position;
                pos.z += 100.0;
                ctx.create_joint_ex(BITMAP_JOINT_SPIRIT, pos, pos, angle, 2, owner, 60.0, 0, 0);
                if i % 20 == 0 {
                    ctx.create_effect(BITMAP_MAGIC + 1, o.position, angle, light, 4, owner, -1, 0, 0, 0.0);
                }
            }
            ctx.play_sound("SOUND_SWELLLIFE");
        }
        AT_SKILL_TWISTING_SLASH => {
            // o->Weapon/WeaponLevel = right-hand weapon: the effect reads it from the owner's `Actor::weapon`.
            ctx.create_effect(MODEL_SKILL_WHEEL1, o.position, o.angle, light, 0, owner, pk_key, 0, hot_key, 0.0);
            ctx.play_sound("SOUND_SKILL_SWORD4");
            // TODO(original): c->PostMoveProcess_Active(g_iLimitAttackTime).
        }
        _ => {}
    }

    // TODO(original): with TargetCharacter == -1 only arrows (bows) and PlayUntargetedAttackHitSound.
    // With a target: BITMAP_BLOOD+1 blood on Hit, CreateSpark for PLAYER_ATTACK_SKILL_SWORD1..5,
    // BITMAP_JOINT_ENERGY on ATTACK_DIE and the hit sound (no monsters or damage yet).
}
