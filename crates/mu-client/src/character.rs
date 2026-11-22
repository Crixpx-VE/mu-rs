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
    use crate::fx::tables::ids;
    pub const STOP_MALE: usize = 1;
    pub const WALK_MALE: usize = 15;
    pub const SIT1: usize = ids::PLAYER_SIT1 as usize;
    /// `PLAYER_SIT1..=PLAYER_POSE_FEMALE1`: sentado, curándose o apoyado (se queda así hasta moverse).
    pub fn resting(a: usize) -> bool {
        (SIT1..=ids::PLAYER_POSE_FEMALE1 as usize).contains(&a)
    }

    /// Los gestos (`PLAYER_GREETING1..=PLAYER_RUSH1`): caen en el `PLAYER_ATTACK_FIST..PLAYER_SHOCK`
    /// de `PlayerStopAnimationSetting` (ZzzCharacter.cpp:3264-3282), así que al acabar la animación
    /// vuelve a estar parado (`SetPlayerStop`).
    pub fn emote(a: usize) -> bool {
        (ids::PLAYER_GREETING1 as usize..=ids::PLAYER_RUSH1 as usize).contains(&a)
    }

    /// `ReceiveAction` (WSclient.cpp:3142-3350): la animación de `player.bmd` de una acción `AT_*`
    /// (las de mujer, uno más; sentarse, dos). `Some(None)` = `AT_STAND1/2` (`SetPlayerStop`);
    /// `None` = ninguna que se sepa poner (ataques y daño, sin armas ni monstruos todavía).
    pub fn for_at(at: u8, female: bool) -> Option<Option<usize>> {
        use mu_protocol::game::action as at_;
        let f = female as i32;
        let a = match at {
            at_::STAND1 | at_::STAND2 => return Some(None),
            at_::ATTACK1 | at_::ATTACK2 | at_::DAMAGE1 => return None,
            at_::SIT1 => ids::PLAYER_SIT1 + 2 * f,
            at_::POSE1 => ids::PLAYER_POSE1 + f,
            at_::HEALING1 => ids::PLAYER_HEALING1 + f,
            at_::GREETING1 => ids::PLAYER_GREETING1 + f,
            at_::GOODBYE1 => ids::PLAYER_GOODBYE1 + f,
            at_::CLAP1 => ids::PLAYER_CLAP1 + f,
            at_::GESTURE1 => ids::PLAYER_GESTURE1 + f,
            at_::DIRECTION1 => ids::PLAYER_DIRECTION1 + f,
            at_::UNKNOWN1 => ids::PLAYER_UNKNOWN1 + f,
            at_::CRY1 => ids::PLAYER_CRY1 + f,
            at_::AWKWARD1 => ids::PLAYER_AWKWARD1 + f,
            at_::SEE1 => ids::PLAYER_SEE1 + f,
            at_::CHEER1 => ids::PLAYER_CHEER1 + f,
            at_::WIN1 => ids::PLAYER_WIN1 + f,
            at_::SMILE1 => ids::PLAYER_SMILE1 + f,
            at_::SLEEP1 => ids::PLAYER_SLEEP1 + f,
            at_::COLD1 => ids::PLAYER_COLD1 + f,
            at_::AGAIN1 => ids::PLAYER_AGAIN1 + f,
            at_::RESPECT1 => ids::PLAYER_RESPECT1,
            at_::SALUTE1 => ids::PLAYER_SALUTE1,
            at_::RUSH1 => ids::PLAYER_RUSH1,
            at_::SCISSORS => ids::PLAYER_SCISSORS,
            at_::ROCK => ids::PLAYER_ROCK,
            at_::PAPER => ids::PLAYER_PAPER,
            at_::HUSTLE => ids::PLAYER_HUSTLE,
            at_::PROVOCATION => ids::PLAYER_PROVOCATION,
            at_::LOOK_AROUND => ids::PLAYER_LOOK_AROUND,
            at_::CHEERS => ids::PLAYER_CHEERS,
            // `default: SetAction(&c->Object, Data->Action)`.
            other => other as i32,
        };
        Some(Some(a as usize))
    }

    /// `SetActionClass` (ZzzInterface.cpp:4416-4432): solo se pone estando parado (`PLAYER_STOP_MALE..
    /// =PLAYER_STOP_RIDE_WEAPON`, `PLAYER_STOP_TWO_HAND_SWORD_TWO` o el del Rage Fighter).
    pub fn standing(a: usize) -> bool {
        let a = a as i32;
        (ids::PLAYER_STOP_MALE..=ids::PLAYER_STOP_RIDE_WEAPON).contains(&a)
            || a == ids::PLAYER_STOP_TWO_HAND_SWORD_TWO
            || a == ids::PLAYER_STOP_RAGEFIGHTER
    }
}

