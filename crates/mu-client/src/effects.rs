//! Map emitters: the object points that release fire and smoke (`CreateFire` and the fountain
//! splashes), and flames requested by other systems (the dragons' heads).
//!
//! This only decides when and where each particle is born, like `CreateFire` in the original;
//! creation, movement and drawing are done by the effects engine (`fx/`) with the translated code of
//! `BITMAP_FIRE` and `BITMAP_SMOKE`.

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
            // Before the engine tick, so the particles are born in this same tick.
            .add_systems(FixedUpdate, emit.before(crate::fx::sim::tick));
    }
}

/// Distance (MU) up to which fires light the ground.
const LIGHT_DISTANCE: f32 = 3000.0;

/// An emitter placed on the map (registered by `objects.rs` when loading objects).
pub struct PlacedEmitter {
    pub emitter: Emitter,
    /// Object position, rotation and scale (in MU).
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// Object angles in degrees (`o->Angle`, the particle is born with them).
    pub angle: Vec3,
    /// For emitters attached to a bone: the model and its animation speed.
    pub model: Option<(Arc<Bmd>, f32)>,
}

#[derive(Resource, Default)]
pub struct Emitters(pub Vec<PlacedEmitter>);

/// On map change: nothing from the previous one.
pub fn reset(mut emitters: ResMut<Emitters>, mut fire: ResMut<FireRequests>) {
    emitters.0.clear();
    fire.0.clear();
}

/// Loose flames requested by other systems for this tick (e.g. the dragons' heads):
/// (position in MU, light).
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

/// One emitter tick (`CreateFire` and the fountain splashes).
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

    // Flames requested by others: `CreateParticle(BITMAP_FIRE, pos, angle, light, 0)`.
    for (pos, light) in requests.0.drain(..) {
        push(Particle::new(BITMAP_FIRE, pos, Vec3::ZERO, light, 0, 1.0, None));
    }

    let hero = players.single().map(|p| p.pos * MU_SCALE).ok();
    for e in &emitters.0 {
        let em = &e.emitter;
        let local = match (em.bone, &e.model) {
            // Fountain splashes: random point around a bone of the animated jet.
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
            // `CreateFire` moves each flame slightly at random.
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
                // Orange ground light of radius 4, every tick (only near the character).
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
