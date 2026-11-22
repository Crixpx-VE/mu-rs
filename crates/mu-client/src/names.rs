//! Lo que hay bajo el ratón (jugadores; luego NPC y monstruos), su resaltado y su nombre encima.
//!
//! **Qué se elige** (`SelectObjects`, ZzzInterface.cpp:7847-7987), cada fotograma y solo con el ratón
//! en la zona de juego (`CheckMouseIn(0, 0, ancho, 429)`) y fuera de las ventanas:
//! - sin Alt: monstruos, luego jugadores (el elfo y el mago con una habilidad de ayuda elegida, al
//!   revés), luego NPC, objetos del suelo y por último los de sentarse (`SelectOperate`);
//! - con Alt (`VK_MENU`): objetos del suelo, NPC, monstruos, jugadores y los de sentarse.
//!
//! El primero que encuentra algo corta la cadena: si hay un jugador bajo el ratón, `SelectOperate` ni
//! se llama (el banco que estuviera iluminado se queda como estaba).
//!
//! `SelectCharacter(Kind)` (7677-7813): primero pone a todos los de ese tipo `o->Light` según
//! `c->Level` (0.2; −0.4 los asesinos, `PK ≥ PVP_MURDERER2`), que se suma a la luz del terreno para
//! dibujarlos (`VectorAdd(Light, o->Light, c->Light)`, ZzzCharacter.cpp:8909 y 9651). Luego el rayo del
//! ratón contra la caja de cada uno (`o->Position + BoundingBoxMin..Max`, sin girar ni escalar:
//! (−60, −60, 0)..(40, 40, 120) el jugador, ZzzCharacter.cpp:11397-11401) y se queda con el más
//! cercano a la cámara. Nunca el propio personaje (`Main && c == Hero`). En Devias no se elige a quien
//! está en suelo 3 (dentro de las casas) si el héroe no pisa un 3 o un 11 (7773-7782; la parte de
//! Lorencia nunca se cumple).
//!
//! **El resaltado** no es más luz: el elegido se dibuja con `RenderCharacter(c, o, true)`
//! (ZzzCharacter.cpp:11079-11082) y cada pieza del cuerpo (`RenderPartObject` con `Select` = 1,
//! ZzzObject.cpp:10508-10543) pinta antes dos copias de sí misma infladas desde los huesos
//! (`BoneScale` = `m_fEdgeScale` = 1.2 y 1.18; `vp = R·v·BoneScale + T`, ZzzBMD.cpp:246-254) de color
//! plano (0.1, 0.03, 0) y (0.7, 0.2, 0) (los NPC (0.02, 0.1, 0) y (0.16, 0.7, 0)), con
//! `RENDER_BRIGHT`: sumando, sin textura, sin escribir profundidad y sin quitar caras
//! (ZzzBMD.cpp:1215-1231, `EnableAlphaBlend`), saltándose las mallas con textura TGA
//! (`Components == 4`). Después la pieza normal las tapa: queda un halo naranja alrededor de la
//! silueta. El arma no lo lleva (`RenderLinkObject` no pasa `Select`).
//!
//! **El nombre** (`CNewUINameWindow::RenderName`, NewUINameWindow.cpp:101-184): solo el del que está
//! bajo el ratón (el de un monstruo va arriba en el centro). Alt no enseña nombres de personajes (solo
//! los de los objetos del suelo). `CreateChat(c->ID, "", c)` (ZzzInterface.cpp:1106-1169) usa una de
//! las 120 cajas `CHAT`: la que ya tenía ese personaje (vida 10 ticks) o la primera libre (100); se
//! llama cada fotograma mientras siga bajo el ratón, así que al quitarlo el nombre dura 10 ticks de
//! 25 Hz (`MoveChat`, 1239-1263). `RenderBooleans` (8618-8690): la caja se centra sobre
//! `Projection(o->Position + (0, 0, BoundingBoxMax[2] + 60))`, con el borde de abajo ahí; mide
//! `FontHeight` × 5 de alto (los 5 `GetTextExtentPoint32` de `SetBooleanPosition` dan TRUE aunque el
//! texto esté vacío, 736-785) y de ancho el nombre + 3 píxeles (o el gremio, si es más largo); las
//! que se pisan se apartan arriba o abajo y luego se meten en la pantalla. `RenderBoolean` (805-1017)
//! escribe arriba la alianza, el gremio y el nombre, cada línea con su fondo. Lo que dice alguien va
//! en la misma caja, debajo del nombre (`CreateChat` con texto: [`NameBoxes::say`], que llama chat.rs
//! con `AssignChat`): una o dos líneas que duran 2 ticks por byte + 160 (el héroe también tiene la
//! suya cuando habla, con su fondo verde).
//!
//! Colores del nombre: el fondo, verde (60, 100, 0, 150) el propio, el de alianza (20, 50, 80, 150) si
//! es del mismo gremio que el héroe —sin gremio los dos, también: `-1 == -1`— y si no según la
//! relación; las letras, por el `PK` (`SetPlayerColor`, 787-799) o (100, 250, 250) en negrita los GM.
//! Con el ratón sobre la caja, en zona segura, el fondo y las letras se cambian a ratos
//! (`WorldTime % 24 < 12`).

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use bevy::ui::{UiScale, UiSystems};
use mu_data::bmd;

use crate::character::{Player, Skinned};
use crate::model_light::{ModelLight, ModelMaterial};
use crate::scene::GameScene;
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct NamesPlugin;

impl Plugin for NamesPlugin {
    fn build(&self, app: &mut App) {
        let game = || in_state(GameScene::Game);
        app.init_resource::<Hovered>()
            .init_resource::<NameBoxes>()
            .add_systems(OnEnter(GameScene::Game), spawn_root)
            .add_systems(OnExit(GameScene::Game), (crate::scene::despawn::<NamesRoot>, reset))
            .add_systems(
                Update,
                (
                    select_objects.in_set(SelectObjects).before(crate::character::HeroInput),
                    create_names.after(SelectObjects),
                    edges.after(SelectObjects).after(crate::remote::animate),
                )
                    .run_if(game()),
            )
            .add_systems(
                PostUpdate,
                layout_names.after(crate::camera::CameraMoved).before(UiSystems::Prepare).run_if(game()),
            );
    }
}

/// `SelectObjects` (para ordenar lo que lee lo que hay bajo el ratón: `SelectOperate`, el clic...).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectObjects;

/// `KIND_*` del original.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // NPC y monstruos aún no existen.
pub enum Kind {
    Player,
    Npc,
    /// `KIND_MONSTER | KIND_EDIT`.
    Monster,
}

