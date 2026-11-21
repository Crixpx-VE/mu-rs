//! Personaje jugable: modelo animado (esqueleto de `player.bmd` + piezas de armadura)
//! que camina por el mapa al hacer clic en el suelo.
//!
//! La animación se hace en CPU: cada fotograma se recalculan los vértices de cada pieza
//! con los huesos de la pose actual. Para unos pocos personajes es de sobra rápido.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::texture::AlphaKind;
use mu_data::{fs, path, texture};

use crate::Args;
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

/// Animaciones de `player.bmd` (enum `PLAYER_*` del original).
pub mod action {
    pub const STOP_MALE: usize = 1;
    pub const WALK_MALE: usize = 15;
}

/// Casillas por segundo al caminar.
const WALK_SPEED: f32 = 3.6;
/// Fotogramas de animación por segundo.
const ANIM_FPS: f32 = 8.0;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_player)
            .add_systems(Update, (scripted_walk, click_to_move, walk, animate).chain());
    }
}

#[derive(Component)]
pub struct Player {
    /// Posición en casillas (puede ser fraccionaria mientras camina).
    pub pos: Vec2,
    /// Casillas que faltan por recorrer.
    pub path: VecDeque<(usize, usize)>,
    /// Hacia dónde mira, en radianes alrededor del eje vertical de MU.
    pub facing: f32,
    pub action: usize,
    pub frame: f32,
}

/// Esqueleto + piezas, compartido por la animación.
#[derive(Component)]
struct Skinned {
    skeleton: Bmd,
    /// (modelo de la pieza, índice de malla, malla de Bevy que se actualiza)
    parts: Vec<(usize, usize, Handle<Mesh>)>,
    models: Vec<Bmd>,
}

/// Piezas del Dark Knight con la armadura inicial (clase 02).
const DARK_KNIGHT: [&str; 5] = ["HelmClass02.bmd", "ArmorClass02.bmd", "PantClass02.bmd", "GloveClass02.bmd", "BootClass02.bmd"];

fn spawn_player(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let dir = args.data.join("Player");
    let skeleton = match fs::read(&dir, "player.bmd").and_then(|b| bmd::read(&b)) {
        Ok(s) => s,
        Err(e) => {
            error!("no se pudo cargar player.bmd: {e}");
            return;
        }
    };

    let start = args.spawn.unwrap_or(if args.world == 1 { [133.0, 127.0] } else { [128.0, 128.0] });
    let mut pos = Vec2::from_array(start);
    if let Some(t) = &terrain {
        pos = nearest_walkable(&t.0, pos).unwrap_or(pos);
    }

    let bones = skeleton.bone_matrices(action::STOP_MALE, 0);
    let mut models = vec![];
    let mut parts = vec![];
    let mut children = vec![];
    for name in DARK_KNIGHT {
        let Ok(model) = fs::read(&dir, name).and_then(|b| bmd::read(&b)) else {
            warn!("falta la pieza {name}");
            continue;
        };
        let model_index = models.len();
        for i in 0..model.meshes.len() {
            let posed = model.pose_with(i, &bones);
            if posed.positions.is_empty() {
                continue;
            }
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs);
            let handle = meshes.add(mesh);
            let material = materials.add(part_material(&dir, &model.meshes[i].texture, &mut images));
            children.push((Mesh3d(handle.clone()), MeshMaterial3d(material)));
            parts.push((model_index, i, handle));
        }
        models.push(model);
    }

    let player = Player { pos, path: VecDeque::new(), facing: 0.0, action: action::STOP_MALE, frame: 0.0 };
    let mut e = commands.spawn((
        Name::new("Jugador"),
        player_transform(&player, terrain.as_deref()),
        Visibility::default(),
        player,
        Skinned { skeleton, parts, models },
    ));
    for child in children {
        e.with_child(child);
    }
}

/// `--walk-to`: orden de caminar sin ratón. En modo captura se da 1 s antes de capturar,
/// para que la foto pille al personaje caminando.
fn scripted_walk(
    args: Res<Args>,
    time: Res<Time<Real>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut done: Local<bool>,
    mut players: Query<&mut Player>,
) {
    let (false, Some(target), Some(t), Ok(mut p)) = (*done, args.walk_to, terrain, players.single_mut()) else {
        return;
    };
    if args.screenshot.is_some() && time.elapsed_secs() < args.screenshot_delay - 1.0 {
        return;
    }
    *done = true;
    let from = (p.pos.x as usize, p.pos.y as usize);
    match path::find_path(from, (target[0] as usize, target[1] as usize), |x, y| t.0.walkable(x, y), 20_000) {
        Some(path) => p.path = path.into_iter().skip(1).collect(),
        None => warn!("--walk-to: no hay camino hasta {target:?}"),
    }
}

