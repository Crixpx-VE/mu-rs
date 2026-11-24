//! What is under the mouse (players; later NPCs and monsters), its highlight and its name above it.
//!
//! **What gets picked** (`SelectObjects`, ZzzInterface.cpp:7847-7987), every frame and only with the mouse
//! in the play area (`CheckMouseIn(0, 0, width, 429)`) and outside the windows:
//! - without Alt: monsters, then players (reversed for the elf and the wizard with a support skill
//!   selected), then NPCs, ground items and finally the sittable objects (`SelectOperate`);
//! - with Alt (`VK_MENU`): ground items, NPCs, monsters, players and the sittable objects.
//!
//! The first one that finds something breaks the chain: if a player is under the mouse, `SelectOperate`
//! is not even called (a bench that was lit stays as it was).
//!
//! `SelectCharacter(Kind)` (7677-7813): first sets `o->Light` for everyone of that kind according to
//! `c->Level` (0.2; −0.4 for murderers, `PK ≥ PVP_MURDERER2`), which is added to the terrain light when
//! drawing them (`VectorAdd(Light, o->Light, c->Light)`, ZzzCharacter.cpp:8909 and 9651). Then the mouse
//! ray against each one's box (`o->Position + BoundingBoxMin..Max`, not rotated or scaled:
//! (−60, −60, 0)..(40, 40, 120) for the player, ZzzCharacter.cpp:11397-11401), keeping the one
//! closest to the camera. Never the own character (`Main && c == Hero`). In Devias, someone on
//! floor 3 (inside the houses) is not picked unless the hero stands on a 3 or an 11 (7773-7782; the
//! Lorencia part never holds).
//!
//! **The highlight** is not extra light: the picked one is drawn with `RenderCharacter(c, o, true)`
//! (ZzzCharacter.cpp:11079-11082) and each body part (`RenderPartObject` with `Select` = 1,
//! ZzzObject.cpp:10508-10543) first paints two copies of itself inflated from the bones
//! (`BoneScale` = `m_fEdgeScale` = 1.2 and 1.18; `vp = R·v·BoneScale + T`, ZzzBMD.cpp:246-254) in flat
//! color (0.1, 0.03, 0) and (0.7, 0.2, 0) (NPCs (0.02, 0.1, 0) and (0.16, 0.7, 0)), with
//! `RENDER_BRIGHT`: additive, untextured, no depth write and no face culling
//! (ZzzBMD.cpp:1215-1231, `EnableAlphaBlend`), skipping meshes with a TGA texture
//! (`Components == 4`). Then the normal part covers them: an orange halo remains around the
//! silhouette. The weapon does not get it (`RenderLinkObject` does not pass `Select`).
//!
//! **The name** (`CNewUINameWindow::RenderName`, NewUINameWindow.cpp:101-184): only the one under
//! the mouse (a monster's goes at the top center). Alt shows no character names (only
//! ground items'). `CreateChat(c->ID, "", c)` (ZzzInterface.cpp:1106-1169) uses one of
//! the 120 `CHAT` boxes: the one that character already had (life 10 ticks) or the first free one (100); it
//! is called every frame while it stays under the mouse, so after moving away the name lasts 10
//! 25 Hz ticks (`MoveChat`, 1239-1263). `RenderBooleans` (8618-8690): the box is centered over
//! `Projection(o->Position + (0, 0, BoundingBoxMax[2] + 60))`, with its bottom edge there; it is
//! `FontHeight` × 5 tall (the 5 `GetTextExtentPoint32` calls of `SetBooleanPosition` return TRUE even when the
//! text is empty, 736-785) and as wide as the name + 3 pixels (or the guild, if longer);
//! overlapping ones move up or down and are then pushed inside the screen. `RenderBoolean` (805-1017)
//! writes the alliance, guild and name on top, each line with its background. What someone says goes
//! in the same box, below the name (`CreateChat` with text: [`NameBoxes::say`], which chat.rs calls
//! with `AssignChat`): one or two lines lasting 2 ticks per byte + 160 (the hero also gets
//! one when speaking, with its green background).
//!
//! Name colors: the background is green (60, 100, 0, 150) for the own one, alliance (20, 50, 80, 150) if
//! in the same guild as the hero (also when neither has a guild: `-1 == -1`), otherwise by the
//! relation; the letters by `PK` (`SetPlayerColor`, 787-799) or bold (100, 250, 250) for GMs.
//! With the mouse over the box, in a safe zone, background and letters alternate periodically
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