/// Algo que se puede señalar con el ratón (`CHARACTER` de `CharactersClient`). Va en la entidad raíz
/// del personaje, junto a su `Transform` (casillas; el suelo, en la altura).
#[derive(Component, Clone, Debug)]
pub struct Selectable {
    pub kind: Kind,
    /// `c->ID`.
    pub name: String,
    /// `c->PK` (`CharacterHeroState` del paquete: 0..6, 3 = normal).
    pub pk: u8,
    /// `c->Level`: el de los jugadores es 1 con `PK ≥ PVP_MURDERER2` (WSclient.cpp:2683, 6250).
    pub level: u8,
    /// `CtlCode` `CTLCODE_20OPERATOR`/`CTLCODE_08OPERATOR` o `eBuff_GMEffect`.
    pub gm: bool,
    /// `c->Dead`: los muertos no se eligen.
    pub dead: bool,
    /// Gremio (`GuildMarkIndex` ≥ 0), con las líneas ya escritas (`AddGuildName`).
    pub guild: Option<Guild>,
    /// `o->BoundingBoxMin/Max`.
    pub bbox_min: Vec3,
    pub bbox_max: Vec3,
    /// `o->m_fEdgeScale` (1.2 de `CreateCharacterPointer`, ZzzCharacter.cpp:11312).
    pub edge_scale: f32,
    /// `o->Light`: se suma a la luz del terreno al dibujarlo. 0 hasta la primera vez que el ratón pasa
    /// por la zona de juego (`CreateCharacterPointer`, ZzzCharacter.cpp:11396).
    pub light: Vec3,
}

/// Gremio de un personaje, para su caja de nombre.
#[derive(Clone, Debug, PartialEq)]
pub struct Guild {
    /// `GuildMarkIndex`.
    pub mark: i32,
    /// `GuildRelationShip` con el gremio del héroe (`GR_*`).
    pub relation: u8,
    /// `"<alianza> cargo"` (`c->Union`), si tiene.
    pub union_line: Option<String>,
    /// `"[gremio] cargo"` (`c->Guild`).
    pub guild_line: String,
}

/// `GR_NONE`, `GR_RIVAL`, `GR_RIVALUNION`, `GR_UNION` (UIGuildInfo.h:30-37).
pub const GR_NONE: u8 = 0x00;
pub const GR_UNION: u8 = 0x01;
pub const GR_RIVAL: u8 = 0x02;
pub const GR_RIVALUNION: u8 = 0x08;

/// Caja de `MODEL_PLAYER` (ZzzCharacter.cpp:11397-11401).
pub const PLAYER_BOX_MIN: Vec3 = Vec3::new(-60.0, -60.0, 0.0);
pub const PLAYER_BOX_MAX: Vec3 = Vec3::new(40.0, 40.0, 120.0);
/// `PVP_MURDERER2`.
pub const PVP_MURDERER2: u8 = 6;

impl Selectable {
    /// Otro jugador (`ReceiveCreatePlayerViewport`) con su `PK`.
    pub fn player(name: String, pk: u8) -> Self {
        Self {
            kind: Kind::Player,
            name,
            pk,
            level: (pk >= PVP_MURDERER2) as u8,
            gm: false,
            dead: false,
            guild: None,
            bbox_min: PLAYER_BOX_MIN,
            bbox_max: PLAYER_BOX_MAX,
            edge_scale: 1.2,
            light: Vec3::ZERO,
        }
    }
}

/// `SelectedCharacter` y `SelectedNpc`.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct Hovered {
    pub character: Option<Entity>,
    pub npc: Option<Entity>,
}

impl Hovered {
    /// ¿Hay algún personaje bajo el ratón? (Entonces no se mira lo demás.)
    pub fn any(&self) -> bool {
        self.character.is_some() || self.npc.is_some()
    }
}

// --- Elegir ----------------------------------------------------------------------------------------

/// `SelectCharacter`, primer bucle: `o->Light` de todos los del tipo según `c->Level`, y −0.4 los
/// asesinos. `None`: un `Level` sin caso, la luz se queda como estaba.
pub fn select_light(level: u8, pk: u8) -> Option<Vec3> {
    if pk >= PVP_MURDERER2 {
        return Some(Vec3::splat(-0.4));
    }
    match level {
        0 => Some(Vec3::splat(0.2)),
        1 => Some(Vec3::splat(-0.4)),
        2 => Some(Vec3::new(0.2, -0.6, -0.6)),
        3 => Some(Vec3::splat(1.5)),
        4 => Some(Vec3::new(0.3, 0.2, -0.5)),
        _ => None,
    }
}

/// `bCanTalk` (ZzzInterface.cpp:7772-7782): en Devias, quien pisa suelo 3 solo se elige si el héroe
/// también pisa un 3 o un 11. (En Lorencia la condición lleva `WorldActive == WD_2DEVIAS` y no se cumple.)
pub fn can_talk(world: u16, target_tile: u8, hero_tile: u8) -> bool {
    // `WD_2DEVIAS` = carpeta 3.
    !(world == 3 && target_tile == 3 && target_tile != hero_tile && hero_tile != 11)
}

/// `TerrainMappingLayer1` en una posición de MU (`Position / TERRAIN_SCALE`).
fn layer1_at(terrain: &mu_data::terrain::Terrain, mu: Vec3) -> u8 {
    let (x, y) = ((mu.x / MU_SCALE) as i32, (mu.y / MU_SCALE) as i32);
    let index = (y * 256 + x).clamp(0, 65535) as usize;
    terrain.mapping.layer1.get(index).copied().unwrap_or(0)
}

/// Posición en MU de una entidad colocada en casillas (Bevy: x, altura, −y).
fn mu_position(tf: &Transform) -> Vec3 {
    Vec3::new(tf.translation.x, -tf.translation.z, tf.translation.y) * MU_SCALE
}

/// Una llamada a `SelectCharacter(Kind)`: luz de todos los de esos tipos y el más cercano bajo el rayo.
fn select_character(
    kinds: &[Kind],
    ray: (Vec3, Vec3),
    eye: Vec3,
    world: u16,
    terrain: Option<&mu_data::terrain::Terrain>,
    hero_tile: u8,
    q: &mut Query<(Entity, &mut Selectable, &Transform)>,
) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for (e, mut s, tf) in q.iter_mut() {
        if !kinds.contains(&s.kind) {
            continue;
        }
        if let Some(l) = select_light(s.level, s.pk)
            && s.light != l
        {
            s.light = l;
        }
        if s.dead {
            continue;
        }
        let p = mu_position(tf);
        if !crate::operate::ray_hits_box(ray.0, ray.1, p + s.bbox_min, p + s.bbox_max) {
            continue;
        }
        let d = (p - eye).length_squared();
        if best.is_some_and(|(bd, _)| d >= bd) {
            continue;
        }
        if terrain.is_some_and(|t| !can_talk(world, layer1_at(t, p), hero_tile)) {
            continue;
        }
        best = Some((d, e));
    }
    best.map(|(_, e)| e)
}

