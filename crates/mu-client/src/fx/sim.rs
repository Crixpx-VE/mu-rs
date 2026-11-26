//! Effect engine simulation: a 25 Hz tick that does what the original does in
//! `MoveEffects`, `MoveJoints` and `MoveParticles` (in that order, as in `MainScene.cpp`), and then
//! creates whatever was requested during the tick with each kind's create function.

use std::collections::HashMap;

use bevy::prelude::*;

use super::core::*;
use super::core::Spawn;
use crate::terrain::{LoadedTerrain, TerrainLights};

/// Limits of the original (`_define.h`).
pub const MAX_PARTICLES: usize = 3000;
pub const MAX_JOINTS: usize = 500;
pub const MAX_EFFECTS: usize = 300;
/// Original logic ticks per second (`REFERENCE_FPS`).
pub const TICKS_PER_SECOND: f32 = 25.0;
/// Distance (MU) within which map objects emit their effects.
pub const OBJECT_FX_DISTANCE: f32 = 3000.0;

/// Everything alive in the engine.
#[derive(Resource, Default)]
pub struct FxWorld {
    pub particles: Vec<Particle>,
    pub joints: Vec<Joint>,
    pub effects: Vec<Effect>,
    /// Sprites of this tick (`CreateSprite`); cleared when the next one starts.
    pub sprites: Vec<SpriteFx>,
    pub rng: Rng,
    pub next_effect_id: u32,
    pub earthquake: f32,
    pub ticks: u64,
    /// Requests from outside the engine (characters, map objects) for the next tick.
    pub pending: Vec<Spawn>,
    /// Skills in progress (each character's `AttackTime`).
    pub casts: Vec<Cast>,
    /// Weapon trails.
    pub blurs: Vec<Blur>,
    /// Map objects with effect code (registered by `objects.rs`).
    pub map_objects: Vec<MapObject>,
    /// Sounds requested during the ticks (`PlayBuffer`); played by `sound.rs`.
    pub sounds: Vec<FxSound>,
    /// Effect model files for the kinds (`Models[]`).
    pub models: super::model_data::ModelData,
    /// `CharacterAttribute->Skill[]`: the hero's skills by slot, once the client knows them.
    pub hero_skills: Vec<i32>,
    /// `CharacterMachine->PacketSerial` (see [`Ctx::packet_serial`]).
    pub packet_serial: u8,
    /// `Hero->CurrentSkill` (the selected slot), kept up to date by skills.rs.
    pub hero_current_skill: i32,
    /// Changes to characters requested by the effects; applied by `fx::apply_actor_requests`.
    pub actor_requests: Vec<ActorRequest>,
    /// `o->m_sTargetIndex` of each character that has one (set by combat.rs), for `Actor::target`.
    pub actor_targets: HashMap<Entity, Entity>,
}

impl FxWorld {
    /// `CreateBlur`: adds a point to `owner`'s weapon trail (or creates it). `frame` is the
    /// draw frame number (the points of one frame replace those of the previous one).
    pub fn create_blur(&mut self, owner: Entity, p1: Vec3, p2: Vec3, light: Vec3, mapping: i32, short: bool, frame: u64) {
        if let Some(b) = self.blurs.iter_mut().find(|b| b.live && b.owner == owner) {
            b.light = light;
            b.mapping = mapping;
            b.add_sample(p1, p2, frame);
            return;
        }
        let mut b = Blur {
            owner,
            p1: vec![],
            p2: vec![],
            light,
            mapping,
            life: if short { 15 } else { MAX_BLUR_LIFETIME },
            frame_samples: 0,
            frame,
            live: true,
        };
        b.add_sample(p1, p2, frame);
        self.blurs.push(b);
    }
}

/// Characters that effects can follow, updated by whoever moves them.
#[derive(Resource, Default)]
pub struct Actors {
    pub map: HashMap<Entity, Actor>,
    pub hero: Option<Entity>,
}

/// Component with a snapshot of a character for the engine (updated by `character.rs`).
#[derive(Component, Clone)]
pub struct ActorState(pub Actor);