/// `SelectObjects` (to order what reads what is under the mouse: `SelectOperate`, the click...).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectObjects;

/// The original's `KIND_*`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // NPCs and monsters do not exist yet.
pub enum Kind {
    Player,
    Npc,
    /// `KIND_MONSTER | KIND_EDIT`.
    Monster,
}

/// Something the mouse can point at (`CHARACTER` of `CharactersClient`). Lives on the character's root
/// entity, next to its `Transform` (tiles; the ground, in the height).
#[derive(Component, Clone, Debug)]
pub struct Selectable {
    pub kind: Kind,
    /// `c->ID`.
    pub name: String,
    /// `c->PK` (`CharacterHeroState` from the packet: 0..6, 3 = normal).
    pub pk: u8,
    /// `c->Level`: for players it is 1 with `PK ≥ PVP_MURDERER2` (WSclient.cpp:2683, 6250).
    pub level: u8,
    /// `CtlCode` `CTLCODE_20OPERATOR`/`CTLCODE_08OPERATOR` or `eBuff_GMEffect`.
    pub gm: bool,
    /// `c->Dead`: the dead are not picked.
    pub dead: bool,
    /// Guild (`GuildMarkIndex` ≥ 0), with the lines already built (`AddGuildName`).
    pub guild: Option<Guild>,
    /// `o->BoundingBoxMin/Max`.
    pub bbox_min: Vec3,
    pub bbox_max: Vec3,
    /// `o->m_fEdgeScale` (1.2 from `CreateCharacterPointer`, ZzzCharacter.cpp:11312).
    pub edge_scale: f32,
    /// `o->Light`: added to the terrain light when drawing. 0 until the mouse first enters
    /// the play area (`CreateCharacterPointer`, ZzzCharacter.cpp:11396).
    pub light: Vec3,
}

/// A character's guild, for its name box.
#[derive(Clone, Debug, PartialEq)]
pub struct Guild {
    /// `GuildMarkIndex`.
    pub mark: i32,
    /// `GuildRelationShip` with the hero's guild (`GR_*`).
    pub relation: u8,
    /// `"<alliance> rank"` (`c->Union`), if any.
    pub union_line: Option<String>,
    /// `"[guild] rank"` (`c->Guild`).
    pub guild_line: String,
}

/// `GR_NONE`, `GR_RIVAL`, `GR_RIVALUNION`, `GR_UNION` (UIGuildInfo.h:30-37).
pub const GR_NONE: u8 = 0x00;
pub const GR_UNION: u8 = 0x01;
pub const GR_RIVAL: u8 = 0x02;
pub const GR_RIVALUNION: u8 = 0x08;

/// `MODEL_PLAYER` box (ZzzCharacter.cpp:11397-11401).
pub const PLAYER_BOX_MIN: Vec3 = Vec3::new(-60.0, -60.0, 0.0);
pub const PLAYER_BOX_MAX: Vec3 = Vec3::new(40.0, 40.0, 120.0);
/// `PVP_MURDERER2`.
pub const PVP_MURDERER2: u8 = 6;

impl Selectable {
    /// Another player (`ReceiveCreatePlayerViewport`) with their `PK`.
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

/// `SelectedCharacter` and `SelectedNpc`.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct Hovered {
    pub character: Option<Entity>,
    pub npc: Option<Entity>,
}