#[allow(clippy::too_many_arguments)]
fn select_objects(
    args: Res<crate::Args>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), (With<Camera3d>, With<IsDefaultUiCamera>)>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    terrain: Option<Res<LoadedTerrain>>,
    hero: Query<&Transform, With<Player>>,
    mut hovered: ResMut<Hovered>,
    mut q: Query<(Entity, &mut Selectable, &Transform)>,
) {
    // Con el ataque automático, el elegido solo se guarda si es un monstruo al que se ataca; los
    // jugadores y los NPC se vuelven a buscar cada fotograma.
    let mut now = Hovered::default();
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else {
        hovered.set_if_neq(now);
        return;
    };
    let in_area = crate::mainframe::cursor_virtual(window).is_some_and(|c| c.y < 429.0);
    if over_ui.0 || !in_area {
        hovered.set_if_neq(now);
        return;
    }
    let Some(ray) = crate::operate::mouse_ray_mu(window, camera, cam_tf) else {
        hovered.set_if_neq(now);
        return;
    };
    let t = cam_tf.translation();
    let eye = Vec3::new(t.x, -t.z, t.y) * MU_SCALE;
    let terrain = terrain.as_deref().map(|t| &t.0);
    // `HeroTile` (ZzzInterface.cpp:7640-7643).
    let hero_tile = hero.single().ok().zip(terrain).map_or(0, |(h, t)| layer1_at(t, mu_position(h)));
    let mut pick = |kinds: &[Kind]| select_character(kinds, ray, eye, args.world, terrain, hero_tile, &mut q);
    if keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
        // (Objetos del suelo: aún no hay.)
        now.npc = pick(&[Kind::Npc]);
        if now.npc.is_none() {
            now.character = pick(&[Kind::Monster]).or_else(|| pick(&[Kind::Player]));
        }
    } else {
        // `CKind_1` = monstruos, `CKind_2` = jugadores (sin habilidades de ayuda del elfo/mago aún).
        now.character = pick(&[Kind::Monster]).or_else(|| pick(&[Kind::Player]));
        if now.character.is_none() {
            now.npc = pick(&[Kind::Npc]);
        }
    }
    hovered.set_if_neq(now);
}

// --- Nombres ---------------------------------------------------------------------------------------

/// `MAX_CHAT`.
const MAX_CHAT: usize = 120;
/// `IDLifeTime` de una caja nueva y de la que ya tenía el personaje (ticks de 25 Hz).
const ID_LIFE_NEW: i32 = 100;
const ID_LIFE_AGAIN: i32 = 10;
/// Altura de la caja sobre la de elegir (`BoundingBoxMax[2] + 60`, ZzzInterface.cpp:8632).
const NAME_LIFT: f32 = 60.0;

/// Una caja `CHAT`: el nombre y, si ha hablado, lo que dijo (lo pone el chat con [`NameBoxes::say`]).
#[derive(Default)]
struct NameSlot {
    owner: Option<Entity>,
    /// `IDLifeTime`.
    id_life: i32,
    /// `Text[2]` y `LifeTime[2]` (`AddChat`, ZzzInterface.cpp:1018-1044).
    text: [String; 2],
    life: [i32; 2],
    ui: Option<SlotUi>,
}

impl NameSlot {
    /// Se dibuja (`IDLifeTime > 0 || LifeTime[0] > 0`).
    fn active(&self) -> bool {
        self.id_life > 0 || self.life[0] > 0
    }

    /// `AddChat`: `Flag` 0 dura 2 ticks por byte + 160; 1 (los `#`), 1000. Con 20 bytes o más se
    /// parte en dos líneas por el espacio más cercano al centro (`CutText`).
    fn add_chat(&mut self, text: &str, flag: u8) {
        let time = if flag == 1 { 1000 } else { text.len() as i32 * 2 + 160 };
        if text.len() >= 20 {
            let (after, before) = crate::chat_log::cut_text(text);
            self.text = [after, before];
            self.life = [time, time];
        } else {
            self.text[0] = text.to_string();
            self.life[0] = time;
        }
    }

    /// Las líneas de chat que se ven, de arriba a abajo, con su vida (`RenderBoolean`, 988-1001).
    fn chat_lines(&self) -> Vec<(&str, i32)> {
        if self.life[1] > 0 {
            vec![(self.text[1].as_str(), self.life[1]), (self.text[0].as_str(), self.life[0])]
        } else if self.life[0] > 0 {
            vec![(self.text[0].as_str(), self.life[0])]
        } else {
            Vec::new()
        }
    }
}

/// Los nodos de una caja.
struct SlotUi {
    root: Entity,
    /// Lo que hay escrito (para rehacerla solo si cambia).
    key: SlotKey,
    /// (fila, texto) de la alianza, el gremio y el nombre.
    rows: Vec<(Entity, Entity)>,
}

#[derive(Clone, PartialEq)]
struct SlotKey {
    lines: Vec<String>,
    /// Cuántas de las últimas son de chat (debajo del nombre).
    chat: usize,
    bold: bool,
    font: f32,
    line: i32,
    pad: f32,
}

/// Las 120 cajas `Chat[]` y los ticks.
#[derive(Resource)]
pub struct NameBoxes {
    slots: Vec<NameSlot>,
    /// Fracción de tick acumulada.
    acc: f32,
}

impl Default for NameBoxes {
    fn default() -> Self {
        Self { slots: (0..MAX_CHAT).map(|_| NameSlot::default()).collect(), acc: 0.0 }
    }
}

impl NameBoxes {
    /// `CreateChat(ID, "", Owner)`: la caja que ya era suya o la primera libre.
    fn create(&mut self, owner: Entity) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.owner == Some(owner)) {
            s.id_life = ID_LIFE_AGAIN;
            return;
        }
        if let Some(s) = self.slots.iter_mut().find(|s| s.id_life <= 0 && s.life[0] <= 0) {
            s.owner = Some(owner);
            s.id_life = ID_LIFE_NEW;
        }
    }

    /// `CreateChat(ID, Text, Owner, Flag)` con texto (ZzzInterface.cpp:1106-1169): en la caja que ya
    /// era suya (lo que decía sube a la segunda línea) o en la primera libre. `false` si no hay caja.
    pub fn say(&mut self, owner: Entity, text: &str, flag: u8) -> bool {
        if text.is_empty() {
            self.create(owner);
            return true;
        }
        if let Some(s) = self.slots.iter_mut().find(|s| s.owner == Some(owner)) {
            if s.life[0] > 0 {
                s.text[1] = s.text[0].clone();
                s.life[1] = s.life[0];
            }
            s.add_chat(text, flag);
            return true;
        }
        if let Some(s) = self.slots.iter_mut().find(|s| s.id_life <= 0 && s.life[0] <= 0) {
            s.owner = Some(owner);
            s.life[1] = 0;
            s.add_chat(text, flag);
            return true;
        }
        false
    }

    /// `MoveChat`: un tick menos; las de personajes que ya no están se apagan.
    fn tick(&mut self, alive: impl Fn(Entity) -> bool) {
        for s in &mut self.slots {
            if s.id_life > 0 {
                s.id_life -= 1;
            }
            for l in &mut s.life {
                if *l > 0 {
                    *l -= 1;
                }
            }
            if s.owner.is_some_and(|o| !alive(o)) {
                s.id_life = 0;
                s.life = [0, 0];
            }
        }
    }
}

/// Al salir del juego: sin cajas (sus nodos se van con `NamesRoot`).
fn reset(mut boxes: ResMut<NameBoxes>, mut hovered: ResMut<Hovered>) {
    *boxes = NameBoxes::default();
    *hovered = Hovered::default();
}

