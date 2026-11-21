//! Parte de personaje de las habilidades del Dark Knight, traducida de `ZzzCharacter.cpp`
//! (`AttackStage`, `AttackEffect` y el `switch` al llegar a `g_iLimitAttackTime`), con lo visual de
//! `UseSkillWarrior` (`SkillCast.cpp`) y `AttackKnight` (`ClassAttack.cpp`) al lanzarlas:
//! Twisting Slash, Rageful Blow, Death Stab, Impale, Swell Life y las de espada (19-23).

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

// --- Utilidades ----------------------------------------------------------------------------------

/// `c->LastAttackEffectTime` (0 = ninguno, `AttackTime` nunca vale 0 mientras dura).
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

/// `IsAttackImpactFrame(o)` para un jugador.
fn is_attack_impact_frame(a: &Actor) -> bool {
    a.frame >= 5.0 && is_attack_action(a.action)
}

/// `(sin, cos)` del rumbo (`o->Angle[2]`).
fn heading(a: &Actor) -> (f32, f32) {
    let r = a.angle.z.to_radians();
    (r.sin(), r.cos())
}

/// `c->Weapon[Hand].Type` (-1 sin arma).
fn weapon_type(a: &Actor) -> i32 {
    a.weapon.map_or(-1, |w| w.0)
}

/// Habilidades que pasan por `UseSkillWarrior` (las demás van por `AttackKnight`).
fn uses_skill_warrior(skill: i32) -> bool {
    matches!(
        skill,
        AT_SKILL_FALLING_SLASH | AT_SKILL_LUNGE | AT_SKILL_UPPERCUT | AT_SKILL_CYCLONE | AT_SKILL_SLASH | AT_SKILL_DEATHSTAB | AT_SKILL_IMPALE
    )
}

// --- Cada tick (AttackStage + AttackEffect) ------------------------------------------------------------

fn stage(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let actor = ctx.actor(owner).cloned();

    if let Some(o) = actor.as_ref() {
        // Al lanzar: lo visual de UseSkillWarrior / AttackKnight.
        if c.attack_time == 1 {
            launch(c, o, ctx);
        }
        attack_stage(c, o, ctx);
    }

    // AttackEffect: `Luminosity = rand() % 6 + 2` (para un jugador no hace nada más).
    let _ = ctx.rand();

    // El original hace `AttackTime += 1` y comprueba el límite en el mismo tick: así `finish`
    // corre en el tick en que AttackStage vio 14 (o el salto a 15), no uno después.
    if c.attack_time + 1 >= LIMIT_ATTACK_TIME {
        c.attack_time = LIMIT_ATTACK_TIME;
    }
}

/// Lo que hacen `UseSkillWarrior` / `AttackKnight` en lo visual al lanzar.
fn launch(c: &mut Cast, o: &Actor, ctx: &mut Ctx) {
    // TODO(original): SetAttackSpeed/SetAction, giro hacia TargetPosition, LetHeroStop y
    // SendRequestMagic(Continue) los hace character.rs / la red.
    if uses_skill_warrior(c.skill) {
        let owner = Some(Target::Actor(c.owner));
        ctx.create_particle(BITMAP_SHINY + 2, o.position, o.angle, Vec3::ONE, 0, 0.0, owner);
        let swing = if ctx.rand() % 2 == 0 { "SOUND_BRANDISH_SWORD01" } else { "SOUND_BRANDISH_SWORD02" };
        ctx.play_sound(swing);
    }
}

