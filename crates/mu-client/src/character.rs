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
            .add_systems(Update, (scripted_walk, click_to_move, cast_skill, walk, animate, light_body).chain());
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
    /// Haciendo una habilidad: su animación manda hasta que acaba (fotogramas por segundo).
    pub casting: Option<f32>,
}

/// Habilidades del Dark Knight en las teclas (sin servidor todavía: se lanzan en el sitio):
/// (tecla, `AT_SKILL_*`, acción de `player.bmd`, velocidad `PlaySpeed` de `SetAttackSpeed`).
const SKILL_KEYS: [(KeyCode, i32, usize, f32); 10] = {
    use crate::fx::core::*;
    [
        (KeyCode::Digit1, AT_SKILL_TWISTING_SLASH, PLAYER_ATTACK_SKILL_WHEEL as usize, 0.24),
        (KeyCode::Digit2, AT_SKILL_RAGEFUL_BLOW, PLAYER_ATTACK_SKILL_FURY_STRIKE as usize, 0.38),
        (KeyCode::Digit3, AT_SKILL_DEATHSTAB, PLAYER_ATTACK_DEATHSTAB as usize, 0.25),
        (KeyCode::Digit4, AT_SKILL_IMPALE, PLAYER_ATTACK_SKILL_SPEAR as usize, 0.30),
        (KeyCode::Digit5, AT_SKILL_SWELL_LIFE, PLAYER_SKILL_VITALITY as usize, 0.34),
        (KeyCode::Digit6, AT_SKILL_FALLING_SLASH, PLAYER_ATTACK_SKILL_SWORD1 as usize, 0.30),
        (KeyCode::Digit7, AT_SKILL_LUNGE, PLAYER_ATTACK_SKILL_SWORD1 as usize + 1, 0.30),
        (KeyCode::Digit8, AT_SKILL_UPPERCUT, PLAYER_ATTACK_SKILL_SWORD1 as usize + 2, 0.27),
        (KeyCode::Digit9, AT_SKILL_CYCLONE, PLAYER_ATTACK_SKILL_SWORD1 as usize + 3, 0.30),
        (KeyCode::Digit0, AT_SKILL_SLASH, PLAYER_ATTACK_SKILL_SWORD1 as usize + 4, 0.24),
    ]
};

/// Lanza una habilidad: animación del personaje y su `AttackTime` en el motor de efectos.
fn cast_skill(
    keys: Res<ButtonInput<KeyCode>>,
    args: Res<Args>,
    mut auto: Local<usize>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut players: Query<(Entity, &mut Player)>,
) {
    let Ok((entity, mut p)) = players.single_mut() else { return };
    if p.casting.is_some() {
        return;
    }
    // `--probar-habilidades`: una tras otra, sin teclado.
    let pressed = if args.skill_test {
        *auto += 1;
        Some(&SKILL_KEYS[(*auto - 1) % SKILL_KEYS.len()])
    } else {
        SKILL_KEYS.iter().find(|(k, ..)| keys.just_pressed(*k))
    };
    let Some(&(_, skill, action, speed)) = pressed else { return };
    p.path.clear();
    p.pos = Vec2::new(p.pos.x.floor() + 0.5, p.pos.y.floor() + 0.5);
    p.action = action;
    p.frame = 0.0;
    // `PlaySpeed` son fotogramas por tick del original (25 por segundo).
    p.casting = Some(speed * crate::fx::sim::TICKS_PER_SECOND);
    // Objetivo: un punto 2 casillas por delante (no hay monstruos todavía).
    let (s, c) = p.facing.sin_cos();
    let ahead = Vec3::new((p.pos.x + 2.0 * s) * MU_SCALE, (p.pos.y - 2.0 * c) * MU_SCALE, 0.0);
    world.casts.push(crate::fx::core::Cast::new(entity, skill, None, ahead));
}