/// Mundos de Chaos Castle (`InChaosCastle`: `WD_18CHAOS_CASTLE..END` y `WD_53CAOSCASTLE_MASTER_LEVEL`).
fn in_chaos_castle(world: u16) -> bool {
    matches!(world, 19..=24 | 54)
}

/// `MoveChat` en cada tick y `RenderName` (`CreateChat` del señalado) en cada fotograma.
#[allow(clippy::too_many_arguments)]
fn create_names(
    time: Res<Time>,
    args: Res<crate::Args>,
    hero: Option<Res<crate::account::Hero>>,
    hovered: Res<Hovered>,
    selectables: Query<&Selectable>,
    hero_entity: Query<Entity, With<Player>>,
    mut boxes: ResMut<NameBoxes>,
) {
    // El héroe no lleva `Selectable`, pero sí caja cuando habla.
    let hero_e = hero_entity.single().ok();
    boxes.acc += time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND;
    while boxes.acc >= 1.0 {
        boxes.acc -= 1.0;
        boxes.tick(|e| selectables.contains(e) || hero_e == Some(e));
    }
    // En Chaos Castle no se ven los nombres (salvo los de Webzen, `FindText(Hero->ID, "webzen")`).
    if in_chaos_castle(args.world) && !hero.is_some_and(|h| h.name.contains("webzen")) && hovered.any() {
        return;
    }
    if let Some(npc) = hovered.npc {
        boxes.create(npc);
    } else if let Some(c) = hovered.character
        && selectables.get(c).is_ok_and(|s| s.kind != Kind::Monster)
    {
        // (El de un monstruo va arriba, `MonsterLabel`.)
        boxes.create(c);
    }
}

/// Medidas de la letra para una ventana: `FontHeight` (Winmain.cpp:1456-1462) y lo que sale de él en
/// unidades de la pantalla virtual (`rate` = píxeles de la ventana por unidad).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// `g_hFont`: `FontHeight − 1` píxeles.
    pub font: f32,
    /// `iLineHeight = FontHeight / g_fScreenRate_y`.
    pub line: i32,
    /// `c->Height = FontHeight · 5 / g_fScreenRate_y`.
    pub height: i32,
    /// Los 3 píxeles de más del nombre (`Size[0].cx += 3`).
    pub pad: f32,
}

impl Metrics {
    pub fn new(window_width: f32, rate: f32) -> Self {
        // La resolución más cercana de las del `switch (WindowWidth)`.
        let font_height = [(640.0, 12.0), (800.0, 13.0), (1024.0, 14.0), (1280.0, 15.0)]
            .into_iter()
            .min_by(|a, b| (a.0 - window_width).abs().total_cmp(&(b.0 - window_width).abs()))
            .map_or(14.0, |(_, h)| h);
        let rate = rate.max(1e-3);
        Self {
            font: (font_height - 1.0) / rate,
            line: (font_height / rate) as i32,
            height: (font_height * 5.0 / rate) as i32,
            pad: 3.0 / rate,
        }
    }
}

/// `x`, `y`, `Width`, `Height` de una caja en la pantalla virtual.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// `RenderBooleans`: centradas sobre el punto, se apartan las que se pisan (8654-8675) y se meten en
/// la pantalla (8682-8686). Como el original, el borde derecho y el de abajo se comparan con el tamaño
/// de la ventana en píxeles (`WindowWidth − Width`), no con el de la pantalla virtual.
pub fn place_boxes(boxes: &mut [BoxRect], window: (i32, i32)) {
    for i in 0..boxes.len() {
        for j in 0..boxes.len() {
            if i == j {
                continue;
            }
            let (ci, cj) = (boxes[i], boxes[j]);
            if ci.x + ci.w > cj.x && ci.x < cj.x + cj.w && ci.y + ci.h > cj.y && ci.y < cj.y + cj.h {
                boxes[i].y = if ci.y < cj.y + cj.h / 2 { cj.y - ci.h } else { cj.y + cj.h };
            }
        }
    }
    for b in boxes.iter_mut() {
        if b.x < 0 {
            b.x = 0;
        }
        if b.x >= window.0 - b.w {
            b.x = window.0 - b.w;
        }
        if b.y < 0 {
            b.y = 0;
        }
        if b.y >= window.1 - b.h {
            b.y = window.1 - b.h;
        }
    }
}

/// Color de la caja como `RGBA` (0..255).
pub type Rgba = [u8; 4];

/// `SetPlayerColor` (ZzzInterface.cpp:787-799) con `c->Color` = `PK` (0 los NPC, `CreateChat`).
pub fn player_color(color: u8) -> Rgba {
    match color {
        0 => [150, 255, 240, 255],
        1 => [100, 120, 255, 255],
        2 => [140, 180, 255, 255],
        3 => [200, 220, 255, 255],
        4 => [255, 150, 60, 255],
        5 => [255, 80, 30, 255],
        _ => [255, 0, 0, 255],
    }
}

/// `GetGuildRelationShipTextColor` y `GetGuildRelationShipBGColor` (ZzzCharacter.cpp:12009-12035).
pub fn relation_colors(relation: u8) -> (Rgba, Rgba) {
    match relation {
        GR_NONE => ([10, 30, 50, 150], [230, 230, 255, 255]),
        GR_RIVAL | GR_RIVALUNION => ([0, 0, 0, 150], [255, 30, 0, 255]),
        _ => ([20, 50, 80, 150], [200, 255, 0, 255]),
    }
}

/// Colores de una caja de nombre: (fondo, letras) de las líneas del gremio y del nombre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxStyle {
    pub guild: (Rgba, Rgba),
    pub name: (Rgba, Rgba),
    pub bold: bool,
}

/// `RenderBoolean` (ZzzInterface.cpp:893-959). `same_guild`: `GuildMarkIndex == Hero->GuildMarkIndex`
/// (también sin gremio los dos). `swap`: el ratón está sobre la caja y toca cambiar (938-946).
pub fn box_style(kind: Kind, is_hero: bool, gm: bool, pk: u8, same_guild: bool, relation: u8, swap: bool) -> BoxStyle {
    // El fondo gris de los GM (898) lo pisa siempre lo de abajo.
    let guild = if is_hero {
        ([60, 100, 0, 150], [200, 255, 0, 255])
    } else if same_guild {
        relation_colors(GR_UNION)
    } else {
        relation_colors(relation)
    };
    let color = if kind == Kind::Npc { 0 } else { pk };
    let text = if gm { [100, 250, 250, 255] } else { player_color(color) };
    let name = if swap { (text, guild.0) } else { (guild.0, text) };
    BoxStyle { guild, name, bold: gm }
}

/// (fondo, letras) de una línea de chat de la caja (`RenderBoolean`, 961-986): el fondo por
/// `GuildColor` (sin gremios, 0: (10, 30, 50, 150)) y las letras (230, 220, 200), o (250, 200, 50)
/// los GM, a media transparencia los 10 últimos ticks.
pub fn chat_colors(gm: bool, life: i32) -> (Rgba, Rgba) {
    let a = if life > 0 && life < 10 { 128 } else { 255 };
    let text = if gm { [250, 200, 50, a] } else { [230, 220, 200, a] };
    ([10, 30, 50, 150], text)
}