fn part_material(dir: &Path, texture_name: &str, images: &mut Assets<Image>) -> StandardMaterial {
    let tex = texture::load(dir, &bmd::texture_file_name(texture_name)).ok();
    // Igual que los objetos: prueba de alfa y profundidad, como el original.
    let alpha_mode = match tex.as_ref().map(|t| t.alpha_kind()) {
        Some(AlphaKind::Cutout | AlphaKind::Blend) => AlphaMode::Mask(crate::objects::ALPHA_REF),
        _ => AlphaMode::Opaque,
    };
    StandardMaterial {
        base_color_texture: tex.map(|t| images.add(to_bevy_image(&t))),
        unlit: true,
        cull_mode: None,
        alpha_mode,
        ..default()
    }
}

/// Clic izquierdo en el suelo → buscar camino hasta esa casilla.
fn click_to_move(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    terrain: Option<Res<LoadedTerrain>>,
    mut players: Query<&mut Player>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let (Some(t), Ok(window), Ok((camera, cam_tf)), Ok(mut player)) =
        (terrain, windows.single(), cameras.single(), players.single_mut())
    else {
        return;
    };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else { return };
    let Some(hit) = raycast_terrain(&t.0, ray) else { return };

    let from = (player.pos.x as usize, player.pos.y as usize);
    let to = (hit.x as usize, hit.y as usize);
    if let Some(p) = path::find_path(from, to, |x, y| t.0.walkable(x, y), 20_000) {
        player.path = p.into_iter().skip(1).collect();
    }
}

/// Punto del terreno (en casillas de MU) bajo un rayo de la cámara.
fn raycast_terrain(t: &mu_data::terrain::Terrain, ray: Ray3d) -> Option<Vec2> {
    let ground = |p: Vec3| t.height_at(p.x, -p.z) / MU_SCALE;
    let mut prev = 0.0;
    let mut d = 0.0;
    while d < 400.0 {
        let p = ray.get_point(d);
        if p.y <= ground(p) {
            // Afinar entre el paso anterior y este.
            let (mut a, mut b) = (prev, d);
            for _ in 0..12 {
                let m = (a + b) / 2.0;
                if ray.get_point(m).y <= ground(ray.get_point(m)) { b = m } else { a = m }
            }
            let hit = ray.get_point(b);
            return Some(Vec2::new(hit.x, -hit.z));
        }
        prev = d;
        d += 0.25;
    }
    None
}

fn walk(time: Res<Time>, terrain: Option<Res<LoadedTerrain>>, mut q: Query<(&mut Player, &mut Transform)>) {
    for (mut p, mut tf) in &mut q {
        let mut budget = WALK_SPEED * time.delta_secs();
        while budget > 0.0 {
            let Some(&(nx, ny)) = p.path.front() else { break };
            let target = Vec2::new(nx as f32 + 0.5, ny as f32 + 0.5);
            let delta = target - p.pos;
            let dist = delta.length();
            if dist > 1e-4 {
                // MU: ángulo 0 = mirando hacia -Y; crece en sentido antihorario.
                p.facing = delta.x.atan2(-delta.y);
            }
            if dist <= budget {
                p.pos = target;
                budget -= dist;
                p.path.pop_front();
            } else {
                p.pos += delta / dist * budget;
                budget = 0.0;
            }
        }
        let action = if p.path.is_empty() { action::STOP_MALE } else { action::WALK_MALE };
        if action != p.action {
            p.action = action;
            p.frame = 0.0;
        }
        *tf = player_transform(&p, terrain.as_deref());
    }
}

fn animate(time: Res<Time>, mut meshes: ResMut<Assets<Mesh>>, mut q: Query<(&mut Player, &Skinned)>) {
    for (mut p, s) in &mut q {
        p.frame += ANIM_FPS * time.delta_secs();
        let bones = s.skeleton.bone_matrices_at(p.action, p.frame);
        for (model, mesh, handle) in &s.parts {
            let posed = s.models[*model].pose_with(*mesh, &bones);
            if let Some(mut m) = meshes.get_mut(handle) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
    }
}

/// Transformación del personaje: posición sobre el terreno y giro hacia donde mira.
pub fn player_transform(p: &Player, terrain: Option<&LoadedTerrain>) -> Transform {
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    Transform {
        translation: Vec3::new(p.pos.x, h / MU_SCALE, -p.pos.y),
        rotation: to_bevy * Quat::from_rotation_z(p.facing),
        scale: Vec3::splat(1.0 / MU_SCALE),
    }
}

fn nearest_walkable(t: &mu_data::terrain::Terrain, p: Vec2) -> Option<Vec2> {
    let (cx, cy) = (p.x as i32, p.y as i32);
    for r in 0..20i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && t.walkable(x as usize, y as usize) {
                    return Some(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                }
            }
        }
    }
    None
}
