//! Simulación del motor de efectos: un tick a 25 Hz que hace lo mismo que el original en
//! `MoveEffects`, `MoveJoints` y `MoveParticles` (en ese orden, como `MainScene.cpp`), y luego
//! crea lo que se pidió durante el tick con la función de creación de cada tipo.

use std::collections::HashMap;

use bevy::prelude::*;

use super::core::*;
use super::core::Spawn;
use crate::terrain::{LoadedTerrain, TerrainLights};

/// Límites del original (`_define.h`).
pub const MAX_PARTICLES: usize = 3000;
pub const MAX_JOINTS: usize = 500;
pub const MAX_EFFECTS: usize = 300;
/// Ticks de la lógica del original por segundo (`REFERENCE_FPS`).
pub const TICKS_PER_SECOND: f32 = 25.0;

/// Todo lo vivo del motor.
#[derive(Resource, Default)]
pub struct FxWorld {
    pub particles: Vec<Particle>,
    pub joints: Vec<Joint>,
    pub effects: Vec<Effect>,
    /// Sprites de este tick (`CreateSprite`); se vacían al empezar el siguiente.
    pub sprites: Vec<SpriteFx>,
    pub rng: Rng,
    pub next_effect_id: u32,
    pub earthquake: f32,
    pub ticks: u64,
    /// Lo pedido desde fuera del motor (personajes, objetos del mapa) para el próximo tick.
    pub pending: Vec<Spawn>,
    /// Habilidades en curso (`AttackTime` de cada personaje).
    pub casts: Vec<Cast>,
    /// Estelas de las armas.
    pub blurs: Vec<Blur>,
}

impl FxWorld {
    /// `CreateBlur`: añade un punto a la estela del arma de `owner` (o la crea). `frame` es el
    /// número de fotograma de dibujo (los puntos de un fotograma sustituyen a los del anterior).
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

/// Personajes que pueden seguir los efectos, actualizados por quien los mueve.
#[derive(Resource, Default)]
pub struct Actors {
    pub map: HashMap<Entity, Actor>,
    pub hero: Option<Entity>,
}

/// Componente con la foto de un personaje para el motor (lo actualiza `character.rs`).
#[derive(Component, Clone)]
pub struct ActorState(pub Actor);

pub fn gather_actors(mut actors: ResMut<Actors>, world: Res<FxWorld>, q: Query<(Entity, &ActorState, Has<crate::character::Player>)>) {
    actors.map.clear();
    for (e, s, is_hero) in &q {
        let mut a = s.0.clone();
        // `m_vPosSword`: lo calcula la habilidad en curso del personaje.
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
    let world = &mut *world;
    world.ticks += 1;
    world.sprites.clear();
    world.earthquake = 0.0;
    let effect_positions: HashMap<u32, Vec3> = world.effects.iter().map(|e| (e.id, e.position)).collect();
    let mut spawns: Vec<Spawn> = std::mem::take(&mut world.pending);

    let FxWorld { particles, joints, effects, sprites, rng, next_effect_id, earthquake, casts, blurs: world_blurs, .. } = world;
    let mut ctx = Ctx {
        rng,
        world_time: time.elapsed_secs() * 1000.0,
        terrain: terrain.as_deref().map(|t| &t.0),
        hero: actors.hero,
        actors: &actors.map,
        effect_positions: &effect_positions,
        spawns: &mut spawns,
        sprites,
        lights: &mut lights,
        earthquake,
        next_effect_id,
        world_active: args.world as i32 - 1,
    };

    // Habilidades en curso (`MoveCharactersClient` va antes que `MoveEffects`): `AttackStage` cada
    // tick y el efecto final al llegar a `g_iLimitAttackTime`.
    for c in casts.iter_mut() {
        let kind = registry.skills.get(&c.skill).copied().unwrap_or_default();
        if let Some(stage) = kind.stage {
            stage(c, &mut ctx);
        }
        // Como el original: AttackStage, AttackTime += 1 y entonces se mira el límite (mismo tick).
        c.attack_time += 1;
        if c.attack_time >= LIMIT_ATTACK_TIME {
            if let Some(finish) = kind.finish {
                finish(c, &mut ctx);
            }
            c.live = false;
        }
    }
    casts.retain(|c| c.live);

    // Estelas de las armas (`MoveBlurs`).
    for b in world_blurs.iter_mut() {
        b.age();
    }
    world_blurs.retain(|b| b.live);

    // Efectos (`MoveEffect`): lo propio del tipo y luego `LifeTime -= 1`.
    for e in effects.iter_mut() {
        e.prev_position = e.position;
        e.prev_angle = e.angle;
        match registry.effects.get(&e.ty).and_then(|k| k.step) {
            Some(step) => step(e, &mut ctx),
            None => {
                // Por defecto: animar y avanzar en su dirección (`MoveParticle(o, true)`).
                e.animation_frame += e.velocity;
                e.position += rotate(e.angle, e.direction);
            }
        }
        e.life_time -= 1.0;
        if e.life_time <= 0.0 {
            e.live = false;
        }
    }

    // Estelas (`MoveJoint`): avanzar por su −Y local, lo propio del tipo, dejar cola, vida.
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

    // Partículas (`MoveParticles`): vida, avance con su velocidad girada y lo propio del tipo.
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

    // Crear lo pedido (y lo que pidan a su vez las funciones de creación).
    let mut rounds = 0;
    while !ctx.spawns.is_empty() && rounds < 8 {
        rounds += 1;
        let batch = std::mem::take(ctx.spawns);
        for s in batch {
            match s {
                Spawn::Particle(mut p) => {
                    if let Some(create) = registry.particles.get(&p.ty).and_then(|k| k.create) {
                        create(&mut p, &mut ctx);
                    }
                    p.prev_position = p.position;
                    p.prev_scale = p.scale;
                    p.prev_light = p.light;
                    if p.live && particles.len() < MAX_PARTICLES {
                        particles.push(p);
                    }
                }
                Spawn::Joint(mut j) => {
                    if let Some(create) = registry.joints.get(&j.ty).and_then(|k| k.create) {
                        create(&mut j, &mut ctx);
                    }
                    if j.create_tails && j.tails.is_empty() {
                        j.tails.push(j.make_tail());
                    }
                    if j.live && joints.len() < MAX_JOINTS {
                        joints.push(j);
                    }
                }
                Spawn::Effect(mut e) => {
                    if let Some(create) = registry.effects.get(&e.ty).and_then(|k| k.create) {
                        create(&mut e, &mut ctx);
                    }
                    e.prev_position = e.position;
                    e.prev_angle = e.angle;
                    if e.live && effects.len() < MAX_EFFECTS {
                        effects.push(e);
                    }
                }
            }
        }
    }

    particles.retain(|p| p.live);
    joints.retain(|j| j.live);
    effects.retain(|e| e.live);
}