impl Hovered {
    /// Is any character under the mouse? (Then nothing else is checked.)
    pub fn any(&self) -> bool {
        self.character.is_some() || self.npc.is_some()
    }
}

// --- Picking ---------------------------------------------------------------------------------------

/// `SelectCharacter`, first loop: `o->Light` of everyone of the kind by `c->Level`, and −0.4 for
/// murderers. `None`: a `Level` with no case, the light stays as it was.
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

/// `bCanTalk` (ZzzInterface.cpp:7772-7782): in Devias, someone on floor 3 is only picked if the hero
/// also stands on a 3 or an 11. (In Lorencia the condition includes `WorldActive == WD_2DEVIAS` and never holds.)
pub fn can_talk(world: u16, target_tile: u8, hero_tile: u8) -> bool {
    // `WD_2DEVIAS` = folder 3.
    !(world == 3 && target_tile == 3 && target_tile != hero_tile && hero_tile != 11)
}

/// `TerrainMappingLayer1` at a MU position (`Position / TERRAIN_SCALE`).
fn layer1_at(terrain: &mu_data::terrain::Terrain, mu: Vec3) -> u8 {
    let (x, y) = ((mu.x / MU_SCALE) as i32, (mu.y / MU_SCALE) as i32);
    let index = (y * 256 + x).clamp(0, 65535) as usize;
    terrain.mapping.layer1.get(index).copied().unwrap_or(0)
}

/// MU position of an entity placed in tiles (Bevy: x, height, −y).
fn mu_position(tf: &Transform) -> Vec3 {
    Vec3::new(tf.translation.x, -tf.translation.z, tf.translation.y) * MU_SCALE
}

/// One `SelectCharacter(Kind)` call: light for everyone of those kinds and the closest one under the ray.
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
    // With auto-attack, the picked one is only kept if it is a monster being attacked; players
    // and NPCs are searched again every frame.
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
        // (Ground items: none yet.)
        now.npc = pick(&[Kind::Npc]);
        if now.npc.is_none() {
            now.character = pick(&[Kind::Monster]).or_else(|| pick(&[Kind::Player]));
        }
    } else {
        // `CKind_1` = monsters, `CKind_2` = players (no elf/wizard support skills yet).
        now.character = pick(&[Kind::Monster]).or_else(|| pick(&[Kind::Player]));
        if now.character.is_none() {
            now.npc = pick(&[Kind::Npc]);
        }
    }
    hovered.set_if_neq(now);
}

// --- Names -----------------------------------------------------------------------------------------

/// `MAX_CHAT`.
const MAX_CHAT: usize = 120;
/// `IDLifeTime` of a new box and of the one the character already had (25 Hz ticks).
const ID_LIFE_NEW: i32 = 100;
const ID_LIFE_AGAIN: i32 = 10;
/// Height of the box above the picking box (`BoundingBoxMax[2] + 60`, ZzzInterface.cpp:8632).
const NAME_LIFT: f32 = 60.0;

/// A `CHAT` box: the name and, if they spoke, what they said (set by the chat via [`NameBoxes::say`]).
#[derive(Default)]
struct NameSlot {
    owner: Option<Entity>,
    /// `IDLifeTime`.
    id_life: i32,
    /// `Text[2]` and `LifeTime[2]` (`AddChat`, ZzzInterface.cpp:1018-1044).
    text: [String; 2],
    life: [i32; 2],
    ui: Option<SlotUi>,
}

impl NameSlot {
    /// Drawn (`IDLifeTime > 0 || LifeTime[0] > 0`).
    fn active(&self) -> bool {
        self.id_life > 0 || self.life[0] > 0
    }

    /// `AddChat`: `Flag` 0 lasts 2 ticks per byte + 160; 1 (the `#` ones), 1000. With 20 bytes or more it
    /// is split into two lines at the space closest to the middle (`CutText`).
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