fn rgba(c: Rgba) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// Raíz de las cajas de nombre: `CNewUINameWindow` es la capa más baja (`GetLayerDepth` 1).
#[derive(Component)]
struct NamesRoot;

/// El nombre de un monstruo, arriba en el centro, y su texto.
#[derive(Component)]
struct MonsterLabel;
#[derive(Component)]
struct MonsterLabelText;

fn spawn_root(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Nombres"),
            NamesRoot,
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            GlobalZIndex(-2),
        ))
        .with_children(|r| {
            // `RenderText(320, 2, c->ID, 0, 0, RT3_WRITE_CENTER)` con (255, 230, 200) sobre (100, 0, 0).
            r.spawn((
                MonsterLabel,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(2.0),
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Visibility::Hidden,
            ))
            .with_children(|l| {
                l.spawn((Node::default(), BackgroundColor(Color::srgb_u8(100, 0, 0)))).with_children(|b| {
                    b.spawn((
                        MonsterLabelText,
                        Text::new(""),
                        name_font(Metrics::new(1024.0, 1.6).font, false),
                        TextColor(Color::srgb_u8(255, 230, 200)),
                        TextLayout::no_wrap(),
                    ));
                });
            });
        });
}

fn name_font(px: f32, bold: bool) -> TextFont {
    TextFont {
        font_size: FontSize::Px(px),
        // `FW_BOLD` (`g_hFontBold`); la Noto Sans de `fonts.rs` no es variable, así que no cambia.
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        font_smoothing: FontSmoothing::AntiAliased,
        ..default()
    }
}