/// Esqueleto + piezas, compartido por la animación.
#[derive(Component)]
struct Skinned {
    skeleton: Bmd,
    /// (modelo de la pieza, índice de malla, malla de Bevy que se actualiza)
    parts: Vec<(usize, usize, Handle<Mesh>)>,
    models: Vec<Bmd>,
    /// El arma de la mano derecha y sus mallas.
    weapon: Option<(Bmd, Vec<(usize, Handle<Mesh>)>)>,
}

/// Piezas del Dark Knight con la armadura inicial (clase 02).
const DARK_KNIGHT: [&str; 5] = ["HelmClass02.bmd", "ArmorClass02.bmd", "PantClass02.bmd", "GloveClass02.bmd", "BootClass02.bmd"];

fn spawn_player(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<crate::model_light::ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
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
    let mut body_materials = vec![];
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
            body_materials.push(material.clone());
            children.push((Mesh3d(handle.clone()), MeshMaterial3d(material)));
            parts.push((model_index, i, handle));
        }
        models.push(model);
    }

    // Arma en la mano derecha (como `RenderLinkObject` con `Link` = true).
    let item_dir = args.data.join("Item");
    let weapon = fs::read(&item_dir, WEAPON_FILE).and_then(|b| bmd::read(&b)).ok().map(|w| {
        let wbones = weapon_bones(&w, &bones, false);
        let mut handles = vec![];
        for i in 0..w.meshes.len() {
            let posed = w.pose_with(i, &wbones);
            if posed.positions.is_empty() {
                continue;
            }
            let mesh = meshes.add(
                Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
            );
            let material = materials.add(part_material(&item_dir, &w.meshes[i].texture, &mut images));
            body_materials.push(material.clone());
            children.push((Mesh3d(mesh.clone()), MeshMaterial3d(material)));
            handles.push((i, mesh));
        }
        (w, handles)
    });
    if weapon.is_none() {
        warn!("falta el arma Item/{WEAPON_FILE}");
    }

    let player = Player { pos, path: VecDeque::new(), facing: 0.0, action: action::STOP_MALE, frame: 0.0, casting: None };
    let actor = actor_state(&player, &bones, terrain.as_deref());
    let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
    let mut e = commands.spawn((
        Name::new("Jugador"),
        player_transform(&player, terrain.as_deref()),
        Visibility::default(),
        player,
        Skinned { skeleton, parts, models, weapon },
        BodyMaterials(body_materials),
        shadow,
        actor,
    ));
    for child in children {
        e.with_child(child);
    }
}

/// Arma inicial del Dark Knight: la espada corta (`MODEL_SWORD + 1` = `Item/Sword02.bmd`).
const WEAPON_FILE: &str = "Sword02.bmd";
const WEAPON_MODEL: i32 = crate::fx::core::MODEL_SWORD + 1;
/// `Weapon[0].LinkBone`: la mano derecha de `player.bmd`.
pub const RIGHT_HAND_BONE: usize = 33;

/// Huesos del arma en la mano: el arma en mano se coloca con `Link = false` (`PlanAttachments`,
/// ZzzCharacter.cpp ≈14257), o sea, directamente con la matriz del hueso de la mano. (El giro
/// (70°, 0, 90°) desplazado (-20, 5, 40) de `PlaceLinkedPart` es para las armas a la espalda.)
///
/// En zona segura el original la lleva a la espalda (`AreWeaponsOnBack`): hueso 47 con `Link`
/// (`PlaceLinkedPart`: `AngleMatrix(70°, 0, 90°)` desplazada (-20, 5, 40) para las espadas).
fn weapon_bones(weapon: &Bmd, player_bones: &[bmd::Mat34], on_back: bool) -> Vec<bmd::Mat34> {
    let local = weapon.bone_matrices(0, 0);
    let parent = if on_back {
        let Some(back) = player_bones.get(BACK_BONE) else { return local };
        let mut m = bmd::angle_matrix([70f32.to_radians(), 0.0, 90f32.to_radians()]);
        m[0][3] = -20.0;
        m[1][3] = 5.0;
        m[2][3] = 40.0;
        bmd::concat(back, &m)
    } else {
        let Some(hand) = player_bones.get(RIGHT_HAND_BONE) else { return local };
        *hand
    };
    local.iter().map(|b| bmd::concat(&parent, b)).collect()
}