fn attack_stage(c: &mut Cast, o: &Actor, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    // GetHandOfWeapon: las acciones de habilidad nunca son las de la mano izquierda → Hand = 0.
    let bone = o.weapon_bone;
    let (s, co) = heading(o);

    match c.skill {
        AT_SKILL_DEATHSTAB => {
            // TODO(original): también se corta si el hueso es Dummy (no lo sabemos).
            if bone >= o.bones.len() {
                return;
            }
            if check_attack_time(c, 8) {
                ctx.play_sound("SOUND_SKILL_SWORD2");
                set_last_attack_effect_time(c);
            }
            if (2..=8).contains(&c.attack_time) && ctx.rand_fps_check(1) {
                // Viento que se junta en la espada desde atrás.
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
                // Punto donde se junta el viento (m_vPosSword).
                c.pos_sword = o.bone_world(bone, Vec3::ZERO);
                let dist = 300.0;
                c.pos_sword.x += dist * s;
                c.pos_sword.y += -dist * co;
            }
            if (6..=12).contains(&c.attack_time) {
                // Conos de la estocada.
                if ctx.rand_fps_check(1) {
                    let mut pos = o.bone_world(bone, Vec3::ZERO);
                    let dist = 100.0 + (c.attack_time - 8) as f32 * 10.0;
                    pos.x += dist * s;
                    pos.y += -dist * co;
                    ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 1, owner, -1, 0, 0, 0.0);
                    ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 1, owner, -1, 0, 0, 0.0);
                }
                // TODO(original): si hay TargetCharacter vivo y AttackTime >= 10,
                // to->m_byHurtByDeathstab = 35 (rayos por los huesos del objetivo; no hay monstruos).
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
                // Preparación: la lanza en la mano.
                let p = o.bone_world(bone, Vec3::ZERO);
                ctx.create_effect(MODEL__SPEAR, p, o.angle, Vec3::new(1.0, 1.0, 0.5), weapon_type(o), owner, -1, 0, 0, 0.0);
                set_last_attack_effect_time(c);
            } else if check_attack_time(c, 8) {
                // Conos delante.
                let mut pos = o.position;
                pos.x += 50.0 * s;
                pos.y += -50.0 * co;
                pos.z += 110.0;
                ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 0, owner, -1, 0, 0, 0.0);
                ctx.create_effect(MODEL_SPEAR, pos, o.angle, Vec3::ONE, 0, owner, -1, 0, 0, 0.0);
                set_last_attack_effect_time(c);
            }
            if (13..=14).contains(&c.attack_time) && ctx.rand_fps_check(1) {
                // Lanzazos rápidos.
                for _ in 0..3 {
                    let mut pos = o.position;
                    pos.x += 145.0 * s;
                    pos.y += -145.0 * co;
                    pos.z += 110.0;
                    let light = Vec3::splat(0.3);
                    if check_attack_time(c, 11) {
                        // (vacío en el original)
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
            // default: Rageful Blow, Twisting Slash y las de espada.
            if o.frame >= 1.0 && o.action == PLAYER_ATTACK_SKILL_FURY_STRIKE as usize {
                c.attack_time = LIMIT_ATTACK_TIME;
            } else if is_attack_impact_frame(o) {
                // TODO(original): chispas BITMAP_SPARK+1 con el arco ITEM_SYLPH_WIND_BOW (no es del DK).
                c.attack_time = LIMIT_ATTACK_TIME;
            }
        }
    }
}

// --- Al llegar a g_iLimitAttackTime -------------------------------------------------------------------

fn finish(c: &mut Cast, ctx: &mut Ctx) {
    let owner = Some(Target::Actor(c.owner));
    let Some(o) = ctx.actor(owner).cloned() else { return };
    c.user[LAST_EFFECT] = 0.0;
    // TODO(original): o->PKKey = getTargetCharacterKey(c, SelectedCharacter) (sin objetivos → -1).
    let pk_key = -1;
    // TODO(original): FindHotKey(Skill) (índice de la habilidad en la lista; solo lo usa la red).
    let hot_key = 0;
    // o->Light del personaje: no lo tenemos, blanco.
    let light = Vec3::ONE;

    match c.skill {
        AT_SKILL_RAGEFUL_BLOW => {
            // o->Weapon/WeaponLevel = arma derecha: el efecto lo lee de `Actor::weapon` del dueño.
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
            // o->Weapon/WeaponLevel = arma derecha: el efecto lo lee de `Actor::weapon` del dueño.
            ctx.create_effect(MODEL_SKILL_WHEEL1, o.position, o.angle, light, 0, owner, pk_key, 0, hot_key, 0.0);
            ctx.play_sound("SOUND_SKILL_SWORD4");
            // TODO(original): c->PostMoveProcess_Active(g_iLimitAttackTime).
        }
        _ => {}
    }

    // TODO(original): con TargetCharacter == -1 solo flechas (arcos) y PlayUntargetedAttackHitSound.
    // Con objetivo: sangre BITMAP_BLOOD+1 si Hit, CreateSpark para PLAYER_ATTACK_SKILL_SWORD1..5,
    // BITMAP_JOINT_ENERGY si ATTACK_DIE y el sonido del golpe (no hay monstruos ni daño).
}
