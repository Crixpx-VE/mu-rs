//! Emisores del mapa: los puntos de los objetos que sueltan fuego y humo (`CreateFire` y las
//! salpicaduras de la fuente), y las llamas que piden otros sistemas (la cabeza de los dragones).
//!
//! Aquí solo se decide cuándo y dónde nace cada partícula, como `CreateFire` en el original; su
//! creación, movimiento y dibujo los hace el motor de efectos (`fx/`) con el código traducido de
//! `BITMAP_FIRE` y `BITMAP_SMOKE`.

use std::sync::Arc;

use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::objects::{EmitKind, Emitter};

use crate::fx::core::{BITMAP_FIRE, BITMAP_SMOKE, Particle, Rng, Spawn};
use crate::fx::FxWorld;
use crate::terrain::{MU_SCALE, TerrainLights};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Emitters>()
            .init_resource::<FireRequests>()
            .init_resource::<EmitRng>()
            // Antes del tick del motor, para que las partículas nazcan en este mismo tick.
            .add_systems(FixedUpdate, emit.before(crate::fx::sim::tick));
    }
}

/// Distancia (MU) hasta la que los fuegos iluminan el suelo.
const LIGHT_DISTANCE: f32 = 3000.0;

/// Un emisor colocado en el mapa (lo registra `objects.rs` al cargar los objetos).
pub struct PlacedEmitter {
    pub emitter: Emitter,
    /// Posición, giro y escala del objeto (en MU).
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// Ángulos del objeto en grados (`o->Angle`, con los que nace la partícula).
    pub angle: Vec3,
    /// Para los emisores pegados a un hueso: el modelo y su velocidad de animación.
    pub model: Option<(Arc<Bmd>, f32)>,
}

#[derive(Resource, Default)]
pub struct Emitters(pub Vec<PlacedEmitter>);

/// Al cambiar de mapa: nada del anterior.
pub fn reset(mut emitters: ResMut<Emitters>, mut fire: ResMut<FireRequests>) {
    emitters.0.clear();
    fire.0.clear();
}

/// Llamas sueltas que piden otros sistemas para este tick (p. ej. la cabeza de los dragones):
/// (posición en MU, luz).
#[derive(Resource, Default)]
pub struct FireRequests(pub Vec<(Vec3, Vec3)>);

#[derive(Resource, Default)]
struct EmitRng(Rng);

impl EmitRng {
    fn rand(&mut self) -> i32 {
        self.0.rand()
    }
    /// `Random::RangeFloat(a, b)`.
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * (self.rand() as f32 / 32767.0)
    }
}

/// Un tick de los emisores (`CreateFire` y las salpicaduras de la fuente).
fn emit(
    emitters: Res<Emitters>,
    mut requests: ResMut<FireRequests>,
    mut rng: ResMut<EmitRng>,
    mut world: ResMut<FxWorld>,
    mut lights: ResMut<TerrainLights>,
    players: Query<&crate::character::Player>,
    time: Res<Time<Fixed>>,
) {
    let mut push = |p: Particle| world.pending.push(Spawn::Particle(p));

    // Llamas pedidas por otros: `CreateParticle(BITMAP_FIRE, pos, angle, luz, 0)`.
    for (pos, light) in requests.0.drain(..) {
        push(Particle::new(BITMAP_FIRE, pos, Vec3::ZERO, light, 0, 1.0, None));
    }

    let hero = players.single().map(|p| p.pos * MU_SCALE).ok();
    for e in &emitters.0 {
        let em = &e.emitter;
        let local = match (em.bone, &e.model) {
            // Salpicaduras de la fuente: punto al azar alrededor de un hueso del chorro animado.
            (Some(bone), Some((model, fps))) => {
                let s = em.spread;
                let off = Vec3::new(em.offset[0] + rng.range(-s, s - 1.0), em.offset[1], em.offset[2] + rng.range(-s, s - 1.0));
                let bones = model.bone_matrices_at(0, time.elapsed_secs() * fps);
                let Some(m) = bones.get(bone) else { continue };
                Vec3::from_array(bmd::transform_point(m, off.to_array())) * e.scale
            }
            _ => Vec3::from_array(em.offset),
        };
        let mut pos = e.position + e.rotation * local;
        if em.bone.is_none() {
            // `CreateFire` mueve cada llama un poco al azar.
            pos += Vec3::new(rng.range(-8.0, 7.0), rng.range(-8.0, 7.0), rng.range(-8.0, 7.0));
        }
        match em.kind {
            EmitKind::Fire => {
                let l = rng.range(6.0, 11.0) * 0.1;
                let light = Vec3::new(l, 0.6 * l, 0.4 * l);
                if rng.rand() % 2 == 0 {
                    let sub = rng.rand() % 4;
                    push(Particle::new(BITMAP_FIRE, pos, e.angle, light, sub, 1.0, None));
                }
                // Luz naranja de radio 4 en el suelo, cada tick (solo cerca del personaje).
                if hero.is_some_and(|h| (pos.truncate() - h).length() < LIGHT_DISTANCE) {
                    lights.add(pos.truncate() / MU_SCALE, light.to_array(), 4.0);
                }
            }
            EmitKind::Smoke => {
                if rng.rand() % 2 == 0 {
                    push(Particle::new(BITMAP_SMOKE, pos, e.angle, Vec3::ONE, 0, 1.0, None));
                }
            }
            EmitKind::DarkSmoke => {
                if rng.rand() % 2 == 0 {
                    push(Particle::new(BITMAP_SMOKE, pos, e.angle, Vec3::ONE, 2, 1.0, None));
                }
            }
        }
    }
}
