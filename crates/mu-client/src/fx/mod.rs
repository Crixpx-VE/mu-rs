//! Effects engine (particles, sprites, trails, model effects) translated from MuMain.
//!
//! - `core.rs`: the original's objects and the context (`Ctx`) with `CreateParticle`, `rand()`...
//! - `sim.rs`: the 25 Hz tick (`MoveEffects`, `MoveJoints`, `MoveParticles`).
//! - `draw.rs` / `models.rs`: drawing, interpolated between ticks.
//! - `kinds/`: the code for each kind (`BITMAP_FIRE`, `MODEL_SKILL_WHEEL1`...), one family per
//!   file, which registers its functions in the [`core::Registry`].
//! - `tables/`: tables generated from MuMain with `tools/gen_fx_tables.py` (constants, textures, models).

// Much of the original is only used by some effect kinds.
#![allow(dead_code, unused_imports, unused_variables, unused_mut, clippy::too_many_arguments)]

pub mod core;
pub mod draw;
pub mod kinds;
pub mod material;
pub mod models;
pub mod sim;
pub mod textures;
#[cfg(test)]
mod smoke_tests;

pub mod tables {
    pub mod bitmaps;
    pub mod ids;
    pub mod models;
}

use bevy::prelude::*;

use crate::Args;

pub use self::core::{Actor, Target};
pub use self::sim::{ActorState, FxWorld};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        let mut registry = core::Registry::default();
        kinds::register_all(&mut registry);
        app.add_plugins(material::FxMaterialPlugin)
            // The original logic runs at 25 ticks per second (`REFERENCE_FPS`): everything using
            // `FixedUpdate` (effects, skills, birds...) depends on this. Bevy defaults to 64.
            .insert_resource(Time::<Fixed>::from_hz(sim::TICKS_PER_SECOND as f64))
            .insert_resource(registry)
            .init_resource::<sim::FxWorld>()
            .init_resource::<sim::Actors>()
            .init_resource::<draw::FrameDraw>()
            .init_resource::<draw::Batches>()
            .add_systems(Startup, |mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>| {
                commands.insert_resource(textures::FxTextures::new(args.data.clone()));
                commands.insert_resource(models::FxModels::new(args.data.clone()));
                let _ = images.add(textures::white_image());
            })
            .add_systems(FixedUpdate, (sim::gather_actors, sim::tick).chain())
            .add_systems(
                PostUpdate,
                (draw::collect, draw::render_quads, models::render_models)
                    .chain()
                    .after(crate::camera::CameraMoved)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// On map change: no effects, map objects or characters from the previous map.
pub fn reset(mut world: ResMut<FxWorld>, mut actors: ResMut<sim::Actors>) {
    let next_effect_id = world.next_effect_id;
    *world = FxWorld { next_effect_id, ..default() };
    *actors = sim::Actors::default();
}

/// Request an effect from outside the engine (characters, map objects): created on the next tick.
pub fn request(world: &mut FxWorld, spawn: core::Spawn) {
    world.pending.push(spawn);
}
