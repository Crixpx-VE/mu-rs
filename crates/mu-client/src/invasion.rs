//! Ambiente de invasión de dragones (`EnableEvent` del original; el servidor lo activa con el
//! paquete C1 0B <encendido> <evento>: evento 1 = dragones rojos, 3 = dorados).
//!
//! Lo que cambia mientras está activo (GOBoid.cpp / ZzzLodTerrain.cpp):
//! - los pájaros dejan paso a dragones que cruzan el cielo (ver `boids.rs`);
//! - 1 de cada 40 ticks sale un meteorito (`MODEL_FIRE` subtipo 3, bola de fuego) cerca del
//!   personaje (ver `fx/kinds/fire.rs`);
//! - una zona más oscura de radio 16 casillas sigue al personaje (luz -0.3, -0.3, -0.2);
//! - la hierba se mueve 5 veces más rápido (ver `grass.rs`).
//!
//! Hasta que lo mande el servidor, solo con `--invasion 1|3`.

use bevy::prelude::*;

use crate::Args;
use crate::character::Player;
use crate::terrain::{LoadedTerrain, MU_SCALE, TerrainLights};

pub struct InvasionPlugin;

impl Plugin for InvasionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Invasion>()
            .init_resource::<MeteorRng>()
            .add_systems(Startup, (spawn_label, |args: Res<Args>, mut inv: ResMut<Invasion>| {
                inv.0 = args.invasion;
            }))
            .add_systems(Update, update_label)
            .add_systems(FixedUpdate, (darken_around_hero, spawn_meteors.before(crate::fx::sim::tick)));
    }
}

/// Evento activo: 0 = ninguno, 1 = dragones rojos, 3 = dragones dorados.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct Invasion(pub u8);

impl Invasion {
    pub fn active(self) -> bool {
        self.0 != 0
    }
}

/// La zona oscura que sigue al personaje (`AddTerrainLight(héroe, (-0.3,-0.3,-0.2), 16)`).
fn darken_around_hero(inv: Res<Invasion>, players: Query<&Player>, mut lights: ResMut<TerrainLights>) {
    if !inv.active() {
        return;
    }
    if let Ok(p) = players.single() {
        lights.add(p.pos, [-0.3, -0.3, -0.2], 16.0);
    }
}

#[derive(Component)]
struct InvasionLabel;

fn spawn_label(mut commands: Commands) {
    commands.spawn((
        InvasionLabel,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(8.0), ..default() },
        TextColor(Color::srgb(1.0, 0.45, 0.3)),
        TextShadow { offset: Vec2::splat(0.6), color: Color::BLACK },
        Node { position_type: PositionType::Absolute, top: Val::Px(14.0), left: Val::Px(4.0), ..default() },
    ));
}

fn update_label(inv: Res<Invasion>, mut q: Query<&mut Text, With<InvasionLabel>>) {
    if !inv.is_changed() {
        return;
    }
    for mut t in &mut q {
        t.0 = match inv.0 {
            1 => "Invasión de dragones rojos".into(),
            3 => "Invasión de dragones dorados".into(),
            _ => String::new(),
        };
    }
}


// --- Meteoritos -------------------------------------------------------------------------

/// Azar propio para la posición de los meteoritos (el original usa su `rand()` global).
#[derive(Resource, Default)]
struct MeteorRng(u32);

impl MeteorRng {
    fn rand(&mut self) -> i32 {
        let s = &mut self.0;
        if *s == 0 {
            *s = 0x6C07_8965;
        }
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        ((*s >> 1) & 0x7fff) as i32
    }
}

/// `GOBoid.cpp` ≈1497: 1 de cada 40 ticks, `CreateEffect(MODEL_FIRE, héroe + (rand%600-200,
/// rand%400+200, 300), 0, (1,1,1), 3)`. El meteorito lo mueve y dibuja el motor (`fx/kinds/fire.rs`).
fn spawn_meteors(
    inv: Res<Invasion>,
    mut rng: ResMut<MeteorRng>,
    players: Query<&Player>,
    terrain: Option<Res<LoadedTerrain>>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut sound: ResMut<crate::sound::SoundQueue>,
) {
    if !inv.active() {
        return;
    }
    let (Ok(p), Some(t)) = (players.single(), terrain) else { return };
    if rng.rand() % 40 != 0 {
        return;
    }
    let hero = Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, t.0.height_at(p.pos.x, p.pos.y));
    let (rx, ry) = (rng.rand() % 600 - 200, rng.rand() % 400 + 200);
    let pos = hero + Vec3::new(rx as f32, ry as f32, 300.0);
    world.next_effect_id += 1;
    let id = world.next_effect_id;
    let e = crate::fx::core::Effect::new(id, crate::fx::core::MODEL_FIRE, pos, Vec3::ZERO, Vec3::ONE, 3, None, -1, 0, 0, 0.0);
    crate::fx::request(&mut world, crate::fx::core::Spawn::Effect(e));
    sound.play("SOUND_METEORITE01");
}
