//! Smoke tests for every registered kind: each one is created through the engine (with and
//! without an owner character, for several subtypes), ticked like the game does and drawn, and
//! nothing may panic or produce non-finite positions.

use std::sync::Arc;

use bevy::math::Vec3;
use bevy::prelude::Entity;

use super::core::*;
use super::sim::{self, Actors, FxWorld};
use crate::terrain::TerrainLights;

fn registry() -> Registry {
    let mut r = Registry::default();
    super::kinds::register_all(&mut r);
    r
}

/// The hero, standing at (12800, 12800) with 80 identity bones (enough for any bone index the
/// kinds use) and a sword.
fn actors() -> (Actors, Entity) {
    let e = Entity::PLACEHOLDER;
    let identity: mu_data::bmd::Mat34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
    let actor = Actor {
        ty: MODEL_PLAYER,
        position: Vec3::new(12800.0, 12800.0, 0.0),
        angle: Vec3::new(0.0, 0.0, 45.0),
        scale: 1.0,
        bones: Arc::new(vec![identity; 80]),
        action: 0,
        frame: 0.0,
        weapon: Some((MODEL_SWORD, 0)),
        weapon_bone: 33,
        live: true,
        pos_sword: Vec3::new(12850.0, 12850.0, 100.0),
        ..Default::default()
    };
    let mut a = Actors::default();
    a.map.insert(e, actor);
    a.hero = Some(e);
    (a, e)
}

const SUBTYPES: [i32; 8] = [0, 1, 2, 3, 4, 5, 10, 20];
const TICKS: usize = 120;
/// `WorldActive` values the kinds branch on: Lorencia (0), Kalima 1 (24, `InHellas`) and
/// Kanturu 3 (39).
const WORLDS: [i32; 3] = [0, 24, 39];

fn tex_info(_id: i32) -> Option<TexInfo> {
    Some(TexInfo { width: 64.0, height: 64.0, components: 4 })
}

/// Ticks `world` like the game and draws every frame; checks positions stay finite.
fn run(world: &mut FxWorld, registry: &Registry, actors: &Actors, world_active: i32, what: &str) {
    let mut lights = TerrainLights::default();
    let models = super::model_data::ModelData::default();
    let draw_ctx = DrawCtx { world_time: 0.0, tex_info: &tex_info, actors: &actors.map, world_active, models: &models };
    for t in 0..TICKS {
        sim::step(world, registry, actors, None, &mut lights, t as f32 * 40.0, world_active);
        let mut out = DrawList::default();
        for e in &world.effects {
            assert!(e.position.is_finite(), "{what}: effect {} position {:?}", e.ty, e.position);
            match registry.effects.get(&e.ty).and_then(|k| k.draw) {
                Some(draw) => draw(e, &draw_ctx, &mut out),
                None => out.models.push(ModelDraw::of_effect(e)),
            }
        }
        for p in &world.particles {
            assert!(p.position.is_finite(), "{what}: particle {} position {:?}", p.ty, p.position);
            match registry.particles.get(&p.ty).and_then(|k| k.draw) {
                Some(draw) => draw(p, &draw_ctx, &mut out),
                None => default_particle_draw(p, &draw_ctx, &mut out),
            }
        }
        for j in &world.joints {
            assert!(j.position.is_finite(), "{what}: joint {} position {:?}", j.ty, j.position);
        }
    }
}

#[test]
fn no_kind_registered_twice() {
    assert_eq!(registry().duplicates, vec![], "kinds registered by two files (the later one wins)");
}

#[test]
fn every_effect_kind_runs() {
    let registry = registry();
    let (actors, hero) = actors();
    let mut types: Vec<i32> = registry.effects.keys().copied().collect();
    types.sort();
    for &ty in &types {
        for &world_active in &WORLDS {
            for &sub in &SUBTYPES {
                for owner in [None, Some(Target::Actor(hero))] {
                    let mut world = FxWorld::default();
                    let pos = Vec3::new(12900.0, 12900.0, 100.0);
                    let id = 1;
                    world.next_effect_id = id;
                    let mut e = Effect::new(id, ty, pos, Vec3::new(0.0, 0.0, 90.0), Vec3::ONE, sub, owner, -1, 0, 0, 1.0);
                    e.target = owner;
                    world.pending.push(Spawn::Effect(e));
                    run(&mut world, &registry, &actors, world_active, &format!("effect {ty} sub {sub} world {world_active} owner {}", owner.is_some()));
                }
            }
        }
    }
}

#[test]
fn every_particle_kind_runs() {
    let registry = registry();
    let (actors, hero) = actors();
    let mut types: Vec<i32> = registry.particles.keys().copied().collect();
    types.sort();
    for &ty in &types {
        for &sub in &SUBTYPES {
            for owner in [None, Some(Target::Actor(hero))] {
                let mut world = FxWorld::default();
                let pos = Vec3::new(12900.0, 12900.0, 100.0);
                world.pending.push(Spawn::Particle(Particle::new(ty, pos, Vec3::ZERO, Vec3::ONE, sub, 1.0, owner)));
                run(&mut world, &registry, &actors, 0, &format!("particle {ty} sub {sub}"));
            }
        }
    }
}

#[test]
fn every_joint_kind_runs() {
    let registry = registry();
    let (actors, hero) = actors();
    let mut types: Vec<i32> = registry.joints.keys().copied().collect();
    types.sort();
    for &ty in &types {
        for &sub in &SUBTYPES {
            for target in [None, Some(Target::Actor(hero))] {
                let mut world = FxWorld::default();
                let pos = Vec3::new(12900.0, 12900.0, 100.0);
                let to = Vec3::new(13100.0, 13000.0, 120.0);
                world.pending.push(Spawn::Joint(Joint::new(ty, pos, to, Vec3::ZERO, sub, target, 10.0)));
                run(&mut world, &registry, &actors, 0, &format!("joint {ty} sub {sub}"));
            }
        }
    }
}