    /// The visible chat lines, top to bottom, with their life (`RenderBoolean`, 988-1001).
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

/// The nodes of one box.
struct SlotUi {
    root: Entity,
    /// What is written (to rebuild it only when it changes).
    key: SlotKey,
    /// (row, text) of the alliance, guild and name.
    rows: Vec<(Entity, Entity)>,
}

#[derive(Clone, PartialEq)]
struct SlotKey {
    lines: Vec<String>,
    /// How many of the last ones are chat (below the name).
    chat: usize,
    bold: bool,
    font: f32,
    line: i32,
    pad: f32,
}

/// The 120 `Chat[]` boxes and the ticks.
#[derive(Resource)]
pub struct NameBoxes {
    slots: Vec<NameSlot>,
    /// Accumulated tick fraction.
    acc: f32,
}

impl Default for NameBoxes {
    fn default() -> Self {
        Self { slots: (0..MAX_CHAT).map(|_| NameSlot::default()).collect(), acc: 0.0 }
    }
}

impl NameBoxes {
    /// `CreateChat(ID, "", Owner)`: the box that was already theirs or the first free one.
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

    /// `CreateChat(ID, Text, Owner, Flag)` with text (ZzzInterface.cpp:1106-1169): in the box that was
    /// already theirs (the previous text moves to the second line) or the first free one. `false` if no box.
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

    /// `MoveChat`: one tick less; boxes of characters no longer present are switched off.
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

/// On leaving the game: no boxes (their nodes go away with `NamesRoot`).
fn reset(mut boxes: ResMut<NameBoxes>, mut hovered: ResMut<Hovered>) {
    *boxes = NameBoxes::default();
    *hovered = Hovered::default();
}

/// Chaos Castle worlds (`InChaosCastle`: `WD_18CHAOS_CASTLE..END` and `WD_53CAOSCASTLE_MASTER_LEVEL`).
pub(crate) fn in_chaos_castle(world: u16) -> bool {
    matches!(world, 19..=24 | 54)
}

/// `MoveChat` every tick and `RenderName` (`CreateChat` of the hovered one) every frame.
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
    // The hero has no `Selectable`, but gets a box when speaking.
    let hero_e = hero_entity.single().ok();
    boxes.acc += time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND;
    while boxes.acc >= 1.0 {
        boxes.acc -= 1.0;
        boxes.tick(|e| selectables.contains(e) || hero_e == Some(e));
    }
    // In Chaos Castle names are hidden (except Webzen's, `FindText(Hero->ID, "webzen")`).
    if in_chaos_castle(args.world) && !hero.is_some_and(|h| h.name.contains("webzen")) && hovered.any() {
        return;
    }
    if let Some(npc) = hovered.npc {
        boxes.create(npc);
    } else if let Some(c) = hovered.character
        && selectables.get(c).is_ok_and(|s| s.kind != Kind::Monster)
    {
        // (A monster's goes at the top, `MonsterLabel`.)
        boxes.create(c);
    }
}

/// Font metrics for a window: `FontHeight` (Winmain.cpp:1456-1462) and what follows from it in
/// virtual screen units (`rate` = window pixels per unit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// `g_hFont`: `FontHeight − 1` pixels.
    pub font: f32,
    /// `iLineHeight = FontHeight / g_fScreenRate_y`.
    pub line: i32,
    /// `c->Height = FontHeight · 5 / g_fScreenRate_y`.
    pub height: i32,
    /// The name's 3 extra pixels (`Size[0].cx += 3`).
    pub pad: f32,
}

