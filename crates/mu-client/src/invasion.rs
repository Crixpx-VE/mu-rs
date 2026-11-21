//! Ambiente de invasión de dragones (`EnableEvent` del original; el servidor lo activa con el
//! paquete C1 0B <encendido> <evento>: evento 1 = dragones rojos, 3 = dorados).
//!
//! Lo que cambia mientras está activo (GOBoid.cpp / ZzzLodTerrain.cpp):
//! - los pájaros dejan paso a dragones que cruzan el cielo (ver `boids.rs`);
//! - cae un meteorito (`Skill/Fire01.bmd`) 1 de cada 40 ticks cerca del personaje;
//! - una zona más oscura de radio 16 casillas sigue al personaje (luz -0.3, -0.3, -0.2);
//! - la hierba se mueve 5 veces más rápido (ver `grass.rs`).
//!
//! Sin servidor todavía: `F9` lo va cambiando (apagado → rojo → dorado → apagado).

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::{fs, texture};

use crate::Args;
use crate::character::Player;
use crate::terrain::{LoadedTerrain, MU_SCALE, TerrainLights, to_bevy_image};

pub struct InvasionPlugin;

impl Plugin for InvasionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Invasion>()
            .add_systems(Startup, (setup_meteor, spawn_label, |args: Res<Args>, mut inv: ResMut<Invasion>| {
                inv.0 = args.invasion;
            }))
            .add_systems(Update, (toggle, update_label, draw_meteors))
            .add_systems(FixedUpdate, (darken_around_hero, step_meteors));
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

fn toggle(keys: Res<ButtonInput<KeyCode>>, mut inv: ResMut<Invasion>) {
    if keys.just_pressed(KeyCode::F9) {
        inv.0 = match inv.0 {
            0 => 1,
            1 => 3,
            _ => 0,
        };
        info!("invasión de dragones: {}", inv.0);
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
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, top: Val::Px(14.0), left: Val::Px(4.0), ..default() },
    ));
}

fn update_label(inv: Res<Invasion>, mut q: Query<&mut Text, With<InvasionLabel>>) {
    if !inv.is_changed() {
        return;
    }
    for mut t in &mut q {
        t.0 = match inv.0 {
            1 => "Invasión de dragones rojos (F9)".into(),
            3 => "Invasión de dragones dorados (F9)".into(),
            _ => String::new(),
        };
    }
}

// --- Meteoritos -------------------------------------------------------------------------

/// Modelo del meteorito, ya en mallas de Bevy (una entidad por meteorito, que se recicla).
#[derive(Resource)]
struct MeteorModel {
    parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
}

struct Meteor {
    entity: Entity,
    pos: Vec3,
    prev: Vec3,
    life: i32,
}

#[derive(Resource, Default)]
struct Meteors {
    list: Vec<Meteor>,
    rng: u32,
}

impl Meteors {
    fn rand(&mut self) -> i32 {
        let s = &mut self.rng;
        if *s == 0 {
            *s = 0x6C07_8965;
        }
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        (*s >> 1) as i32
    }
}

fn setup_meteor(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.init_resource::<Meteors>();
    let dir = args.data.join("Skill");
    let Ok(model) = fs::read(&dir, "Fire01.bmd").and_then(|b| bmd::read(&b)) else {
        warn!("sin Skill/Fire01.bmd: no habrá meteoritos");
        return;
    };
    let model: Arc<Bmd> = Arc::new(model);
    let mut parts = vec![];
    for (i, m) in model.meshes.iter().enumerate() {
        let posed = model.pose(i, 0, 0);
        if posed.positions.is_empty() {
            continue;
        }
        let mesh = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
        );
        let tex = texture::load(&dir, &bmd::texture_file_name(&m.texture)).ok().map(|t| images.add(to_bevy_image(&t)));
        // `BlendMesh = 1`: la malla con textura 1 (el fuego) se suma a lo de detrás.
        let additive = m.texture_index == 1;
        let mat = materials.add(StandardMaterial {
            base_color_texture: tex,
            unlit: true,
            cull_mode: None,
            alpha_mode: if additive { AlphaMode::Add } else { AlphaMode::Opaque },
            ..default()
        });
        parts.push((mesh, mat));
    }
    commands.insert_resource(MeteorModel { parts });
}

/// Un tick: nace un meteorito 1 de cada 40 y los que hay avanzan (`MODEL_FIRE` subtipo 3).
fn step_meteors(
    mut commands: Commands,
    inv: Res<Invasion>,
    model: Option<Res<MeteorModel>>,
    mut meteors: ResMut<Meteors>,
    players: Query<&Player>,
    terrain: Option<Res<LoadedTerrain>>,
) {
    let Some(model) = model else { return };
    // Avanzar: 12 hacia -Y por movimiento, y la mitad de las veces se mueve dos veces en el tick.
    let extra: Vec<bool> = (0..meteors.list.len()).map(|_| meteors.rand() % 2 == 0).collect();
    for (m, twice) in meteors.list.iter_mut().zip(extra) {
        m.prev = m.pos;
        for _ in 0..if twice { 2 } else { 1 } {
            m.pos.y -= 12.0;
            m.life -= 1;
        }
    }
    meteors.list.retain(|m| {
        let alive = m.life > 0;
        if !alive {
            commands.entity(m.entity).despawn();
        }
        alive
    });

    if !inv.active() {
        return;
    }
    let (Ok(p), Some(t)) = (players.single(), terrain) else { return };
    if meteors.rand() % 40 != 0 {
        return;
    }
    let hero = Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, t.0.height_at(p.pos.x, p.pos.y));
    let (rx, ry) = (meteors.rand() % 600 - 200, meteors.rand() % 400 + 200);
    let pos = hero + Vec3::new(rx as f32, ry as f32, 300.0);
    let entity = commands
        .spawn((Name::new("Meteorito"), Transform::default(), Visibility::Hidden))
        .with_children(|c| {
            for (mesh, mat) in &model.parts {
                c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
            }
        })
        .id();
    meteors.list.push(Meteor { entity, pos, prev: pos, life: 80 });
}

fn draw_meteors(meteors: Option<Res<Meteors>>, fixed: Res<Time<Fixed>>, mut q: Query<(&mut Transform, &mut Visibility)>) {
    let Some(meteors) = meteors else { return };
    let a = fixed.overstep_fraction();
    for m in &meteors.list {
        let Ok((mut tf, mut vis)) = q.get_mut(m.entity) else { continue };
        let p = m.prev.lerp(m.pos, a);
        *tf = Transform {
            translation: Vec3::new(p.x, p.z, -p.y) / MU_SCALE,
            rotation: Quat::from_rotation_x(-FRAC_PI_2),
            scale: Vec3::splat(0.3 / MU_SCALE),
        };
        *vis = Visibility::Inherited;
    }
}