pub fn gather_actors(
    mut actors: ResMut<Actors>,
    world: Res<FxWorld>,
    buffs: Option<Res<crate::buffs::Buffs>>,
    hero_key: Option<Res<crate::remote::HeroKey>>,
    q: Query<(Entity, &ActorState, Has<crate::character::Player>)>,
) {
    actors.map.clear();
    for (e, s, is_hero) in &q {
        let mut a = s.0.clone();
        if let Some(t) = world.actor_targets.get(&e) {
            a.target = Some(*t);
        }
        // `g_isCharacterBuff`: what the server said of this object (buffs.rs).
        let key = if is_hero { hero_key.as_ref().map(|k| k.0 as i32) } else { (a.key >= 0).then_some(a.key) };
        if let (Some(b), Some(key)) = (buffs.as_ref(), key) {
            let list = b.of(key as u16);
            if !list.is_empty() {
                a.buffs = std::sync::Arc::new(list.to_vec());
            }
        }
        // `m_vPosSword`: computed by the character's skill in progress.
        if let Some(c) = world.casts.iter().find(|c| c.owner == e) {
            a.pos_sword = c.pos_sword;
        }
        actors.map.insert(e, a);
        if is_hero {
            actors.hero = Some(e);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut world: ResMut<FxWorld>,
    registry: Res<Registry>,
    actors: Res<Actors>,
    terrain: Option<Res<LoadedTerrain>>,
    mut lights: ResMut<TerrainLights>,
    time: Res<Time<Fixed>>,
    args: Res<crate::Args>,
) {
    step(
        &mut world,
        &registry,
        &actors,
        terrain.as_deref().map(|t| &t.0),
        &mut lights,
        time.elapsed_secs() * 1000.0,
        args.world as i32 - 1,
    );
}

/// One 25 Hz tick of the whole engine (what [`tick`] runs), without Bevy resources so tests can
/// drive it. `world_time` is `WorldTime` in ms; `world_active` the original's map number.
pub fn step(
    world: &mut FxWorld,
    registry: &Registry,
    actors: &Actors,
    terrain: Option<&mu_data::terrain::Terrain>,
    lights: &mut TerrainLights,
    world_time: f32,
    world_active: i32,
) {
    world.ticks += 1;
    world.sprites.clear();
    world.earthquake = 0.0;
    let effect_positions: HashMap<u32, Vec3> = world.effects.iter().map(|e| (e.id, e.position)).collect();
    let effect_infos: Vec<EffectInfo> = world
        .effects
        .iter()
        .map(EffectInfo::of)
        .collect();
    let mut deletes: Vec<Delete> = Vec::new();
    let pending: Vec<Spawn> = std::mem::take(&mut world.pending);
    let mut spawns: Vec<Spawn> = Vec::new();
    world.models.set_world(world_active);
    let free = [
        MAX_PARTICLES.saturating_sub(world.particles.len()),
        MAX_JOINTS.saturating_sub(world.joints.len()),
        MAX_EFFECTS.saturating_sub(world.effects.len()),
    ];

    let FxWorld {
        particles,
        joints,
        effects,
        sprites,
        rng,
        next_effect_id,
        earthquake,
        casts,
        blurs: world_blurs,
        map_objects,
        ticks,
        sounds,
        models,
        hero_skills,
        packet_serial,
        hero_current_skill,
        actor_requests,
        ..
    } = world;
    let mut ctx = Ctx {
        rng,
        world_time,
        terrain,
        hero: actors.hero,
        actors: &actors.map,
        effect_positions: &effect_positions,
        spawns: &mut spawns,
        sprites,
        lights,
        earthquake,
        next_effect_id,
        world_active,
        sounds,
        effects: &effect_infos,
        deletes: &mut deletes,
        registry,
        models,
        hero_skills,
        packet_serial,
        hero_current_skill: *hero_current_skill,
        actor_requests,
        free,
        depth: 0,
    };

    // Map objects (`MoveObjects` / `UpdateObjectVisual`): only nearby ones, like the original,
    // which only moves the visible blocks.
    let hero_pos = ctx.hero_actor().map(|a| a.position);
    for o in map_objects.iter_mut() {
        if hero_pos.is_some_and(|h| (o.position.truncate() - h.truncate()).length() > OBJECT_FX_DISTANCE) {
            continue;
        }
        let Some(step) = registry.objects.get(&(ctx.world_active, o.ty)).copied() else { continue };
        o.animation_frame = o.velocity * *ticks as f32;
        o.bones = o.model.as_ref().filter(|m| m.actions.first().is_some_and(|a| a.frames > 1)).map(|m| {
            std::sync::Arc::new(m.bone_matrices_at(0, o.animation_frame))
        }).or_else(|| o.model.as_ref().map(|m| std::sync::Arc::new(m.bone_matrices(0, 0))));
        step(o, &mut ctx);
        o.created = true;
    }

    // Skills in progress (`MoveCharactersClient` runs before `MoveEffects`): `AttackStage` every
    // tick and the final effect on reaching `g_iLimitAttackTime`.
    for c in casts.iter_mut() {
        let kind = registry.skills.get(&c.skill).copied().unwrap_or_default();
        if let Some(stage) = kind.stage {
            stage(c, &mut ctx);
        }
        // As in the original: AttackStage, AttackTime += 1, then the limit is checked (same tick).
        c.attack_time += 1;
        if c.attack_time >= LIMIT_ATTACK_TIME {
            if let Some(finish) = kind.finish {
                finish(c, &mut ctx);
            }
            c.live = false;
        }
    }
    casts.retain(|c| c.live);

    // Weapon trails (`MoveBlurs`).
    for b in world_blurs.iter_mut() {
        b.age();
    }
    world_blurs.retain(|b| b.live);

    // Effects (`MoveEffect`): the kind's own logic, then `LifeTime -= 1`.
    for e in effects.iter_mut() {
        e.prev_position = e.position;
        e.prev_angle = e.angle;
        match registry.effects.get(&e.ty).and_then(|k| k.step) {
            Some(step) => step(e, &mut ctx),
            None => {
                // Default: animate and move along its direction (`MoveParticle(o, true)`).
                e.animation_frame += e.velocity;
                e.position += rotate(e.angle, e.direction);
            }
        }
        e.life_time -= 1.0;
        if e.life_time <= 0.0 {
            e.live = false;
        }
    }

    // Trails (`MoveJoint`): move along local −Y, the kind's own logic, leave a tail, lifetime.
    for j in joints.iter_mut() {
        j.position += rotate(j.angle, Vec3::new(0.0, -j.velocity, 0.0));
        if let Some(step) = registry.joints.get(&j.ty).and_then(|k| k.step) {
            step(j, &mut ctx);
        }
        if j.create_tails && j.live {
            j.follow_tail();
        }
        j.life_time -= 1.0;
        if j.life_time < 0.0 {
            j.live = false;
        }
    }

    // Particles (`MoveParticles`): lifetime, move by its rotated velocity and the kind's own logic.
    for p in particles.iter_mut() {
        p.prev_position = p.position;
        p.prev_scale = p.scale;
        p.prev_light = p.light;
        p.life_time -= 1.0;
        if p.life_time <= 0.0 {
            p.live = false;
        }
        if p.enable_move {
            p.position += rotate(p.angle, p.velocity);
        }
        if let Some(step) = registry.particles.get(&p.ty).and_then(|k| k.step) {
            step(p, &mut ctx);
        }
    }

    // Requests from outside the engine (characters, map objects): their creation code runs now.
    // What the kinds created during the tick already ran it, in the middle of their code.
    for sp in pending {
        ctx.spawn(sp);
    }
    for sp in std::mem::take(ctx.spawns) {
        match sp {
            Spawn::Particle(p) if particles.len() < MAX_PARTICLES => particles.push(p),
            Spawn::Joint(j) if joints.len() < MAX_JOINTS => joints.push(j),
            Spawn::Effect(e) if effects.len() < MAX_EFFECTS => effects.push(e),
            _ => {}
        }
    }
    // Nobody applies them without the game (tests, other scenes): keep a bound.
    let overflow = ctx.actor_requests.len().saturating_sub(256);
    ctx.actor_requests.drain(..overflow);

    // Deletions requested during the tick (`DeleteEffect`, `DeleteJoint`, `DeleteParticle`).
    for d in deletes {
        match d {
            Delete::Effect { ty, owner, sub_type } => {
                for e in effects.iter_mut().filter(|e| e.ty == ty && e.owner == owner && (sub_type == -1 || e.sub_type == sub_type)) {
                    e.live = false;
                }
            }
            Delete::EffectId(id) => {
                for e in effects.iter_mut().filter(|e| e.id == id) {
                    e.live = false;
                }
            }
            // ZzzEffectJoint.cpp:2709: a NULL target matches any target.
            Delete::Joint { ty, target, sub_type } => {
                let hit = |j: &Joint| j.ty == ty && (target.is_none() || j.target == target) && (sub_type == -1 || j.sub_type == sub_type);
                for j in joints.iter_mut().filter(|j| hit(j)) {
                    j.live = false;
                }
            }
            Delete::Particle(ty) => {
                if let Some(p) = particles.iter_mut().find(|p| p.live && p.ty == ty) {
                    p.live = false;
                }
            }
        }
    }

    particles.retain(|p| p.live);
    joints.retain(|j| j.live);
    effects.retain(|e| e.live);
}