/// `CreateWeaponBlur` para el Dark Knight con espada: en las habilidades de espada (SWORD1-5) a
/// partir del fotograma 3, la estela va de (0, -20, 0) a (0, -120, 0) del hueso del arma
/// (`BlurType` 1), con `motion_blur_r` (`BlurMapping` 2), blanca y corta. Cada fotograma se recalcula
/// la pose 10 veces a lo largo del último trozo de animación (`PlaySpeed`) para que salga continua.
fn weapon_blur(entity: Entity, p: &Player, s: &Skinned, terrain: Option<&LoadedTerrain>, world: &mut crate::fx::FxWorld, frame: u64) {
    use crate::fx::core::PLAYER_ATTACK_SKILL_SWORD1;
    let (Some(fps), Some(_)) = (p.casting, &s.weapon) else { return };
    let sword = PLAYER_ATTACK_SKILL_SWORD1 as usize;
    if !(sword..sword + 5).contains(&p.action) || p.frame < 3.0 {
        return;
    }
    let play_speed = fps / crate::fx::sim::TICKS_PER_SECOND;
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    let origin = Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, h);
    let rot = Quat::from_rotation_z(p.facing);
    const INTER: usize = 10;
    let mut f = p.frame - play_speed;
    for _ in 0..INTER {
        let bones = s.skeleton.bone_matrices_at(p.action, f);
        if let Some(m) = bones.get(RIGHT_HAND_BONE) {
            let a = Vec3::from_array(bmd::transform_point(m, [0.0, -20.0, 0.0]));
            let b = Vec3::from_array(bmd::transform_point(m, [0.0, -120.0, 0.0]));
            world.create_blur(entity, origin + rot * a, origin + rot * b, Vec3::ONE, 2, true, frame);
        }
        f += play_speed / INTER as f32;
    }
}

/// Hueso de la espalda donde van las armas en zona segura (`backBone` de `PlanAttachments`).
const BACK_BONE: usize = 47;

/// ¿Va el arma a la espalda? En zona segura, salvo mientras se lanza una habilidad (en el original
/// no se pueden usar en zona segura; aquí se deja para poder probarlas en la ciudad).
fn weapon_on_back(p: &Player, terrain: Option<&LoadedTerrain>) -> bool {
    let safe = terrain.is_some_and(|t| {
        let (x, y) = (p.pos.x as usize, p.pos.y as usize);
        t.0.attribute(x.min(255), y.min(255)) & mu_data::terrain::attr::SAFE_ZONE != 0
    });
    safe && p.casting.is_none()
}

/// Foto del personaje para el motor de efectos (posición en MU, giro, huesos, arma).
fn actor_state(p: &Player, bones: &[bmd::Mat34], terrain: Option<&LoadedTerrain>) -> crate::fx::ActorState {
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    crate::fx::ActorState(crate::fx::Actor {
        position: Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, h),
        angle: Vec3::new(0.0, 0.0, p.facing.to_degrees()),
        scale: 1.0,
        bones: std::sync::Arc::new(bones.to_vec()),
        action: p.action,
        frame: p.frame,
        weapon: Some((WEAPON_MODEL, 0)),
        weapon_bone: RIGHT_HAND_BONE,
        live: true,
        pos_sword: Vec3::ZERO,
    })
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

fn part_material(dir: &Path, texture_name: &str, images: &mut Assets<Image>) -> crate::model_light::ModelMaterial {
    let tex = texture::load(dir, &bmd::texture_file_name(texture_name)).ok();
    // Igual que los objetos: prueba de alfa y profundidad, como el original.
    let alpha_mode = match tex.as_ref().map(|t| t.alpha_kind()) {
        Some(AlphaKind::Cutout | AlphaKind::Blend) => AlphaMode::Mask(crate::objects::ALPHA_REF),
        _ => AlphaMode::Opaque,
    };
    let base = StandardMaterial {
        base_color_texture: tex.map(|t| images.add(to_bevy_image(&t))),
        unlit: true,
        cull_mode: None,
        alpha_mode,
        ..default()
    };
    crate::model_light::model_material(base, crate::model_light::ModelLight::lit([1.0; 3]))
}