/// `((Angle[2] + 22.5) / 360 * 8 + 1) % 8`: la dirección que mandan `SendRequestAction` y el
/// `SendCharacterMove` de una casilla (wsclientinline.h:454-457), con el ángulo en [0, 360).
pub fn facing_direction(facing: f32) -> u8 {
    let deg = facing.to_degrees().rem_euclid(360.0);
    (((deg + 22.5) / 360.0 * 8.0 + 1.0) as u32 % 8) as u8
}

/// Casillas por segundo al caminar.
const WALK_SPEED: f32 = 3.6;
/// Fotogramas de animación por segundo.
const ANIM_FPS: f32 = 8.0;

/// `SetPlayerStop` sin arma (o en zona segura): la Elfa `STOP_FEMALE`, la Summoner
/// `STOP_SUMMONER` y el resto `STOP_MALE`; con la espada en la mano fuera de zona segura, el Dark
/// Knight `STOP_SWORD`.
pub fn stop_action(class: crate::account::Class, sword_in_hand: bool) -> usize {
    use crate::account::Class;
    use crate::fx::tables::ids;
    match class {
        Class::Knight if sword_in_hand => ids::PLAYER_STOP_SWORD as usize,
        Class::Elf => ids::PLAYER_STOP_FEMALE as usize,
        Class::Summoner => ids::PLAYER_STOP_SUMMONER as usize,
        // MuMain ZzzCharacter.cpp:416.
        Class::Fighter => ids::PLAYER_STOP_RAGEFIGHTER as usize,
        _ => action::STOP_MALE,
    }
}

/// `SetPlayerWalk` sin arma: `WALK_FEMALE` las mujeres y `WALK_MALE` el resto.
pub fn walk_action(class: crate::account::Class) -> usize {
    if class.female() { crate::fx::tables::ids::PLAYER_WALK_FEMALE as usize } else { action::WALK_MALE }
}

/// `PlaySpeed` (fotogramas por tick de 25 Hz) de las animaciones que usan la selección y el juego
/// (ZzzOpenData.cpp ≈330-361; `SetPlayerWalk` pone 0.33 a las de caminar).
pub fn play_speed(a: usize) -> f32 {
    use crate::fx::tables::ids;
    let a = a as i32;
    if a == ids::PLAYER_STOP_SWORD {
        0.26
    } else if a == ids::PLAYER_STOP_SUMMONER {
        0.24
    } else if a == ids::PLAYER_STOP_RAGEFIGHTER {
        // MuMain ZzzOpenData.cpp:243.
        0.16
    } else if (ids::PLAYER_STOP_MALE..=ids::PLAYER_STOP_RIDE_WEAPON).contains(&a) {
        0.28
    } else if (ids::PLAYER_WALK_MALE..=ids::PLAYER_WALK_CROSSBOW).contains(&a) {
        0.33
    } else if a == ids::PLAYER_HEALING1 || a == ids::PLAYER_HEALING_FEMALE1 {
        0.2
    } else if a == ids::PLAYER_SEE1 || a == ids::PLAYER_SEE_FEMALE1 {
        // ZzzOpenData.cpp:358-359 (el resto de gestos, `PLAYER_DEFENSE1..=PLAYER_SHOCK`, a 0.32).
        0.28
    } else if action::resting(a as usize) {
        0.4
    } else {
        ANIM_FPS / crate::fx::sim::TICKS_PER_SECOND
    }
}

/// Velocidad de la animación en fotogramas por segundo.
fn action_fps(a: usize) -> f32 {
    play_speed(a) * crate::fx::sim::TICKS_PER_SECOND
}

/// El ratón manda al personaje (para ordenar los sistemas que leen lo que hay bajo el ratón).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeroInput;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentSkill>()
            .add_systems(Update, spawn_player.run_if(crate::scene::load_step(crate::scene::Step::Player)))
            .add_systems(Update, (scripted_walk, hero_input.in_set(HeroInput), cast_skill, walk, animate, light_body).chain());
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
    /// `MOVEMENT_OPERATE`: va hacia este objeto de sentarse (índice en `Operates`).
    pub operate: Option<usize>,
    pub class: crate::account::Class,
    /// Animaciones de parado y de caminar de su clase.
    pub stop: usize,
    pub walk: usize,
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

/// Habilidad elegida (`Hero->CurrentSkill`): índice en `SKILL_KEYS`.
#[derive(Resource, Default)]
pub struct CurrentSkill(pub Option<usize>);