/// Runs `f` with a `Ctx` over `actors` (hero = `hero`).
fn with_ctx<R>(actors: &Actors, f: impl FnOnce(&mut Ctx) -> R) -> (R, Vec<AttackRequest>) {
    let registry = registry();
    let mut world = FxWorld::default();
    let mut lights = TerrainLights::default();
    let (mut spawns, mut deletes, mut requests) = (Vec::new(), Vec::new(), Vec::new());
    let positions = std::collections::HashMap::new();
    let FxWorld { rng, sprites, earthquake, next_effect_id, sounds, models, packet_serial, actor_requests, .. } = &mut world;
    let mut ctx = Ctx {
        rng,
        world_time: 0.0,
        terrain: None,
        hero: actors.hero,
        actors: &actors.map,
        effect_positions: &positions,
        spawns: &mut spawns,
        sprites,
        lights: &mut lights,
        earthquake,
        next_effect_id,
        world_active: 0,
        sounds,
        effects: &[],
        deletes: &mut deletes,
        registry: &registry,
        models,
        hero_skills: &[],
        attack_requests: &mut requests,
        packet_serial,
        hero_current_skill: 0,
        actor_requests,
        free: [10, 10, 10],
        depth: 0,
    };
    let r = f(&mut ctx);
    (r, requests)
}

fn character(kind: i32, key: i32, x: f32) -> Actor {
    Actor { kind, key, position: Vec3::new(x, 12800.0, 0.0), ..Default::default() }
}

/// `CheckCharacterRange` (ZzzEffect.cpp:43-65).
#[test]
fn check_character_range_like_the_original() {
    let (mut actors, hero) = actors();
    let monster = Entity::from_bits(1 << 32 | 5);
    actors.map.insert(monster, character(KIND_MONSTER, 7, 13000.0));
    let at = |x: f32| Vec3::new(x, 12800.0, 50.0);
    // A monster 50 away counts; 150 away does not.
    assert!(with_ctx(&actors, |c| c.check_character_range(at(13050.0), None, 100.0, 0)).0);
    assert!(!with_ctx(&actors, |c| c.check_character_range(at(13150.0), None, 100.0, 0)).0);
    // Not the owner.
    assert!(!with_ctx(&actors, |c| c.check_character_range(at(13050.0), Some(Target::Actor(monster)), 100.0, 0)).0);
    // `Kind` skips that kind.
    assert!(!with_ctx(&actors, |c| c.check_character_range(at(13050.0), None, 100.0, KIND_MONSTER)).0);
    // The hero (at 12800) only counts for `KIND_MONSTER` checks.
    assert!(!with_ctx(&actors, |c| c.check_character_range(at(12810.0), None, 50.0, 0)).0);
    assert!(with_ctx(&actors, |c| c.check_character_range(at(12810.0), None, 50.0, KIND_MONSTER)).0);
    // Dead characters do not count.
    actors.map.get_mut(&monster).unwrap().dead = true;
    assert!(!with_ctx(&actors, |c| c.check_character_range(at(13050.0), None, 100.0, 0)).0);
}

/// `SpinMonstersInTornado` (MuMain ZzzEffect.cpp:106-116): live, visible, not dying monsters in
/// reach; no players or NPCs.
#[test]
fn spin_monsters_in_tornado_picks_the_monsters_in_reach() {
    let (mut actors, _) = actors();
    let (near, far, npc, dead) = (Entity::from_bits(1 << 32 | 5), Entity::from_bits(1 << 32 | 6), Entity::from_bits(1 << 32 | 7), Entity::from_bits(1 << 32 | 8));
    actors.map.insert(near, character(KIND_MONSTER, 7, 13000.0));
    actors.map.insert(far, character(KIND_MONSTER, 8, 13300.0));
    actors.map.insert(npc, character(KIND_NPC, 9, 13010.0));
    actors.map.insert(dead, Actor { dead: true, ..character(KIND_MONSTER, 10, 13020.0) });
    let (spun, _) = with_ctx(&actors, |c| {
        c.spin_monsters_in_tornado(Vec3::new(13050.0, 12800.0, 0.0), 150.0);
        c.actor_requests.clone()
    });
    assert_eq!(spun, vec![ActorRequest { actor: near, change: ActorChange::Spin }]);
}

/// `AttackCharacterRange` (ZzzEffect.cpp:108-189): monsters, or the player with the PK key.
#[test]
fn attack_character_range_collects_the_keys() {
    let (mut actors, _) = actors();
    actors.map.insert(Entity::from_bits(1 << 32 | 5), character(KIND_MONSTER, 7, 13000.0));
    actors.map.insert(Entity::from_bits(1 << 32 | 6), character(KIND_PLAYER, 9, 13010.0));
    actors.map.insert(Entity::from_bits(1 << 32 | 8), character(KIND_NPC, 11, 13020.0));
    let at = Vec3::new(13000.0, 12800.0, 0.0);
    let (hit, req) = with_ctx(&actors, |c| c.attack_character_range(0, at, 100.0, 3, -1));
    assert!(hit);
    assert_eq!(req, vec![AttackRequest { skill: 0, x: 130, y: 128, serial: 3, keys: vec![7] }]);
    let (_, req) = with_ctx(&actors, |c| c.attack_character_range(0, at, 100.0, 3, 9));
    assert_eq!(req[0].keys, vec![7, 9]);
    let (hit, req) = with_ctx(&actors, |c| c.attack_character_range(0, at + Vec3::X * 500.0, 100.0, 3, -1));
    assert!(!hit && req.is_empty());
}