impl Metrics {
    pub fn new(window_width: f32, rate: f32) -> Self {
        // The closest resolution among those in `switch (WindowWidth)`.
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

/// `x`, `y`, `Width`, `Height` of a box on the virtual screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// `RenderBooleans`: centered over the point, overlapping ones move apart (8654-8675) and are pushed into
/// the screen (8682-8686). Like the original, the right and bottom edges are compared with the window
/// size in pixels (`WindowWidth − Width`), not with the virtual screen size.
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

/// Box color as `RGBA` (0..255).
pub type Rgba = [u8; 4];

/// `SetPlayerColor` (ZzzInterface.cpp:787-799) with `c->Color` = `PK` (0 for NPCs, `CreateChat`).
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

/// `GetGuildRelationShipTextColor` and `GetGuildRelationShipBGColor` (ZzzCharacter.cpp:12009-12035).
pub fn relation_colors(relation: u8) -> (Rgba, Rgba) {
    match relation {
        GR_NONE => ([10, 30, 50, 150], [230, 230, 255, 255]),
        GR_RIVAL | GR_RIVALUNION => ([0, 0, 0, 150], [255, 30, 0, 255]),
        _ => ([20, 50, 80, 150], [200, 255, 0, 255]),
    }
}

/// Colors of a name box: (background, letters) of the guild and name lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxStyle {
    pub guild: (Rgba, Rgba),
    pub name: (Rgba, Rgba),
    pub bold: bool,
}

/// `RenderBoolean` (ZzzInterface.cpp:893-959). `same_guild`: `GuildMarkIndex == Hero->GuildMarkIndex`
/// (also when neither has a guild). `swap`: the mouse is over the box and it is time to alternate (938-946).
pub fn box_style(kind: Kind, is_hero: bool, gm: bool, pk: u8, same_guild: bool, relation: u8, swap: bool) -> BoxStyle {
    // The GM gray background (898) is always overwritten below.
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

/// (background, letters) of a chat line in the box (`RenderBoolean`, 961-986): background from
/// `GuildColor` (no guilds, 0: (10, 30, 50, 150)) and letters (230, 220, 200), or (250, 200, 50)
/// for GMs, half transparent in the last 10 ticks.
pub fn chat_colors(gm: bool, life: i32) -> (Rgba, Rgba) {
    let a = if life > 0 && life < 10 { 128 } else { 255 };
    let text = if gm { [250, 200, 50, a] } else { [230, 220, 200, a] };
    ([10, 30, 50, 150], text)
}

fn rgba(c: Rgba) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// Root of the name boxes: `CNewUINameWindow` is the lowest layer (`GetLayerDepth` 1).
#[derive(Component)]
struct NamesRoot;

/// A monster's name, at the top center, and its text.
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
            // `RenderText(320, 2, c->ID, 0, 0, RT3_WRITE_CENTER)` with (255, 230, 200) on (100, 0, 0).
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
        // `FW_BOLD` (`g_hFontBold`); the Noto Sans in `fonts.rs` is not variable, so it does not change.
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        font_smoothing: FontSmoothing::AntiAliased,
        ..default()
    }
}