/// Como el original: `1`-`0` eligen la habilidad de ese atajo (`UseHotKey`) y el botón derecho la
/// usa hacia donde apunta el ratón; en zona segura no se usan (`CheckAttack`). Sin la lista de
/// habilidades todavía, los atajos traen las del Dark Knight en orden.
#[allow(clippy::too_many_arguments)]
fn cast_skill(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    args: Res<Args>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<IsDefaultUiCamera>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut current: ResMut<CurrentSkill>,
    mut auto: Local<usize>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut players: Query<(Entity, &mut Player)>,
) {
    let Ok((entity, mut p)) = players.single_mut() else { return };
    // Las habilidades de los atajos son las del Dark Knight.
    if p.class != crate::account::Class::Knight {
        return;
    }
    if let Some(i) = SKILL_KEYS.iter().position(|(k, ..)| keys.just_pressed(*k)) {
        current.0 = Some(i);
    }
    if p.casting.is_some() {
        return;
    }
    // `--probar-habilidades`: una tras otra, sin ratón y en cualquier sitio.
    let (index, target) = if args.skill_test {
        *auto += 1;
        ((*auto - 1) % SKILL_KEYS.len(), None)
    } else {
        let Some(i) = current.0 else { return };
        if !buttons.just_pressed(MouseButton::Right) || over_ui.0 {
            return;
        }
        let safe = terrain.as_ref().is_some_and(|t| {
            t.0.attribute((p.pos.x as usize).min(255), (p.pos.y as usize).min(255)) & mu_data::terrain::attr::SAFE_ZONE != 0
        });
        if safe {
            return;
        }
        // Punto del suelo bajo el ratón.
        let hit = windows.single().ok().zip(cameras.single().ok()).zip(terrain.as_ref()).and_then(|((w, (cam, tf)), t)| {
            let ray = cam.viewport_to_world(tf, w.cursor_position()?).ok()?;
            raycast_terrain(&t.0, ray)
        });
        (i, hit)
    };
    let (_, skill, action, speed) = SKILL_KEYS[index];
    p.path.clear();
    p.pos = Vec2::new(p.pos.x.floor() + 0.5, p.pos.y.floor() + 0.5);
    // Mira hacia el objetivo.
    if let Some(t) = target {
        let d = t - p.pos;
        if d.length() > 1e-3 {
            p.facing = d.x.atan2(-d.y);
        }
    }
    p.action = action;
    p.frame = 0.0;
    // `PlaySpeed` son fotogramas por tick del original (25 por segundo).
    p.casting = Some(speed * crate::fx::sim::TICKS_PER_SECOND);
    // Sin monstruos todavía: el objetivo es el punto del ratón (o 2 casillas por delante).
    let (s, c) = p.facing.sin_cos();
    let at = target.unwrap_or(Vec2::new(p.pos.x + 2.0 * s, p.pos.y - 2.0 * c));
    world.casts.push(crate::fx::core::Cast::new(entity, skill, None, Vec3::new(at.x * MU_SCALE, at.y * MU_SCALE, 0.0)));
}

/// Esqueleto + piezas, compartido por la animación.
#[derive(Component)]
pub struct Skinned {
    skeleton: Bmd,
    /// (modelo de la pieza, índice de malla, malla de Bevy que se actualiza)
    parts: Vec<(usize, usize, Handle<Mesh>)>,
    models: Vec<Bmd>,
    /// El arma de la mano derecha y sus mallas.
    weapon: Option<(Bmd, Vec<(usize, Handle<Mesh>)>)>,
}

impl Skinned {
    /// Pone las piezas y el arma (en la mano o a la espalda) en la pose `action`/`frame` y añade
    /// sus vértices a la sombra. Devuelve los huesos de esa pose.
    pub fn pose(
        &self,
        action: usize,
        frame: f32,
        weapon_on_back: bool,
        meshes: &mut Assets<Mesh>,
        sh: &mut crate::shadow::Builder,
    ) -> Vec<bmd::Mat34> {
        let bones = self.skeleton.bone_matrices_at(action, frame);
        for (model, mesh, handle) in &self.parts {
            let posed = self.models[*model].pose_with(*mesh, &bones);
            sh.add(&posed.positions);
            if let Some(mut m) = meshes.get_mut(handle) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
        if let Some((w, handles)) = &self.weapon {
            let wbones = weapon_bones(w, &bones, weapon_on_back);
            for (i, handle) in handles {
                let posed = w.pose_with(*i, &wbones);
                sh.add(&posed.positions);
                if let Some(mut m) = meshes.get_mut(handle) {
                    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
                }
            }
        }
        bones
    }

    /// Fotogramas de la animación `action` de `player.bmd` (0 si no la tiene).
    pub fn action_frames(&self, action: usize) -> usize {
        self.skeleton.actions.get(action).map_or(0, |a| a.frames)
    }

    /// Los huesos de la pose `action`/`frame` (los mismos que usa [`Skinned::pose`]).
    pub fn bones_at(&self, action: usize, frame: f32) -> Vec<bmd::Mat34> {
        self.skeleton.bone_matrices_at(action, frame)
    }

    /// Las mallas de las piezas del cuerpo, sin el arma: (modelo de la pieza, índice de malla).
    pub fn body_meshes(&self) -> impl Iterator<Item = (&Bmd, usize)> + '_ {
        self.parts.iter().map(|(model, mesh, _)| (&self.models[*model], *mesh))
    }
}