/// Coloca las cajas y les pone colores (`RenderBooleans` → `RenderBoolean`).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn layout_names(
    mut commands: Commands,
    (windows, scale, cameras): (Query<&Window>, Res<UiScale>, Query<(&Camera, &Transform), (With<Camera3d>, With<IsDefaultUiCamera>)>),
    (args, hero_name, terrain, time): (Res<crate::Args>, Option<Res<crate::account::Hero>>, Option<Res<LoadedTerrain>>, Res<Time<Real>>),
    (hovered, hero_info): (Res<Hovered>, Option<Res<crate::account::HeroInfo>>),
    owners: Query<(&Selectable, &Transform), Without<Camera3d>>,
    hero: Query<(Entity, &Transform), (With<Player>, Without<Camera3d>)>,
    root: Query<Entity, With<NamesRoot>>,
    computed: Query<&ComputedNode>,
    mut nodes: Query<(&mut Node, &mut Visibility), Without<MonsterLabel>>,
    mut label: Query<&mut Visibility, With<MonsterLabel>>,
    mut label_text: Query<(&mut Text, &mut TextFont), With<MonsterLabelText>>,
    mut colors: Query<&mut TextColor>,
    mut backgrounds: Query<&mut BackgroundColor>,
    mut boxes: ResMut<NameBoxes>,
) {
    let (Ok(window), Ok(root)) = (windows.single(), root.single()) else { return };
    let rate = scale.0;
    let metrics = Metrics::new(window.width(), rate);
    // La cámara se mueve en `CameraMoved` (PostUpdate) y no tiene padre: su `Transform` es el de ahora.
    let camera = cameras.single().ok().map(|(c, tf)| (c, GlobalTransform::from(*tf)));
    let hide_all = in_chaos_castle(args.world) && !hero_name.as_ref().is_some_and(|h| h.name.contains("webzen"));

    // El nombre del monstruo señalado (en Chaos Castle tampoco, como los demás).
    if let (Ok(mut vis), Ok((mut text, mut font))) = (label.single_mut(), label_text.single_mut()) {
        let monster = hovered
            .character
            .and_then(|e| owners.get(e).ok())
            .filter(|(s, _)| s.kind == Kind::Monster && !hide_all);
        match monster {
            Some((s, _)) => {
                if text.0 != s.name {
                    text.0 = s.name.clone();
                }
                let f = name_font(metrics.font, false);
                if *font != f {
                    *font = f;
                }
                vis.set_if_neq(Visibility::Inherited);
            }
            None => {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
    }

    // Cursor (`MouseX`, `MouseY`) en la pantalla virtual.
    let mouse = window.cursor_position().map(|c| ((c.x / rate) as i32, (c.y / rate) as i32));
    // El héroe como dueño de su caja de chat (`CharactersClient` lo lleva como un jugador más), con el
    // `PK` de `CharacterInformation`.
    let hero_owner = hero.single().ok().map(|(e, tf)| {
        let name = hero_name.as_ref().map_or_else(crate::account::current_account, |h| h.name.clone());
        (e, Selectable::player(name, hero_info.as_ref().map_or(0, |i| i.0.hero_state)), *tf)
    });
    let owner_of = |o: Entity| -> Option<(Selectable, Transform, bool)> {
        match &hero_owner {
            Some((e, s, tf)) if *e == o => Some((s.clone(), *tf, true)),
            _ => owners.get(o).ok().map(|(s, tf)| (s.clone(), *tf, false)),
        }
    };
    let hero_safe = terrain.as_deref().zip(hero.single().ok().map(|(_, tf)| tf)).is_some_and(|(t, h)| {
        let p = mu_position(h) / MU_SCALE;
        t.0.attribute((p.x.max(0.0) as usize).min(255), (p.y.max(0.0) as usize).min(255)) & mu_data::terrain::attr::SAFE_ZONE != 0
    });
    let world_time = (time.elapsed_secs_f64() * 1000.0) as u64;
    let window_px = (window.width() as i32, window.height() as i32);

    // 1) Contenido y medidas de cada caja encendida.
    let mut active: Vec<(usize, BoxRect)> = Vec::new();
    for (i, slot) in boxes.slots.iter_mut().enumerate() {
        let owner = slot.owner.filter(|_| slot.active()).and_then(owner_of);
        let Some((s, tf, _)) = owner else {
            if let Some(ui) = &slot.ui
                && let Ok((_, mut vis)) = nodes.get_mut(ui.root)
            {
                vis.set_if_neq(Visibility::Hidden);
            }
            continue;
        };
        let mut lines = Vec::new();
        if let Some(g) = &s.guild {
            lines.extend(g.union_line.clone());
            lines.push(g.guild_line.clone());
        }
        lines.push(s.name.clone());
        let chat = slot.chat_lines();
        lines.extend(chat.iter().map(|(t, _)| t.to_string()));
        let key = SlotKey { lines, chat: chat.len(), bold: s.gm, font: metrics.font, line: metrics.line, pad: metrics.pad };
        if slot.ui.as_ref().is_none_or(|ui| ui.key != key) {
            if let Some(old) = slot.ui.take() {
                commands.entity(old.root).despawn();
            }
            slot.ui = Some(spawn_box(&mut commands, root, key));
            continue;
        }
        let Some(ui) = slot.ui.as_ref() else { continue };
        // El ancho (`c->Width`) es el que midió la maquetación en el fotograma anterior (0 en una
        // caja recién hecha: se enseña al siguiente).
        let width = computed.get(ui.root).map_or(0.0, |c| c.size().x * c.inverse_scale_factor());
        // `Projection` del punto sobre la cabeza (detrás de la cámara no hay).
        let p = mu_position(&tf) + Vec3::Z * (s.bbox_max.z + NAME_LIFT);
        let px = camera.and_then(|(c, gt)| c.world_to_viewport(&gt, Vec3::new(p.x, p.z, -p.y) / MU_SCALE).ok());
        let Some(px) = px.filter(|_| width > 0.0) else {
            if let Ok((_, mut vis)) = nodes.get_mut(ui.root) {
                vis.set_if_neq(Visibility::Hidden);
            }
            continue;
        };
        let (sx, sy) = ((px.x / rate) as i32, (px.y / rate) as i32);
        let (w, h) = (width as i32, metrics.height);
        active.push((i, BoxRect { x: sx - w / 2, y: sy - h, w, h }));
    }

    // 2) Que no se pisen y dentro de la pantalla.
    let mut rects: Vec<BoxRect> = active.iter().map(|(_, r)| *r).collect();
    place_boxes(&mut rects, window_px);

    // 3) Colores y posición.
    for ((i, _), r) in active.iter().zip(&rects) {
        let slot = &boxes.slots[*i];
        let (Some(ui), Some((s, _, is_hero))) = (slot.ui.as_ref(), slot.owner.and_then(owner_of)) else { continue };
        // Con el ratón encima, en zona segura y no siendo el propio: fondo y letras cambiados a ratos.
        // (El ancho y el alto de la prueba se vuelven a escalar, como en el original.)
        let over = mouse.is_some_and(|(mx, my)| {
            r.x <= mx && mx < r.x + r.w * 640 / window_px.0.max(1) && r.y <= my && my < r.y + r.h * 480 / window_px.1.max(1)
        });
        // (Sobre la propia caja no se cambian: `strcmp(c->ID, Hero->ID) != 0`.)
        let swap = over && hero_safe && world_time % 24 < 12 && !is_hero;
        // El héroe no tiene `Selectable` (su nombre sale con su chat); sin gremio, `-1 == -1`.
        let hero_guild: Option<i32> = None;
        let mark = s.guild.as_ref().map(|g| g.mark);
        let relation = s.guild.as_ref().map_or(GR_NONE, |g| g.relation);
        let style = box_style(s.kind, is_hero, s.gm, s.pk, mark == hero_guild, relation, swap);
        let chat_lines = slot.chat_lines();
        let name_row = ui.rows.len() - 1 - ui.key.chat;
        for (k, &(row, text)) in ui.rows.iter().enumerate() {
            let (bg, fg) = match k.cmp(&name_row) {
                std::cmp::Ordering::Less => style.guild,
                std::cmp::Ordering::Equal => style.name,
                std::cmp::Ordering::Greater => {
                    let life = chat_lines.get(k - name_row - 1).map_or(0, |(_, l)| *l);
                    chat_colors(s.gm, life)
                }
            };
            if let Ok(mut b) = backgrounds.get_mut(row) {
                b.set_if_neq(BackgroundColor(rgba(bg)));
            }
            if let Ok(mut c) = colors.get_mut(text) {
                c.set_if_neq(TextColor(rgba(fg)));
            }
        }
        if let Ok((mut node, mut vis)) = nodes.get_mut(ui.root) {
            let (l, t) = (Val::Px(r.x as f32), Val::Px(r.y as f32));
            if node.left != l || node.top != t {
                node.left = l;
                node.top = t;
            }
            vis.set_if_neq(Visibility::Inherited);
        }
    }
}

/// Una caja: columna de filas (alianza, gremio, nombre), cada una con su fondo y tan ancha como la más
/// ancha (`RenderText(..., c->Width, iLineHeight, RT3_SORT_LEFT)`). Empieza oculta hasta medirla.
fn spawn_box(commands: &mut Commands, root: Entity, key: SlotKey) -> SlotUi {
    let mut rows = Vec::new();
    // El nombre va encima de las líneas de chat.
    let n = key.lines.len() - key.chat;
    let ui = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();
    for (k, line) in key.lines.iter().enumerate() {
        // Solo el nombre lleva los 3 píxeles de más; la negrita, solo el nombre de los GM.
        let is_name = k + 1 == n;
        let text = commands
            .spawn((
                Text::new(line.clone()),
                name_font(key.font, is_name && key.bold),
                LineHeight::Px(key.font),
                TextColor(Color::WHITE),
                TextLayout::no_wrap(),
            ))
            .id();
        let row = commands
            .spawn((
                Node {
                    height: Val::Px(key.line as f32),
                    padding: UiRect::right(Val::Px(if is_name { key.pad } else { 0.0 })),
                    overflow: Overflow::visible(),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .add_child(text)
            .id();
        commands.entity(ui).add_child(row);
        rows.push((row, text));
    }
    commands.entity(root).add_child(ui);
    SlotUi { root: ui, key, rows }
}

// --- Resaltado (`RenderPartObjectEdge`) ------------------------------------------------------------

/// Una cáscara de una pieza del cuerpo.
struct Shell {
    entity: Entity,
    mesh: Handle<Mesh>,
    /// Índice en `Skinned::body_meshes`.
    part: usize,
    /// 0 = la de fuera (`m_fEdgeScale`), 1 = la de dentro (−0.02).
    pass: usize,
}

/// Las cáscaras de un personaje (se crean la primera vez que se señala y luego se ocultan).
#[derive(Component)]
struct EdgeShells(Vec<Shell>);

/// Marca de las entidades de cáscara.
#[derive(Component)]
struct EdgeShell;

/// `BodyLight` de las dos pasadas (ZzzObject.cpp:10512-10541).
pub fn edge_colors(kind: Kind, chaos_castle: bool) -> [[f32; 3]; 2] {
    if chaos_castle {
        [[0.1, 0.01, 0.0], [0.7, 0.07, 0.0]]
    } else if kind == Kind::Npc {
        [[0.02, 0.1, 0.0], [0.16, 0.7, 0.0]]
    } else {
        [[0.1, 0.03, 0.0], [0.7, 0.2, 0.0]]
    }
}

/// `BoneScale` de las dos pasadas: `m_fEdgeScale` y −0.02; en Chaos Castle `1 + 0.1/Scale` y
/// `1 + 0.04/Scale` (ZzzObject.cpp:10510-10542).
pub fn edge_scales(edge_scale: f32, object_scale: f32, chaos_castle: bool) -> [f32; 2] {
    if chaos_castle {
        // (La segunda es `1 + 0.04/Scale + 0.02`, y se dibuja con `Scale − 0.02`.)
        let s = object_scale.max(1e-3);
        [1.0 + 0.1 / s, 1.0 + 0.04 / s]
    } else {
        [edge_scale, edge_scale - 0.02]
    }
}

/// Huesos con `BoneScale`: `vp = R·v·s + T` es como girar con `R·s`.
pub fn scale_bones(bones: &[bmd::Mat34], s: f32) -> Vec<bmd::Mat34> {
    bones
        .iter()
        .map(|m| {
            let mut m = *m;
            for row in &mut m {
                for v in row.iter_mut().take(3) {
                    *v *= s;
                }
            }
            m
        })
        .collect()
}

/// `pBitmap->Components == 4`: las texturas TGA (`OpenTga`, GlobalBitmap.cpp:716) no llevan cáscara.
pub fn edge_skips_texture(texture: &str) -> bool {
    texture.to_ascii_lowercase().ends_with(".tga")
}

/// Material de una pasada: color plano sumando, sin escribir profundidad.
///
/// El original pinta la cáscara antes que la pieza y la pieza (opaca) la tapa; aquí lo opaco va
/// siempre antes que lo que suma, así que se dibujan solo las caras de atrás de la cáscara (quedan
/// detrás del cuerpo y no lo tiñen) con el doble de color, que es lo que suman delante y detrás
/// (el original no quita caras) en el halo.
fn edge_material(color: [f32; 3]) -> ModelMaterial {
    let base = StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        cull_mode: Some(OUTWARD_FACE),
        alpha_mode: AlphaMode::Add,
        ..default()
    };
    crate::model_light::model_material(base, ModelLight::flat(color.map(|c| c * 2.0)))
}

/// Las caras de fuera de las mallas de MU (las que se quitan para ver solo las de atrás).
const OUTWARD_FACE: Face = Face::Front;

/// Materiales de las cáscaras por (tipo, Chaos Castle) y pasada.
#[derive(Default)]
struct EdgeMaterials(Vec<((Kind, bool), EdgePair)>);

/// Los materiales de las dos pasadas.
type EdgePair = [Handle<ModelMaterial>; 2];

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn edges(
    mut commands: Commands,
    args: Res<crate::Args>,
    hovered: Res<Hovered>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut cache: Local<EdgeMaterials>,
    q: Query<(Entity, &Selectable, &Skinned, &Transform, Option<&crate::remote::RemotePlayer>, Option<&EdgeShells>)>,
    mut vis: Query<&mut Visibility, With<EdgeShell>>,
) {
    let chaos = in_chaos_castle(args.world);
    for (e, s, skinned, tf, remote, shells) in &q {
        let selected = hovered.character == Some(e) || hovered.npc == Some(e);
        let Some(shells) = shells else {
            if selected {
                let mats = edge_materials(&mut cache, &mut materials, s.kind, chaos);
                let list = spawn_shells(&mut commands, e, skinned, &mut meshes, mats);
                commands.entity(e).insert(EdgeShells(list));
            }
            continue;
        };
        for sh in &shells.0 {
            if let Ok(mut v) = vis.get_mut(sh.entity) {
                v.set_if_neq(if selected { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        if !selected {
            continue;
        }
        let Some((action, frame)) = remote.map(|r| r.pose_state()) else { continue };
        let bones = skinned.bones_at(action, frame);
        let scales = edge_scales(s.edge_scale, tf.scale.x * MU_SCALE, chaos);
        let scaled = [scale_bones(&bones, scales[0]), scale_bones(&bones, scales[1])];
        let parts: Vec<_> = skinned.body_meshes().collect();
        for sh in &shells.0 {
            let Some(&(model, mesh)) = parts.get(sh.part) else { continue };
            let posed = model.pose_with(mesh, &scaled[sh.pass]);
            if let Some(mut m) = meshes.get_mut(&sh.mesh) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
            }
        }
    }
}

fn edge_materials(
    cache: &mut EdgeMaterials,
    materials: &mut Assets<ModelMaterial>,
    kind: Kind,
    chaos: bool,
) -> [Handle<ModelMaterial>; 2] {
    if let Some((_, m)) = cache.0.iter().find(|(k, _)| *k == (kind, chaos)) {
        return m.clone();
    }
    let [a, b] = edge_colors(kind, chaos);
    let m = [materials.add(edge_material(a)), materials.add(edge_material(b))];
    cache.0.push(((kind, chaos), m.clone()));
    m
}

/// Dos cáscaras (ocultas) por cada pieza del cuerpo que la lleva, hijas del personaje.
fn spawn_shells(
    commands: &mut Commands,
    owner: Entity,
    skinned: &Skinned,
    meshes: &mut Assets<Mesh>,
    mats: [Handle<ModelMaterial>; 2],
) -> Vec<Shell> {
    let mut out = Vec::new();
    for (part, (model, mesh)) in skinned.body_meshes().enumerate() {
        if edge_skips_texture(&model.meshes[mesh].texture) {
            continue;
        }
        let posed = model.pose_with(mesh, &[]);
        if posed.positions.is_empty() {
            continue;
        }
        for (pass, mat) in mats.iter().enumerate() {
            let handle = meshes.add(
                Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions.clone())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals.clone())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs.clone()),
            );
            let entity = commands
                .spawn((
                    Name::new("Resaltado"),
                    EdgeShell,
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    NoFrustumCulling,
                    ChildOf(owner),
                ))
                .id();
            out.push(Shell { entity, mesh: handle, part, pass });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colores_por_pk() {
        assert_eq!(player_color(3), [200, 220, 255, 255]);
        assert_eq!(player_color(0), [150, 255, 240, 255]);
        assert_eq!(player_color(6), [255, 0, 0, 255]);
        assert_eq!(player_color(9), [255, 0, 0, 255]);
        // Un jugador normal sin gremio (como el héroe sin gremio): fondo de alianza.
        let s = box_style(Kind::Player, false, false, 3, true, GR_NONE, false);
        assert_eq!(s.name, ([20, 50, 80, 150], [200, 220, 255, 255]));
        // El propio: fondo verde, letras por su PK.
        let s = box_style(Kind::Player, true, false, 4, true, GR_NONE, false);
        assert_eq!(s.name, ([60, 100, 0, 150], [255, 150, 60, 255]));
        // GM: letras cian en negrita; el fondo gris lo pisa el de gremio.
        let s = box_style(Kind::Player, false, true, 3, false, GR_RIVAL, false);
        assert_eq!(s.name, ([0, 0, 0, 150], [100, 250, 250, 255]));
        assert!(s.bold);
        // NPC: color 0 aunque traiga otro PK; con el cambio, al revés.
        let s = box_style(Kind::Npc, false, false, 3, false, GR_NONE, true);
        assert_eq!(s.name, ([150, 255, 240, 255], [10, 30, 50, 150]));
    }

    #[test]
    fn luz_al_elegir() {
        assert_eq!(select_light(0, 3), Some(Vec3::splat(0.2)));
        assert_eq!(select_light(1, 6), Some(Vec3::splat(-0.4)));
        assert_eq!(select_light(0, 7), Some(Vec3::splat(-0.4)));
        assert_eq!(select_light(3, 3), Some(Vec3::splat(1.5)));
        assert_eq!(select_light(9, 3), None);
        assert_eq!(Selectable::player("a".into(), 6).level, 1);
    }

    #[test]
    fn medidas_a_1024() {
        let m = Metrics::new(1024.0, 768.0 / 480.0);
        assert_eq!(m.line, 8);
        assert_eq!(m.height, 43);
        assert!((m.font - 13.0 / 1.6).abs() < 1e-4);
        assert_eq!(Metrics::new(800.0, 600.0 / 480.0).height, 52);
        assert_eq!(Metrics::new(1280.0, 1024.0 / 480.0).line, 7);
    }

    #[test]
    fn cajas_se_apartan_y_entran() {
        // Dos cajas en el mismo sitio: la primera sube por encima de la segunda.
        let mut b = [BoxRect { x: 100, y: 100, w: 40, h: 43 }, BoxRect { x: 110, y: 100, w: 40, h: 43 }];
        place_boxes(&mut b, (1024, 768));
        assert_eq!(b[0].y, 100 - 43);
        assert_eq!(b[1].y, 100);
        // Fuera por arriba y por la izquierda: se meten; por la derecha solo contra el ancho en píxeles.
        let mut b = [BoxRect { x: -5, y: -10, w: 40, h: 43 }, BoxRect { x: 700, y: 50, w: 40, h: 43 }];
        place_boxes(&mut b, (1024, 768));
        assert_eq!((b[0].x, b[0].y), (0, 0));
        assert_eq!(b[1].x, 700);
    }

    #[test]
    fn cajas_de_chat() {
        let mut boxes = NameBoxes::default();
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        boxes.create(a);
        assert_eq!(boxes.slots[0].id_life, ID_LIFE_NEW);
        boxes.create(a);
        assert_eq!(boxes.slots[0].id_life, ID_LIFE_AGAIN);
        for _ in 0..ID_LIFE_AGAIN {
            boxes.tick(|_| true);
        }
        assert_eq!(boxes.slots[0].id_life, 0);
        // La caja libre se la queda otro; el primero vuelve a empezar con 100 en la siguiente.
        boxes.create(b);
        assert_eq!(boxes.slots[0].owner, Some(b));
        boxes.create(a);
        assert_eq!((boxes.slots[1].owner, boxes.slots[1].id_life), (Some(a), ID_LIFE_NEW));
        // Si el dueño desaparece, se apaga.
        boxes.tick(|e| e != a);
        assert_eq!(boxes.slots[1].id_life, 0);
    }

    #[test]
    fn lo_que_dicen() {
        let mut boxes = NameBoxes::default();
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        assert!(boxes.say(a, "hola", 0));
        assert_eq!(boxes.slots[0].chat_lines(), [("hola", 4 * 2 + 160)]);
        // Lo nuevo empuja lo de antes a la línea de arriba.
        boxes.tick(|_| true);
        boxes.say(a, "adios", 0);
        assert_eq!(boxes.slots[0].chat_lines(), [("hola", 167), ("adios", 170)]);
        // Largo: partido por el espacio más cercano al centro, las dos con la misma vida.
        boxes.say(b, "hola que tal estas hoy", 0);
        assert_eq!(boxes.slots[1].chat_lines(), [("hola que tal ", 204), ("estas hoy", 204)]);
        // Mientras habla, su caja no se la queda otro, aunque no tenga el nombre a la vista.
        boxes.create(b);
        assert_eq!(boxes.slots[1].id_life, ID_LIFE_AGAIN);
        let c = world.spawn_empty().id();
        boxes.create(c);
        assert_eq!(boxes.slots[2].owner, Some(c));
        // `#`: 1000 ticks.
        boxes.say(a, "aviso", 1);
        assert_eq!(boxes.slots[0].life[0], 1000);
        // Los colores de las líneas: medio transparentes al final.
        assert_eq!(chat_colors(false, 100).1, [230, 220, 200, 255]);
        assert_eq!(chat_colors(false, 5).1[3], 128);
        for _ in 0..1000 {
            boxes.tick(|_| true);
        }
        assert!(!boxes.slots[0].active());
    }

    #[test]
    fn devias_casas() {
        assert!(can_talk(1, 4, 0));
        assert!(!can_talk(3, 3, 0));
        assert!(can_talk(3, 3, 3));
        assert!(can_talk(3, 3, 11));
        assert!(can_talk(3, 2, 0));
    }

    #[test]
    fn cascaras() {
        let s = edge_scales(1.2, 0.9, false);
        assert!((s[0] - 1.2).abs() < 1e-6 && (s[1] - 1.18).abs() < 1e-6);
        let cc = edge_scales(1.2, 0.9, true);
        assert!((cc[0] - (1.0 + 0.1 / 0.9)).abs() < 1e-6 && (cc[1] - (1.0 + 0.04 / 0.9)).abs() < 1e-6);
        assert_eq!(edge_colors(Kind::Player, false)[1], [0.7, 0.2, 0.0]);
        assert_eq!(edge_colors(Kind::Npc, false)[0], [0.02, 0.1, 0.0]);
        assert!(edge_skips_texture("hair_r.TGA") && !edge_skips_texture("skin_01.jpg"));
        // `R·v·s + T`: el desplazamiento del hueso no se escala.
        let mut m: bmd::Mat34 = [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 6.0], [0.0, 0.0, 1.0, 7.0]];
        m = scale_bones(&[m], 1.2)[0];
        assert_eq!(bmd::transform_point(&m, [10.0, 0.0, 0.0]), [17.0, 6.0, 7.0]);
    }

    /// ¿Qué caras de las piezas de MU miran hacia fuera? (Para `OUTWARD_FACE`.) Con la regla de la
    /// mano derecha, la normal de cada triángulo se compara con las normales de sus vértices.
    #[test]
    fn caras_de_fuera() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data/Player");
        let (Ok(skel), Ok(armor)) = (mu_data::fs::read(&data, "player.bmd"), mu_data::fs::read(&data, "ArmorClass02.bmd"))
        else {
            return;
        };
        let bones = bmd::read(&skel).unwrap().bone_matrices(1, 0);
        let armor = bmd::read(&armor).unwrap();
        let (mut agree, mut total) = (0, 0);
        for mesh in 0..armor.meshes.len() {
            let p = armor.pose_with(mesh, &bones);
            for t in 0..p.positions.len() / 3 {
                let v = |k: usize| Vec3::from_array(p.positions[t * 3 + k]);
                let n = (v(1) - v(0)).cross(v(2) - v(0));
                let vn: Vec3 = (0..3).map(|k| Vec3::from_array(p.normals[t * 3 + k])).sum();
                if n.length() > 1e-6 && vn.length() > 1e-6 {
                    total += 1;
                    agree += (n.dot(vn) > 0.0) as usize;
                }
            }
        }
        eprintln!("triángulos con la normal de su giro hacia fuera: {agree} de {total}");
        // Antihorario (regla de la mano derecha) hacia fuera = cara delantera de wgpu.
        let ccw_outward = agree * 2 > total;
        assert_eq!(ccw_outward, OUTWARD_FACE == Face::Front);
    }
}