/// Materiales del cuerpo y del arma, para darles la luz del sitio donde está el personaje.
#[derive(Component)]
struct BodyMaterials(Vec<Handle<crate::model_light::ModelMaterial>>);

/// `BodyLight`: la luz del terreno bajo el personaje (`RequestTerrainLight`, interpolada).
fn light_body(
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<(&Player, &BodyMaterials)>,
    mut materials: ResMut<Assets<crate::model_light::ModelMaterial>>,
    mut last: Local<Vec4>,
) {
    let Some(t) = terrain else { return };
    let Ok((p, body)) = players.single() else { return };
    let size = mu_data::terrain::SIZE;
    let x = p.pos.x.clamp(0.0, (size - 2) as f32);
    let y = p.pos.y.clamp(0.0, (size - 2) as f32);
    let (xi, yi) = (x as usize, y as usize);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let l = |x: usize, y: usize| Vec3::from_array(t.0.light[y * size + x]);
    let top = l(xi, yi).lerp(l(xi + 1, yi), fx);
    let bottom = l(xi, yi + 1).lerp(l(xi + 1, yi + 1), fx);
    let light = top.lerp(bottom, fy).extend(1.0);
    if light == *last {
        return;
    }
    *last = light;
    for h in &body.0 {
        if let Some(mut m) = materials.get_mut(h) {
            m.extension.light = light;
        }
    }
}

/// Clic izquierdo en el suelo → buscar camino hasta esa casilla.
fn click_to_move(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    terrain: Option<Res<LoadedTerrain>>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    mut players: Query<&mut Player>,
) {
    // Un clic en la interfaz no es para caminar.
    if !buttons.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    let (Some(t), Ok(window), Ok((camera, cam_tf)), Ok(mut player)) =
        (terrain, windows.single(), cameras.single(), players.single_mut())
    else {
        return;
    };
    if player.casting.is_some() {
        return;
    }
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
        if p.casting.is_none() && action != p.action {
            p.action = action;
            p.frame = 0.0;
        }
        *tf = player_transform(&p, terrain.as_deref());
    }
}

fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut render_frame: Local<u64>,
    mut q: Query<(Entity, &mut Player, &Skinned, &mut crate::fx::ActorState, &crate::shadow::Shadow)>,
    mut visibility: Query<&mut Visibility, Without<Player>>,
) {
    *render_frame += 1;
    for (entity, mut p, s, mut actor, shadow) in &mut q {
        weapon_blur(entity, &p, s, terrain.as_deref(), &mut world, *render_frame);
        p.frame += p.casting.unwrap_or(ANIM_FPS) * time.delta_secs();
        // La habilidad termina al acabar su animación: vuelve a estar quieto.
        if p.casting.is_some() {
            let frames = s.skeleton.actions.get(p.action).map_or(1, |a| a.frames) as f32;
            if p.frame >= frames - 1.0 {
                p.casting = None;
                p.action = action::STOP_MALE;
                p.frame = 0.0;
            }
        }
        let bones = s.skeleton.bone_matrices_at(p.action, p.frame);
        let ground = terrain.as_ref().map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
        let mut sh = crate::shadow::Builder::new(
            terrain.as_deref().map(|t| &t.0),
            Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, ground),
            Quat::from_rotation_z(p.facing),
            1.0,
        );
        for (model, mesh, handle) in &s.parts {
            let posed = s.models[*model].pose_with(*mesh, &bones);
            sh.add(&posed.positions);
            if let Some(mut m) = meshes.get_mut(handle) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
        if let Some((w, handles)) = &s.weapon {
            let wbones = weapon_bones(w, &bones, weapon_on_back(&p, terrain.as_deref()));
            for (i, handle) in handles {
                let posed = w.pose_with(*i, &wbones);
                sh.add(&posed.positions);
                if let Some(mut m) = meshes.get_mut(handle) {
                    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
                }
            }
        }
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
        *actor = actor_state(&p, &bones, terrain.as_deref());
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