/// Un personaje cargado: esqueleto y piezas, las mallas (hijas de la entidad del personaje)
/// y sus materiales (para darles luz).
pub struct CharacterModel {
    pub skinned: Skinned,
    pub children: Vec<(Mesh3d, MeshMaterial3d<crate::model_light::ModelMaterial>)>,
    pub materials: Vec<Handle<crate::model_light::ModelMaterial>>,
    /// Huesos de la pose inicial.
    pub bones: Vec<bmd::Mat34>,
}

/// Carga `player.bmd` y las piezas sin equipo de la clase (`{Helm,Armor,Pant,Glove,Boot}ClassNN`),
/// en la pose `action` (fotograma 0); el Dark Knight, con la espada en la mano. `None` si falta
/// el esqueleto.
pub fn load_character(
    args: &Args,
    class: crate::account::Class,
    action: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<crate::model_light::ModelMaterial>,
    images: &mut Assets<Image>,
) -> Option<CharacterModel> {
    let dir = args.data.join("Player");
    let skeleton = match fs::read(&dir, "player.bmd").and_then(|b| bmd::read(&b)) {
        Ok(s) => s,
        Err(e) => {
            error!("no se pudo cargar player.bmd: {e}");
            return None;
        }
    };

    let bones = skeleton.bone_matrices(action, 0);
    let mut models = vec![];
    let mut parts = vec![];
    let mut children = vec![];
    let mut body_materials = vec![];
    for part in BODY_PARTS {
        let name = &format!("{part}Class{:02}.bmd", class.skin_file());
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
            let material = materials.add(part_material(&dir, &model.meshes[i].texture, images));
            body_materials.push(material.clone());
            children.push((Mesh3d(handle.clone()), MeshMaterial3d(material)));
            parts.push((model_index, i, handle));
        }
        models.push(model);
    }

    // Arma en la mano derecha (como `RenderLinkObject` con `Link` = true).
    let item_dir = args.data.join("Item");
    let knight = class == crate::account::Class::Knight;
    let weapon = knight.then(|| fs::read(&item_dir, WEAPON_FILE).and_then(|b| bmd::read(&b)).ok()).flatten().map(|w| {
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
            let material = materials.add(part_material(&item_dir, &w.meshes[i].texture, images));
            body_materials.push(material.clone());
            children.push((Mesh3d(mesh.clone()), MeshMaterial3d(material)));
            handles.push((i, mesh));
        }
        (w, handles)
    });
    if knight && weapon.is_none() {
        warn!("falta el arma Item/{WEAPON_FILE}");
    }
    Some(CharacterModel { skinned: Skinned { skeleton, parts, models, weapon }, children, materials: body_materials, bones })
}

/// Piezas del cuerpo sin equipo (`BodyPart[BODYPART_HELM..BOOTS]`).
const BODY_PARTS: [&str; 5] = ["Helm", "Armor", "Pant", "Glove", "Boot"];

