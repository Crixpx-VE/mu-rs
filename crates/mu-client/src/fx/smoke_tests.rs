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
    let draw_ctx = DrawCtx { world_time: 0.0, tex_info: &tex_info, actors: &actors.map };
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