/// Places the boxes and colors them (`RenderBooleans` → `RenderBoolean`).
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
    // The camera moves in `CameraMoved` (PostUpdate) and has no parent: its `Transform` is the current one.
    let camera = cameras.single().ok().map(|(c, tf)| (c, GlobalTransform::from(*tf)));
    let hide_all = in_chaos_castle(args.world) && !hero_name.as_ref().is_some_and(|h| h.name.contains("webzen"));

    // The hovered monster's name (hidden in Chaos Castle too, like the rest).
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

    // Cursor (`MouseX`, `MouseY`) on the virtual screen.
    let mouse = window.cursor_position().map(|c| ((c.x / rate) as i32, (c.y / rate) as i32));
    // The hero as owner of its chat box (`CharactersClient` treats it as just another player), with the
    // `PK` from `CharacterInformation`.
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

    // 1) Content and size of each active box.
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
        // The width (`c->Width`) is the one the layout measured in the previous frame (0 for a
        // freshly made box: shown on the next one).
        let width = computed.get(ui.root).map_or(0.0, |c| c.size().x * c.inverse_scale_factor());
        // `Projection` of the point above the head (none behind the camera).
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

    // 2) No overlaps and inside the screen.
    let mut rects: Vec<BoxRect> = active.iter().map(|(_, r)| *r).collect();
    place_boxes(&mut rects, window_px);

    // 3) Colors and position.
    for ((i, _), r) in active.iter().zip(&rects) {
        let slot = &boxes.slots[*i];
        let (Some(ui), Some((s, _, is_hero))) = (slot.ui.as_ref(), slot.owner.and_then(owner_of)) else { continue };
        // With the mouse over it, in a safe zone and not the own one: background and letters alternate periodically.
        // (The test width and height are rescaled, as in the original.)
        let over = mouse.is_some_and(|(mx, my)| {
            r.x <= mx && mx < r.x + r.w * 640 / window_px.0.max(1) && r.y <= my && my < r.y + r.h * 480 / window_px.1.max(1)
        });
        // (Not alternated over the own box: `strcmp(c->ID, Hero->ID) != 0`.)
        let swap = over && hero_safe && world_time % 24 < 12 && !is_hero;
        // The hero has no `Selectable` (its name appears with its chat); without a guild, `-1 == -1`.
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

/// A box: column of rows (alliance, guild, name), each with its background and as wide as the widest
/// (`RenderText(..., c->Width, iLineHeight, RT3_SORT_LEFT)`). Starts hidden until measured.
fn spawn_box(commands: &mut Commands, root: Entity, key: SlotKey) -> SlotUi {
    let mut rows = Vec::new();
    // The name goes above the chat lines.
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
        // Only the name gets the 3 extra pixels; bold only for GM names.
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

// --- Highlight (`RenderPartObjectEdge`) ------------------------------------------------------------

/// A shell of a body part.
struct Shell {
    entity: Entity,
    mesh: Handle<Mesh>,
    /// Index into `Skinned::body_meshes`.
    part: usize,
    /// 0 = the outer one (`m_fEdgeScale`), 1 = the inner one (−0.02).
    pass: usize,
}

/// A character's shells (created the first time it is hovered, then hidden).
#[derive(Component)]
struct EdgeShells(Vec<Shell>);

/// Marker for shell entities.
#[derive(Component)]
struct EdgeShell;

/// `BodyLight` of the two passes (ZzzObject.cpp:10512-10541).
pub fn edge_colors(kind: Kind, chaos_castle: bool) -> [[f32; 3]; 2] {
    if chaos_castle {
        [[0.1, 0.01, 0.0], [0.7, 0.07, 0.0]]
    } else if kind == Kind::Npc {
        [[0.02, 0.1, 0.0], [0.16, 0.7, 0.0]]
    } else {
        [[0.1, 0.03, 0.0], [0.7, 0.2, 0.0]]
    }
}

/// `BoneScale` of the two passes: `m_fEdgeScale` and −0.02; in Chaos Castle `1 + 0.1/Scale` and
/// `1 + 0.04/Scale` (ZzzObject.cpp:10510-10542).
pub fn edge_scales(edge_scale: f32, object_scale: f32, chaos_castle: bool) -> [f32; 2] {
    if chaos_castle {
        // (The second is `1 + 0.04/Scale + 0.02`, drawn with `Scale − 0.02`.)
        let s = object_scale.max(1e-3);
        [1.0 + 0.1 / s, 1.0 + 0.04 / s]
    } else {
        [edge_scale, edge_scale - 0.02]
    }
}

/// Bones with `BoneScale`: `vp = R·v·s + T` is like rotating with `R·s`.
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

/// `pBitmap->Components == 4`: TGA textures (`OpenTga`, GlobalBitmap.cpp:716) get no shell.
pub fn edge_skips_texture(texture: &str) -> bool {
    texture.to_ascii_lowercase().ends_with(".tga")
}

/// Material for one pass: flat additive color, no depth write.
///
/// The original paints the shell before the part and the (opaque) part covers it; here opaque always
/// goes before additive, so only the back faces of the shell are drawn (they stay
/// behind the body and do not tint it) with double the color, which is what the front and back add up to
/// (the original does no face culling) in the halo.
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

/// The outer faces of MU meshes (culled so only the back ones are seen).
const OUTWARD_FACE: Face = Face::Front;

/// Shell materials by (kind, Chaos Castle) and pass.
#[derive(Default)]
struct EdgeMaterials(Vec<((Kind, bool), EdgePair)>);

/// The materials of the two passes.
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

/// Two (hidden) shells for each body part that has one, children of the character.
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
        // A normal player without a guild (like the hero without a guild): alliance background.
        let s = box_style(Kind::Player, false, false, 3, true, GR_NONE, false);
        assert_eq!(s.name, ([20, 50, 80, 150], [200, 220, 255, 255]));
        // The own one: green background, letters by its PK.
        let s = box_style(Kind::Player, true, false, 4, true, GR_NONE, false);
        assert_eq!(s.name, ([60, 100, 0, 150], [255, 150, 60, 255]));
        // GM: bold cyan letters; the gray background is overwritten by the guild one.
        let s = box_style(Kind::Player, false, true, 3, false, GR_RIVAL, false);
        assert_eq!(s.name, ([0, 0, 0, 150], [100, 250, 250, 255]));
        assert!(s.bold);
        // NPC: color 0 even with another PK; with the swap, reversed.
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
        // Two boxes in the same place: the first moves above the second.
        let mut b = [BoxRect { x: 100, y: 100, w: 40, h: 43 }, BoxRect { x: 110, y: 100, w: 40, h: 43 }];
        place_boxes(&mut b, (1024, 768));
        assert_eq!(b[0].y, 100 - 43);
        assert_eq!(b[1].y, 100);
        // Off the top and left: pushed in; on the right only against the width in pixels.
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
        // The free box is taken by another; the first starts over with 100 on the next one.
        boxes.create(b);
        assert_eq!(boxes.slots[0].owner, Some(b));
        boxes.create(a);
        assert_eq!((boxes.slots[1].owner, boxes.slots[1].id_life), (Some(a), ID_LIFE_NEW));
        // If the owner disappears, it switches off.
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
        // The new text pushes the old one to the line above.
        boxes.tick(|_| true);
        boxes.say(a, "adios", 0);
        assert_eq!(boxes.slots[0].chat_lines(), [("hola", 167), ("adios", 170)]);
        // Long: split at the space closest to the middle, both with the same life.
        boxes.say(b, "hola que tal estas hoy", 0);
        assert_eq!(boxes.slots[1].chat_lines(), [("hola que tal ", 204), ("estas hoy", 204)]);
        // While speaking, nobody else takes their box, even if the name is not in view.
        boxes.create(b);
        assert_eq!(boxes.slots[1].id_life, ID_LIFE_AGAIN);
        let c = world.spawn_empty().id();
        boxes.create(c);
        assert_eq!(boxes.slots[2].owner, Some(c));
        // `#`: 1000 ticks.
        boxes.say(a, "aviso", 1);
        assert_eq!(boxes.slots[0].life[0], 1000);
        // The line colors: half transparent at the end.
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
        // `R·v·s + T`: the bone offset is not scaled.
        let mut m: bmd::Mat34 = [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 6.0], [0.0, 0.0, 1.0, 7.0]];
        m = scale_bones(&[m], 1.2)[0];
        assert_eq!(bmd::transform_point(&m, [10.0, 0.0, 0.0]), [17.0, 6.0, 7.0]);
    }

    /// Which faces of MU parts point outward? (For `OUTWARD_FACE`.) Using the right-hand
    /// rule, each triangle's normal is compared with its vertex normals.
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
        // Counter-clockwise (right-hand rule) outward = wgpu front face.
        let ccw_outward = agree * 2 > total;
        assert_eq!(ccw_outward, OUTWARD_FACE == Face::Front);
    }
}