fn spawn_player(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<crate::model_light::ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut stats: ResMut<crate::mainframe::CharacterStats>,
) {
    let class = hero.map_or(crate::account::Class::Knight, |h| {
        info!("entra {} ({} nivel {})", h.name, h.class.name(), h.level);
        h.class
    });
    let (stop, walk) = (stop_action(class, false), walk_action(class));
    let Some(CharacterModel { skinned, children, materials: body_materials, bones }) =
        load_character(&args, class, stop, &mut meshes, &mut materials, &mut images)
    else {
        return;
    };

    // Con servidor, donde diga (`CharacterInformation`); si no, `--spawn` o el centro del mapa.
    let from_server = info.as_ref().map(|i| [i.0.x as f32 + 0.5, i.0.y as f32 + 0.5]);
    let start = from_server.or(args.spawn).unwrap_or(if args.world == 1 { [133.0, 127.0] } else { [128.0, 128.0] });
    let mut pos = Vec2::from_array(start);
    if let Some(t) = &terrain {
        pos = nearest_walkable(&t.0, pos).unwrap_or(pos);
    }
    // `Direction` (0..7, como los pasos de caminar) → ángulo de MU.
    let facing = info.as_ref().map_or(0.0, |i| direction_facing(i.0.direction));
    if let Some(i) = &info {
        let i = &i.0;
        *stats = crate::mainframe::CharacterStats {
            hp: i.health as u32,
            hp_max: i.max_health as u32,
            mp: i.mana as u32,
            mp_max: i.max_mana as u32,
            ag: i.ability as u32,
            ag_max: i.max_ability as u32,
            sd: i.shield as u32,
            sd_max: i.max_shield as u32,
            ..default()
        };
    }

    let player = Player {
        pos,
        path: VecDeque::new(),
        facing,
        action: stop,
        frame: 0.0,
        casting: None,
        operate: None,
        class,
        stop,
        walk,
    };
    let actor = actor_state(&player, &bones, terrain.as_deref());
    let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
    let mut e = commands.spawn((
        Name::new("Jugador"),
        crate::scene::WorldEntity,
        player_transform(&player, terrain.as_deref()),
        Visibility::default(),
        player,
        skinned,
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
            let k = p.class.game_scale();
            world.create_blur(entity, origin + rot * (a * k), origin + rot * (b * k), Vec3::ONE, 2, true, frame);
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
        scale: p.class.game_scale(),
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

pub(crate) fn part_material(dir: &Path, texture_name: &str, images: &mut Assets<Image>) -> crate::model_light::ModelMaterial {
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

/// `c->Light` del héroe en el juego: `AbilityLight` = (1, 1, 1), sin la luz del terreno
/// (ZzzCharacter.cpp:8967-8993; las habilidades que la tiñen aún no existen). Los demás
/// personajes sí llevan la del terreno (remote.rs).
fn light_body(
    players: Query<&BodyMaterials, With<Player>>,
    mut materials: ResMut<Assets<crate::model_light::ModelMaterial>>,
    mut last: Local<Vec4>,
) {
    let Ok(body) = players.single() else { return };
    let light = Vec4::ONE;
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

/// `MouseUpdateTime` / `MouseUpdateTimeMax` / `MouseLButtonPush` del original, en ticks de 25 Hz.
#[derive(Default)]
struct MouseTimer {
    time: u32,
    max: u32,
    /// `MouseLButtonPush`: clic sin atender (se pierde al soltar el botón).
    push: bool,
    /// El botón se apretó en la zona de juego (no sobre la barra o una ventana).
    held: bool,
    /// Fracción de tick acumulada.
    acc: f32,
}

/// `PathFinding2` → `GetPath`: el camino se corta a `MAX_PATH_FIND` (15) casillas contando la de
/// salida; con el botón apretado se vuelve a pedir y sigue.
const MAX_PATH_FIND: usize = 15;

/// Camino de `from` a `to` (con la casilla de salida), si hay y tiene más de una casilla.
fn hero_path(t: &mu_data::terrain::Terrain, from: (usize, usize), to: (usize, usize)) -> Option<Vec<(usize, usize)>> {
    let mut p = path::find_path(from, to, |x, y| t.walkable(x, y), 20_000)?;
    p.truncate(MAX_PATH_FIND);
    (p.len() > 1).then_some(p)
}

/// `SendMove`: el tiempo hasta atender otro clic depende de lo largo del camino; el camino va al
/// servidor (`SendCharacterMove`: casilla de salida, dirección y un paso por casilla).
fn send_move(player: &mut Player, path: Vec<(usize, usize)>, timer: &mut MouseTimer, net: &mut crate::net::Net) {
    timer.max = match path.len() {
        0..=2 => 0,
        3 => 5,
        n => 10 + (n as u32 - 2) * 3,
    };
    if net.connected() {
        let tiles: Vec<(u8, u8)> = path.iter().map(|&(x, y)| (x.min(255) as u8, y.min(255) as u8)).collect();
        let directions: Vec<u8> =
            tiles.windows(2).filter_map(|w| mu_protocol::game::direction_between(w[0], w[1])).collect();
        if let (Some(&(sx, sy)), Some(&first)) = (tiles.first(), directions.first()) {
            net.send(mu_protocol::game::ClientMessage::Walk { source_x: sx, source_y: sy, rotation: first, directions });
        }
    }
    player.path = path.into_iter().skip(1).collect();
}

/// Ángulo de MU (el de `Player::facing`) de una dirección de paso (0..7).
pub fn direction_facing(direction: u8) -> f32 {
    let (dx, dy) = mu_protocol::game::DIRECTIONS[(direction & 7) as usize];
    (dx as f32).atan2(-(dy as f32))
}

/// El ratón en el juego (`MoveHero`, ZzzInterface.cpp ≈7340-7636), una vez por tick:
/// - Cada vez que `MouseUpdateTime` llega a `MouseUpdateTimeMax`, el clic (o el botón mantenido)
///   vuelve a mandar al personaje hacia donde apunta el ratón: mantener el botón lo hace seguir
///   al ratón.
/// - Sobre un objeto de sentarse (y quieto), va hasta él (`MOVEMENT_OPERATE`).
/// - En el suelo, `DontMove` (cursor `CursorDontMove`) si la casilla no tiene suelo, es agua...
///   (atributo ≥ `TW_NOGROUND` sin `TW_ACTION` ni `TW_HEIGHT`).
fn hero_input(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    terrain: Option<Res<LoadedTerrain>>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    operates: Res<crate::operate::Operates>,
    mut pointer: ResMut<crate::operate::Pointer>,
    mut timer: Local<MouseTimer>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut players: Query<&mut Player>,
    mut net: ResMut<crate::net::Net>,
    gate: Res<crate::gate::GateState>,
) {
    use mu_data::terrain::attr;
    let (Some(t), Ok(mut player)) = (terrain, players.single_mut()) else { return };
    if buttons.just_pressed(MouseButton::Left) && !over_ui.0 {
        timer.push = true;
        timer.held = true;
    }
    if !buttons.pressed(MouseButton::Left) {
        timer.push = false;
        timer.held = false;
    }
    let ray = windows.single().ok().zip(cameras.single().ok()).and_then(|(w, (camera, tf))| {
        let ray = camera.viewport_to_world(tf, w.cursor_position()?).ok()?;
        Some(ray)
    });

    timer.acc += time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND;
    // Tras un parón no se recuperan más de unos pocos ticks.
    timer.acc = timer.acc.min(3.0);
    while timer.acc >= 1.0 {
        timer.acc -= 1.0;
        let mut success = false;
        if timer.time >= timer.max {
            if timer.push {
                timer.push = false;
                success = true;
            }
            if timer.held {
                success = true;
            }
        }
        // Ni sobre la interfaz (`MouseOnWindow`), ni mientras hace una habilidad, ni cruzando una
        // puerta (`LoadingWorld > 30`, ZzzInterface.cpp:7192-7195).
        if success && !over_ui.0 && player.casting.is_none() && !gate.blocks_input() {
            timer.time = 0;
            let here = (player.pos.x as usize, player.pos.y as usize);
            let moving = !player.path.is_empty();
            if let Some((i, op)) = pointer.operate.and_then(|i| operates.0.get(i).map(|o| (i, o))) {
                // `wall == TW_HEIGHT || wall < TW_CHARACTER`, y solo estando quieto.
                let (tx, ty) = op.tile();
                let wall = t.0.attribute(tx.max(0) as usize, ty.max(0) as usize);
                if (wall == attr::HEIGHT || wall < attr::CHARACTER) && !moving {
                    player.operate = Some(i);
                    match hero_path(&t.0, here, (tx as usize, ty as usize)) {
                        Some(p) => send_move(&mut player, p, &mut timer, &mut net),
                        None => operate_action(&mut player, op, &mut sound, &mut net),
                    }
                }
            } else if let Some(hit) = ray.and_then(|r| raycast_terrain(&t.0, r)) {
                let to = (hit.x.max(0.0) as usize, hit.y.max(0.0) as usize);
                let a = t.0.attribute(to.0, to.1);
                pointer.dont_move = a >= attr::NO_GROUND && a & attr::ACTION == 0 && a & attr::HEIGHT == 0;
                if let Some(p) = (here != to || !moving).then(|| hero_path(&t.0, here, to)).flatten() {
                    player.operate = None;
                    send_move(&mut player, p, &mut timer, &mut net);
                }
            }
        }
        timer.time += 1;
    }
}

/// `Action` con `MOVEMENT_OPERATE` (ZzzInterface.cpp:3587-3737): a una casilla o menos del objeto
/// se sienta, se apoya o se cura (las mujeres, con las suyas), mirando hacia donde mira el objeto si
/// toca, y lo dice al servidor (`SendRequestAction`) para que lo vean los demás. Salvo al curarse,
/// antes le dice que está en la casilla del objeto (`SendCharacterMove` de una casilla, sin pasos) y
/// suena `SOUND_DROP_ITEM01`.
fn operate_action(p: &mut Player, op: &crate::operate::Operate, sound: &mut crate::sound::SoundQueue, net: &mut crate::net::Net) {
    use crate::operate::Act;
    use mu_protocol::game::{ClientMessage, action as at};
    let (tx, ty) = op.tile();
    let (hx, hy) = (p.pos.x as i32, p.pos.y as i32);
    if (hx - tx).abs().max((hy - ty).abs()) > 1 {
        return;
    }
    if op.rule.face && op.rule.act != Act::Nothing {
        p.facing = op.angle.to_radians();
    }
    let at = match op.rule.act {
        Act::Sit => Some(at::SIT1),
        Act::Pose => Some(at::POSE1),
        Act::Healing => Some(at::HEALING1),
        Act::Nothing => None,
    };
    let dir = facing_direction(p.facing);
    if op.rule.act != Act::Healing && net.connected() {
        let (x, y) = (tx.clamp(0, 255) as u8, ty.clamp(0, 255) as u8);
        net.send(ClientMessage::Walk { source_x: x, source_y: y, rotation: dir, directions: Vec::new() });
    }
    if let Some(at) = at {
        if let Some(Some(a)) = action::for_at(at, p.class.female()) {
            p.action = a;
            p.frame = 0.0;
        }
        if net.connected() {
            net.send(ClientMessage::Animation { rotation: dir, action: at });
        }
    }
    if op.rule.act != Act::Healing {
        p.path.clear();
        sound.play_object("SOUND_DROP_ITEM01");
    }
}

/// Un gesto del chat (`SetActionClass` + `SendRequestAction` de `CheckChatText`,
/// ZzzInterface.cpp:4416-4547): la animación solo si está parado, y el aviso al servidor dos veces
/// si la pone (una en `SetActionClass` y otra después, como el original) o una si no.
pub fn chat_emote(p: &mut Player, at: u8, net: &mut crate::net::Net) {
    use mu_protocol::game::ClientMessage;
    let dir = facing_direction(p.facing);
    let connected = net.connected();
    if action::standing(p.action) && p.path.is_empty() && p.casting.is_none() {
        if let Some(Some(a)) = action::for_at(at, p.class.female()) {
            p.action = a;
            p.frame = 0.0;
        }
        if connected {
            net.send(ClientMessage::Animation { rotation: dir, action: at });
        }
    }
    if connected {
        net.send(ClientMessage::Animation { rotation: dir, action: at });
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

fn walk(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    operates: Res<crate::operate::Operates>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut net: ResMut<crate::net::Net>,
    mut q: Query<(&mut Player, &mut Transform)>,
) {
    for (mut p, mut tf) in &mut q {
        let had_path = !p.path.is_empty();
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
        // Sentado o apoyado sigue así hasta que vuelve a caminar; un gesto, hasta que acaba (`animate`).
        let action = if !p.path.is_empty() {
            p.walk
        } else if action::resting(p.action) || action::emote(p.action) {
            p.action
        } else {
            p.stop
        };
        if p.casting.is_none() && action != p.action {
            p.action = action;
            p.frame = 0.0;
        }
        // Llegó al objeto de sentarse (`MovePath` acaba → `Action`).
        if had_path && p.path.is_empty() && p.casting.is_none() {
            if let Some(op) = p.operate.take().and_then(|i| operates.0.get(i)) {
                operate_action(&mut p, op, &mut sound, &mut net);
            }
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
    args: Res<Args>,
    mut sound: ResMut<crate::sound::SoundQueue>,
) {
    *render_frame += 1;
    for (entity, mut p, s, mut actor, shadow) in &mut q {
        weapon_blur(entity, &p, s, terrain.as_deref(), &mut world, *render_frame);
        let before = p.frame;
        // `SetPlayerWalk`: el Rage Fighter camina a 0.32 en vez de 0.33.
        let fps = if p.class == crate::account::Class::Fighter && p.action == p.walk {
            0.32 * crate::fx::sim::TICKS_PER_SECOND
        } else {
            action_fps(p.action)
        };
        p.frame += p.casting.unwrap_or(fps) * time.delta_secs();
        if p.action == p.walk {
            footsteps(&p, before, s, terrain.as_deref(), args.world as i32 - 1, &mut sound);
        }
        // La habilidad termina al acabar su animación: vuelve a estar quieto. Un gesto también
        // (`PlayerStopAnimationSetting`).
        if p.casting.is_some() || action::emote(p.action) {
            let frames = s.skeleton.actions.get(p.action).map_or(1, |a| a.frames) as f32;
            if p.frame >= frames - 1.0 {
                p.casting = None;
                p.action = p.stop;
                p.frame = 0.0;
            }
        }
        let ground = terrain.as_ref().map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
        let mut sh = crate::shadow::Builder::new(
            terrain.as_deref().map(|t| &t.0),
            Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, ground),
            Quat::from_rotation_z(p.facing),
            p.class.game_scale(),
        );
        let bones = s.pose(p.action, p.frame, weapon_on_back(&p, terrain.as_deref()), &mut meshes, &mut sh);
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
        *actor = actor_state(&p, &bones, terrain.as_deref());
    }
}

/// Pasos (`PlayWalkSound`): al pasar los fotogramas 1.5 y 4.5 de la animación de caminar.
fn footsteps(p: &Player, before: f32, s: &Skinned, terrain: Option<&LoadedTerrain>, world: i32, sound: &mut crate::sound::SoundQueue) {
    let Some(act) = s.skeleton.actions.get(p.action) else { return };
    // Las animaciones de caminar repiten el primer fotograma al final.
    let n = if act.lock_positions.is_some() { act.frames.saturating_sub(1).max(1) } else { act.frames.max(1) } as f32;
    let (a, b) = (before.rem_euclid(n), p.frame.rem_euclid(n));
    let crossed = |k: f32| if b >= a { a < k && b >= k } else { a < k || b >= k };
    if !(crossed(1.5) || crossed(4.5)) {
        return;
    }
    let Some(t) = terrain else { return };
    let size = mu_data::terrain::SIZE;
    let (x, y) = ((p.pos.x.max(0.0) as usize).min(size - 1), (p.pos.y.max(0.0) as usize).min(size - 1));
    let tile = t.0.mapping.layer1[y * size + x];
    let safe = t.0.attribute(x, y) & mu_data::terrain::attr::SAFE_ZONE != 0;
    if let Some(id) = crate::sound::footstep(world, tile, safe) {
        sound.play(&id);
    }
}

/// Transformación del personaje: posición sobre el terreno y giro hacia donde mira.
pub fn player_transform(p: &Player, terrain: Option<&LoadedTerrain>) -> Transform {
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    Transform {
        translation: Vec3::new(p.pos.x, h / MU_SCALE, -p.pos.y),
        rotation: to_bevy * Quat::from_rotation_z(p.facing),
        scale: Vec3::splat(p.class.game_scale() / MU_SCALE),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::tables::ids;
    use mu_protocol::game::action as at;

    #[test]
    fn direccion_de_ida_y_vuelta() {
        // La que se manda es la que el otro gira con `(dirección - 1) * 45`.
        for d in 0..8u8 {
            assert_eq!(facing_direction(direction_facing(d)), d);
            // Hasta 22.5° de más o de menos sigue siendo la misma.
            assert_eq!(facing_direction(direction_facing(d) + 0.39), d);
            assert_eq!(facing_direction(direction_facing(d) - 0.39), d);
        }
        assert_eq!(facing_direction(0.0), 1);
    }

    #[test]
    fn acciones_recibidas() {
        let a = |at, female| action::for_at(at, female).flatten().map(|a| a as i32);
        assert_eq!(a(at::SIT1, false), Some(ids::PLAYER_SIT1));
        assert_eq!(a(at::SIT1, true), Some(ids::PLAYER_SIT_FEMALE1));
        assert_eq!(a(at::POSE1, true), Some(ids::PLAYER_POSE_FEMALE1));
        assert_eq!(a(at::HEALING1, true), Some(ids::PLAYER_HEALING_FEMALE1));
        assert_eq!(a(at::GREETING1, true), Some(ids::PLAYER_GREETING_FEMALE1));
        assert_eq!(a(at::GESTURE1, false), Some(ids::PLAYER_GESTURE1));
        assert_eq!(a(at::DIRECTION1, false), Some(ids::PLAYER_DIRECTION1));
        assert_eq!(a(at::RESPECT1, true), Some(ids::PLAYER_RESPECT1), "sin versión de mujer");
        assert_eq!(a(at::CHEERS, true), Some(ids::PLAYER_CHEERS));
        assert_eq!(action::for_at(at::STAND1, false), Some(None));
        assert_eq!(action::for_at(at::ATTACK1, false), None);
        for at in [at::SIT1, at::POSE1, at::HEALING1] {
            assert!(action::resting(a(at, false).unwrap() as usize) && action::resting(a(at, true).unwrap() as usize));
        }
        for at in at::GREETING1..=at::RUSH1 {
            assert!(action::emote(a(at, false).unwrap() as usize), "{at}");
        }
        assert!(action::standing(ids::PLAYER_STOP_FEMALE as usize) && !action::standing(ids::PLAYER_SIT1 as usize));
    }
}
