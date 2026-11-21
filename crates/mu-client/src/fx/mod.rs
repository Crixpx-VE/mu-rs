//! Motor de efectos (partículas, sprites, estelas, efectos con modelo) traducido de MuMain.
//!
//! - `core.rs`: los objetos del original y el contexto (`Ctx`) con `CreateParticle`, `rand()`...
//! - `sim.rs`: el tick de 25 Hz (`MoveEffects`, `MoveJoints`, `MoveParticles`).
//! - `draw.rs` / `models.rs`: el dibujo, interpolado entre ticks.
//! - `kinds/`: el código de cada tipo (`BITMAP_FIRE`, `MODEL_SKILL_WHEEL1`...), una familia por
//!   archivo, que registra sus funciones en el [`core::Registry`].
//! - `tables/`: tablas generadas de MuMain con `tools/gen_fx_tables.py` (constantes, texturas, modelos).

// Mucho de lo del original solo lo usan algunos tipos de efecto.
#![allow(dead_code, unused_imports, unused_variables, unused_mut, clippy::too_many_arguments)]

pub mod core;
pub mod draw;
pub mod kinds;
pub mod material;
pub mod models;
pub mod sim;
pub mod textures;

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

/// Pedir un efecto desde fuera del motor (personajes, objetos del mapa): se crea en el próximo tick.
pub fn request(world: &mut FxWorld, spawn: core::Spawn) {
    world.pending.push(spawn);
}
