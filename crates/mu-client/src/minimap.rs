//! Tab minimap (`SEASON3B::CNewUIMiniMap`, NewUIMiniMap.cpp).
//!
//! **When it exists:** on loading each map, `CMapManager::LoadWorld` (MapManager.cpp:1586-1590) calls
//! `LoadImages("WorldN")` with the map folder (except World74/75, the login and character
//! selection screens, which keep the previous map's). `m_bSuccess` = `Data/WorldN/mini_map.ozt` exists
//! (1024×1024); the markers come from `Data/Local/Eng/Minimap/Minimap_WorldN_eng.bmd`
//! (`mu_data::minimap`). Without `m_bSuccess`, Tab (and "Map (Tab)" in the window menu) only closes.
//!
//! **Keyboard** (`GetKeyEventOrder`: message boxes, window menu and options 10, minimap 3,
//! hotkeys 1):
//! - open, Esc or Tab close it with `SOUND_CLICK01` (`UpdateKeyEvent`, NewUIMiniMap.cpp:114-126),
//!   unless the window menu or the options take the Esc first;
//! - open, the other hotkeys do nothing (NewUIHotKey.cpp:135-138);
//! - closed, Tab opens it if there is a minimap (otherwise "closes" it) with `SOUND_CLICK01`
//!   (NewUIHotKey.cpp:283-293); in Valley of Loren (Battle Castle) Tab is the siege window
//!   (277-282), which we lack: it only plays the sound.
//! - with a message box open no key gets through (`CNewUIMessageBoxMng::UpdateKeyEvent`).
//!
//! **Mouse** (`UpdateMouseEvent`, 279-305): the top-right X (`m_BtnExit`, 613,3 30×25)
//! closes on release over it (no sound); all of (0, 0, 640, 430) is UI: a click there does not
//! walk (`GameUi::blocks`), even if the map has no minimap and nothing is shown.
//!
//! **Drawing** (`Render`, 128-203), in order, on the 640×480 screen and below the bar
//! (layer 8.1 vs 10.6):
//! 1. 85 % black over (0, 0, 640, 430) (`RenderColor`);
//! 2. the map: an 800×800 square (`m_Lenth[m_MiniPos]`, `m_MiniPos` always 0: no zoom or
//!    panning) with the character's tile (`Hero->PositionX/Y`, whole tile) at the center
//!    of the screen and rotated 45° counter-clockwise (`RenderBitRotate`, ZzzOpenglUtil.cpp:1315-1360). The
//!    texture U is MU's Y and V is X;
//! 3. the markers (`RenderPointRotate`, ZzzOpenglUtil.cpp:1362-1429): same rotation, then 25 *real*
//!    px to the right, and each icon of 15 (NPC) or 30 (portal) real px rotated `Rotation`
//!    degrees. In Crywolf with the castle occupied only the NPC at (228, 48) is shown (we lack that
//!    server state: all are shown, as in peacetime);
//! 4. the character arrow, fixed and unrotated at (325, 230) 12×12;
//! 5. the frame: 25 pieces of `mini_map_ui_line` on top (flipped) and bottom, 20 on each side rotated
//!    ±90°, and the four flipped `mini_map_ui_corner` corners;
//! 6. the X (`RenderImage` with UV: top of `mini_map_ui_cancel`; when pressed, the V goes past the
//!    texture and nothing shows) with its "Close" tooltip (`GlobalText[1002]`, above and against the edge);
//! 7. the name of the marker under the mouse (`Check_Btn`, 312-351): the first in the list whose box
//!    contains it. `SetBtnPos` stores the box from the first corner of the rotated icon:
//!    7.5×7.5 down-right for NPCs and 30×30 centered on it for portals. White
//!    text on black with alpha 180, in a box 6 wider than the text, above the marker's box.
//!
//! Textures are loaded padded to powers of 2 (`CGlobalBitmap::OpenTga`), so the original's
//! UVs (17.5/32, 41.7/64, 36/64…) are image pixels: 17.5, 41.7, 36…
//!
//! **Missing:** the original's widescreen stretches everything; here, like the bar, it lives in the
//! centered 640×480 (clipped to it). `ClosingProcess` sends `SendExitInventory` to the server (that
//! packet does not exist yet). The original does not draw party members or other players (the
//! `mini_map_ui_party` texture exists but S6 does not use it).

use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, LineHeight};
use bevy::ui::{UiScale, UiTransform, Val2};
use mu_data::minimap::{KIND_NPC, KIND_PORTAL, Marker};

use crate::Args;
use crate::character::Player;
use crate::options::GameUi;
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct MiniMapPlugin;

impl Plugin for MiniMapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MiniMap>()
            .add_systems(OnEnter(GameScene::Game), spawn_minimap)
            .add_systems(OnExit(GameScene::Game), crate::scene::despawn::<MiniMapRoot>)
            // Before everything else: takes Esc/Tab and masks the hotkeys.
            .add_systems(
                PreUpdate,
                // After the chat: while typing, the keys belong to it (chat.rs).
                minimap_keys.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .add_systems(Update, (load_minimap, minimap_mouse, draw_minimap).chain().run_if(in_state(GameScene::Game)));
    }
}

// --- Measurements (640×480 screen) -------------------------------------------------------------

/// `m_Lenth[0]` (NewUIMiniMap.cpp:54-65): side of the drawn map.
const LENGTH: f32 = 800.0;
/// `Rot` in `Render` (130): map rotation, counter-clockwise.
const ROTATION: f32 = 45.0;
/// Screen center (`WindowWidth/2`, `WindowHeight/2`).
const CENTER: Vec2 = Vec2::new(320.0, 240.0);
/// Marker offset, in real px (`Matrix[0][3] = p3[0]+25`, ZzzOpenglUtil.cpp:1405).
const MARKER_SHIFT_PX: f32 = 25.0;
/// `NpcWidth` and `NpcWidthP` (154-155), in real px.
const NPC_PX: f32 = 15.0;
const PORTAL_PX: f32 = 30.0;
/// `m_BtnExit` (`SetPos`, 103): (640 − 27, 3) 30×25.
const CLOSE: [f32; 4] = [640.0 - 27.0, 3.0, 30.0, 25.0];
/// `GlobalText[1002]`.
const CLOSE_TEXT: &str = "Close";
/// Height of `RenderColor` and of what the mouse treats as UI.
const AREA_H: f32 = 430.0;
/// `g_hFont` at 1024×768: 13 px (like options.rs).
const FONT: f32 = 13.0 * 480.0 / 768.0;
/// `WD_30BATTLECASTLE` + 1.
const BATTLE_CASTLE: u16 = 31;
/// `WD_34CRYWOLF_1ST` + 1.
const CRYWOLF: u16 = 35;
/// Language (`g_strSelectedML`): the `Data/Local/Eng` folder.
const LANG: &str = "Eng";

/// Keys of `CNewUIHotKey::UpdateKeyEvent` after line 135 (F, I/V, C, T, P, G, A, U,
/// D, F1, M, X and B): blocked while the minimap is open.
const HOTKEYS: [KeyCode; 14] = [
    KeyCode::KeyF,
    KeyCode::KeyI,
    KeyCode::KeyV,
    KeyCode::KeyC,
    KeyCode::KeyT,
    KeyCode::KeyP,
    KeyCode::KeyG,
    KeyCode::KeyA,
    KeyCode::KeyU,
    KeyCode::KeyD,
    KeyCode::F1,
    KeyCode::KeyM,
    KeyCode::KeyX,
    KeyCode::KeyB,
];

// --- State -------------------------------------------------------------------------------------

/// What `CNewUIMiniMap` keeps about the loaded map (not cleared when leaving the game: the original
/// does not reread it either unless the map changes).
#[derive(Resource, Default)]
struct MiniMap {
    /// Folder of the last `LoadWorld` (`None` = none yet).
    world: Option<u16>,
    /// `m_bSuccess`.
    success: bool,
    /// `IMAGE_MINIMAP_INTERFACE`: `Data/WorldN/mini_map.ozt`.
    map: Option<Handle<Image>>,
    /// `m_Mini_Map_Data`, up to the first `Kind` 0.
    markers: Vec<Marker>,
    /// `m_BtnExit` in `BUTTON_STATE_DOWN`.
    close_down: bool,
}

/// `CNewUIMiniMap::UpdateKeyEvent` and the Tab part of `CNewUIHotKey::UpdateKeyEvent`.
fn minimap_keys(mut keys: ResMut<ButtonInput<KeyCode>>, mut ui: ResMut<GameUi>, args: Res<Args>, mut sound: ResMut<SoundQueue>) {
    if ui.modal() {
        return;
    }
    let tab = keys.just_pressed(KeyCode::Tab);
    if ui.minimap {
        // Esc is handled first by the window menu and the options (order 10).
        let esc = keys.just_pressed(KeyCode::Escape) && !ui.window_menu_open() && !ui.option_open();
        if esc || tab {
            ui.minimap = false;
            sound.play("SOUND_CLICK01");
            keys.clear_just_pressed(KeyCode::Escape);
            keys.clear_just_pressed(KeyCode::Tab);
            return;
        }
        for k in HOTKEYS {
            keys.clear_just_pressed(k);
        }
    } else if tab {
        if args.world != BATTLE_CASTLE {
            // `Hide` without `m_bSuccess`, `Toggle` with it (it was closed here: opens it).
            ui.minimap = ui.minimap_ok;
        }
        sound.play("SOUND_CLICK01");
    }
}

/// `CMapManager::LoadWorld` → `UnloadImages` + `LoadImages(WorldName)` (MapManager.cpp:1586-1590).
fn load_minimap(args: Res<Args>, mut mm: ResMut<MiniMap>, mut ui: ResMut<GameUi>, mut images: ResMut<Assets<Image>>) {
    let folder = mu_data::terrain::data_world(args.world);
    if mm.world != Some(folder) {
        mm.world = Some(folder);
        if folder != 74 && folder != 75 {
            mm.success = mu_data::minimap::has_minimap(&args.data, folder);
            mm.map = None;
            if mm.success {
                let dir = mu_data::fs::find(&args.data, &format!("World{folder}"));
                match dir.ok_or_else(|| "no hay carpeta".to_string()).and_then(|d| {
                    mu_data::texture::load(&d, "mini_map.ozt").map_err(|e| e.to_string())
                }) {
                    Ok(t) => mm.map = Some(images.add(crate::terrain::to_bevy_image(&t))),
                    Err(e) => warn!("minimapa: World{folder}/mini_map.ozt: {e}"),
                }
                // Markers are only reread if there is a map (the `return` on line 220).
                mm.markers = mu_data::minimap::load(&args.data, folder, LANG).unwrap_or_else(|e| {
                    warn!("minimapa: World{folder}: {e}");
                    Vec::new()
                });
            }
        }
    }
    if ui.minimap_ok != mm.success {
        ui.minimap_ok = mm.success;
    }
}

/// `m_BtnExit.UpdateMouseEvent` (`CNewUIBaseButton::Process`, NewUIButton.cpp:115-138).
fn minimap_mouse(windows: Query<&Window>, mouse: Res<ButtonInput<MouseButton>>, mut ui: ResMut<GameUi>, mut mm: ResMut<MiniMap>) {
    let cursor = windows.single().ok().and_then(crate::mainframe::cursor_virtual);
    let over = ui.minimap && !ui.modal() && cursor.is_some_and(|c| in_rect(c, CLOSE));
    let down = over && mouse.pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Left);
    if mm.close_down != down {
        mm.close_down = down;
    }
    if over && mouse.just_released(MouseButton::Left) {
        ui.minimap = false;
    }
}

/// `CheckMouseIn(x, y, width, height)`.
fn in_rect(c: Vec2, r: [f32; 4]) -> bool {
    c.x >= r[0] && c.x < r[0] + r[2] && c.y >= r[1] && c.y < r[1] + r[3]
}

// --- Geometry ----------------------------------------------------------------------------------

/// `AngleMatrix` with only a z angle: counter-clockwise rotation on screen (the original's y points
/// up), written for coordinates with y pointing down.
fn turn(v: Vec2, degrees: f32) -> Vec2 {
    let (s, c) = degrees.to_radians().sin_cos();
    Vec2::new(v.x * c + v.y * s, -v.x * s + v.y * c)
}

/// Point of the (unrotated) map square for tile (x, y): `(y/256·800, x/256·800)`.
fn map_point(x: f32, y: f32) -> Vec2 {
    Vec2::new(y / 256.0 * LENGTH, x / 256.0 * LENGTH)
}

/// Center of the map square on screen (`RenderBitRotate`): the character's tile
/// stays at the screen center and everything rotates 45° around it.
fn map_center(hero: (i32, i32)) -> Vec2 {
    let t = map_point(hero.0 as f32, hero.1 as f32);
    CENTER + turn(Vec2::splat(LENGTH / 2.0) - t, ROTATION)
}

/// Where texture point (u, v) of the map lands on screen (for the tests).
#[cfg(test)]
fn map_uv_to_screen(hero: (i32, i32), u: f32, v: f32) -> Vec2 {
    map_center(hero) + turn(Vec2::new((u - 0.5) * LENGTH, (v - 0.5) * LENGTH), ROTATION)
}

/// Center of a marker's icon (`RenderPointRotate`). `px` = 640×480 units per real px.
fn marker_center(hero: (i32, i32), m: &Marker, px: f32) -> Vec2 {
    let d = map_point(m.location[0] as f32, m.location[1] as f32) - map_point(hero.0 as f32, hero.1 as f32);
    CENTER + turn(d, ROTATION) + Vec2::new(MARKER_SHIFT_PX * px, 0.0)
}

/// Side of a marker's icon in real px.
fn marker_px(m: &Marker) -> f32 {
    if m.kind == KIND_PORTAL { PORTAL_PX } else { NPC_PX }
}

/// The `SetBtnPos` box (x, y, width, height): from the first corner of the rotated icon
/// (`p4[0]`, top-left before rotating).
fn marker_box(center: Vec2, m: &Marker, px: f32) -> [f32; 4] {
    let size = marker_px(m);
    let half = size * px / 2.0;
    let corner = center + turn(Vec2::new(-half, -half), m.rotation as f32);
    if m.kind == KIND_PORTAL {
        [corner.x - size / 2.0, corner.y - size / 2.0, size, size]
    } else {
        [corner.x, corner.y, size / 2.0, size / 2.0]
    }
}

/// `Check_Btn`: strictly inside, with the mouse in integer units (`MouseX`, `MouseY`).
fn in_box(c: Vec2, b: [f32; 4]) -> bool {
    let (x, y) = (c.x.floor(), c.y.floor());
    x > b[0] && x < b[0] + b[2] && y > b[1] && y < b[1] + b[3]
}

/// Whether the marker is drawn (NewUIMiniMap.cpp:164-171): in occupied Crywolf, of the NPCs only the one at (228, 48).
fn marker_drawn(m: &Marker, world: u16, crywolf_occupied: bool) -> bool {
    match m.kind {
        KIND_NPC => !(world == CRYWOLF && crywolf_occupied) || (m.location == [228, 48] && world == CRYWOLF),
        KIND_PORTAL => true,
        _ => false,
    }
}

// --- Drawing -----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    Map,
    Corner,
    Line,
    Hero,
    Portal,
    Npc,
    Cancel,
}

/// `Create` (NewUIMiniMap.cpp:41-46).
const INTERFACE: [(Tex, &str); 6] = [
    (Tex::Corner, "mini_map_ui_corner.tga"),
    (Tex::Line, "mini_map_ui_line.jpg"),
    (Tex::Hero, "mini_map_ui_cha.tga"),
    (Tex::Portal, "mini_map_ui_portal.tga"),
    (Tex::Npc, "mini_map_ui_npc.tga"),
    (Tex::Cancel, "mini_map_ui_cancel.tga"),
];

#[derive(Resource)]
struct MiniMapTextures(std::collections::HashMap<Tex, Handle<Image>>);

/// One piece on the 640×480 screen.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// Black `RenderColor`.
    Shade { x: f32, y: f32, w: f32, h: f32, alpha: f32 },
    /// `w`×`h` image centered at `c`, with the `src` region of the texture (px; `None` = whole),
    /// flipped and rotated `turn` degrees counter-clockwise.
    Image { tex: Tex, c: Vec2, w: f32, h: f32, src: Option<Rect>, flip: (bool, bool), turn: f32 },
    /// Tooltip: black box with alpha 180 and white text; `x` is the text center or, with
    /// `right`, the box sticks to the right edge.
    Tip { x: f32, y: f32, right: bool, text: String },
}

fn image(tex: Tex, x: f32, y: f32, w: f32, h: f32, src: Rect, flip: (bool, bool)) -> Piece {
    Piece::Image { tex, c: Vec2::new(x + w / 2.0, y + h / 2.0), w, h, src: Some(src), flip, turn: 0.0 }
}

/// What gets drawn, back to front. `hero` is the character's tile and `px` the 640×480
/// units per real px.
fn pieces(mm: &MiniMap, world: u16, hero: (i32, i32), px: f32, cursor: Option<Vec2>) -> Vec<Piece> {
    let mut out = vec![Piece::Shade { x: 0.0, y: 0.0, w: 640.0, h: AREA_H, alpha: 0.85 }];
    out.push(Piece::Image {
        tex: Tex::Map,
        c: map_center(hero),
        w: LENGTH,
        h: LENGTH,
        src: None,
        flip: (false, false),
        turn: ROTATION,
    });

    // Markers: 17.5/32 of the 18×18 texture padded to 32×32.
    let icon = Rect::new(0.0, 0.0, 17.5, 17.5);
    let mut boxes = Vec::with_capacity(mm.markers.len());
    for m in &mm.markers {
        let c = marker_center(hero, m, px);
        let drawn = marker_drawn(m, world, false);
        if drawn {
            let tex = if m.kind == KIND_PORTAL { Tex::Portal } else { Tex::Npc };
            let side = marker_px(m) * px;
            out.push(Piece::Image { tex, c, w: side, h: side, src: Some(icon), flip: (false, false), turn: m.rotation as f32 });
        }
        // Undrawn markers get no box (in the original, the previous frame's box).
        boxes.push(drawn.then(|| marker_box(c, m, px)));
    }

    // The character arrow, unrotated.
    out.push(image(Tex::Hero, 325.0, 230.0, 12.0, 12.0, icon, (false, false)));

    // Frame (`uvxy` = 41.7/64 of the 42-wide texture padded to 64; `Ui_wid` 35, `Ui_Hig` 6).
    let (wid, hig) = (35.0, 6.0);
    let line = Rect::new(0.0, 0.0, 41.7, 8.0);
    for i in 0..25 {
        let x = i as f32 * wid;
        out.push(image(Tex::Line, x, 0.0, wid, hig, line, (false, true)));
        out.push(image(Tex::Line, x, AREA_H - hig, wid, hig, line, (false, false)));
    }
    for i in 0..20 {
        // `RenderBitmapRotate`: (x, y) is the center; −90° on the left and 90° on the right.
        let y = i as f32 * (wid - 3.0);
        for (x, t) in [(hig / 2.0, -90.0), (640.0 - hig / 2.0, 90.0)] {
            out.push(Piece::Image { tex: Tex::Line, c: Vec2::new(x, y), w: wid, h: hig, src: Some(line), flip: (false, false), turn: t });
        }
    }
    let corner = Rect::new(0.0, 0.0, 41.7, 41.7);
    for (x, y, flip) in [
        (0.0, 0.0, (false, false)),
        (640.0 - wid, 0.0, (true, false)),
        (0.0, AREA_H - wid, (false, true)),
        (640.0 - wid, AREA_H - wid, (true, true)),
    ] {
        out.push(image(Tex::Corner, x, y, wid, wid, corner, flip));
    }

    // The X: 36×29 from the top of the 36×58 texture (padded to 64×64). When pressed (`m_CurImgState`
    // 1), V starts at 25 and leaves the texture: the empty padding.
    let [bx, by, bw, bh] = CLOSE;
    if !mm.close_down {
        out.push(image(Tex::Cancel, bx, by, bw, bh, Rect::new(0.0, 0.0, 36.0, 29.0), (false, false)));
    }
    if cursor.is_some_and(|c| in_rect(c, CLOSE)) {
        // `m_IsTopPos`: above the button; wider than the screen, it snaps to the edge.
        out.push(Piece::Tip { x: 0.0, y: by - (FONT + 2.0), right: true, text: CLOSE_TEXT.into() });
    }

    // `Check_Btn`: the first marker containing the mouse.
    if let Some(c) = cursor {
        let hit = mm.markers.iter().zip(&boxes).find_map(|(m, b)| b.filter(|b| in_box(c, *b)).map(|b| (m, b)));
        if let Some((m, b)) = hit {
            out.push(Piece::Tip { x: b[0] + b[2] / 2.0, y: b[1] - (FONT + 2.0), right: false, text: m.name.clone() });
        }
    }
    out
}

#[derive(Component)]
struct MiniMapRoot;

fn spawn_minimap(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>, loaded: Option<Res<MiniMapTextures>>) {
    if loaded.is_none() {
        let textures = INTERFACE
            .iter()
            .filter_map(|&(t, f)| crate::scene::interface_image(&args, &mut images, f).map(|(h, _)| (t, h)))
            .collect();
        commands.insert_resource(MiniMapTextures(textures));
    }
    // The 640×480 screen like the bar, clipped to it and below the bar (layer 8.1).
    commands.spawn((
        Name::new("Minimapa"),
        MiniMapRoot,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(0.0),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-320.0)),
            width: Val::Px(640.0),
            height: Val::Px(480.0),
            overflow: Overflow::clip(),
            ..default()
        },
        GlobalZIndex(-1),
    ));
}

/// Rebuilds the minimap when something it draws changes.
#[allow(clippy::too_many_arguments)]
fn draw_minimap(
    mut commands: Commands,
    ui: Res<GameUi>,
    mm: Res<MiniMap>,
    args: Res<Args>,
    scale: Res<UiScale>,
    windows: Query<&Window>,
    players: Query<&Player>,
    textures: Option<Res<MiniMapTextures>>,
    root: Query<Entity, With<MiniMapRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>)>,
) {
    let (Ok(root), Some(textures)) = (root.single(), textures) else { return };
    let window = windows.single().ok();
    let cursor = window.and_then(crate::mainframe::cursor_virtual);
    // 640×480 units per real window px (without a window, the original's 1024×768).
    let px = window.map_or(640.0 / 1024.0, |w| 1.0 / (scale.0 * w.scale_factor()).max(1e-3));
    let hero = players.iter().next().map(|p| (p.pos.x.floor() as i32, p.pos.y.floor() as i32));
    let pieces = match hero {
        // `Render` returns immediately without `m_bSuccess`.
        Some(hero) if ui.minimap && mm.success => pieces(&mm, args.world, hero, px, cursor),
        _ => Vec::new(),
    };
    if last.0 == Some(root) && last.1 == pieces {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Shade { x, y, w, h, alpha } => {
                    p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, *alpha))));
                }
                Piece::Image { tex, c, w, h, src, flip, turn } => {
                    let handle = if *tex == Tex::Map { mm.map.clone() } else { textures.0.get(tex).cloned() };
                    let Some(handle) = handle else { continue };
                    let mut image = ImageNode::new(handle);
                    image.rect = *src;
                    (image.flip_x, image.flip_y) = *flip;
                    // Bevy rotates clockwise: the original's counter-clockwise is negative.
                    p.spawn((
                        absolute(c.x - w / 2.0, c.y - h / 2.0, *w, *h),
                        image,
                        UiTransform::from_rotation(Rot2::degrees(-turn)),
                    ));
                }
                Piece::Tip { x, y, right, text } => {
                    let mut node = Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(*y),
                        height: Val::Px(FONT),
                        padding: UiRect::horizontal(Val::Px(3.0)),
                        ..default()
                    };
                    // Box as wide as the text + 6, starting at `center − text/2`: its center
                    // ends up 3 to the right of the marker box's center.
                    let transform = if *right {
                        node.right = Val::Px(0.0);
                        UiTransform::IDENTITY
                    } else {
                        node.left = Val::Px(x + 3.0);
                        UiTransform::from_translation(Val2::new(Val::Percent(-50.0), Val::Px(0.0)))
                    };
                    p.spawn((node, transform, BackgroundColor(Color::srgba_u8(0, 0, 0, 180)))).with_children(|t| {
                        t.spawn((
                            Text::new(text.clone()),
                            TextFont { font_size: FontSize::Px(FONT), font_smoothing: FontSmoothing::AntiAliased, ..default() },
                            LineHeight::Px(FONT),
                            TextColor(Color::WHITE),
                            TextLayout::no_wrap(),
                        ));
                    });
                }
            }
        }
    });
    *last = (Some(root), pieces);
}

fn absolute(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x),
        top: Val::Px(y),
        width: Val::Px(w),
        height: Val::Px(h),
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(kind: u8, x: i32, y: i32, rotation: i32) -> Marker {
        Marker { kind, location: [x, y], rotation, name: "n".into() }
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn el_personaje_queda_en_el_centro() {
        for hero in [(0, 0), (121, 108), (255, 3), (128, 128)] {
            let uv = (hero.1 as f32 / 256.0, hero.0 as f32 / 256.0);
            assert!(close(map_uv_to_screen(hero, uv.0, uv.1), CENTER), "{hero:?}");
            // Without the 25 px offset, a marker on the character's tile is also centered.
            assert!(close(marker_center(hero, &marker(1, hero.0, hero.1, 0), 0.0), CENTER));
        }
    }

    #[test]
    fn giro_de_45_antihorario() {
        let hero = (128, 128);
        // One more tile of Y is +U: unrotated it would go right; rotated 45° counter-clockwise,
        // right and up.
        let d = marker_center(hero, &marker(1, 128, 160, 0), 0.0) - CENTER;
        let step = 32.0 / 256.0 * LENGTH;
        assert!(close(d, Vec2::new(step, -step) * std::f32::consts::FRAC_1_SQRT_2));
        // One more of X is +V: unrotated, down; rotated, down and to the right.
        let d = marker_center(hero, &marker(1, 160, 128, 0), 0.0) - CENTER;
        assert!(close(d, Vec2::new(step, step) * std::f32::consts::FRAC_1_SQRT_2));
        // Marker and map agree: the texture point of that tile (U = Y, V = X) lands
        // in the same place.
        assert!(close(map_uv_to_screen(hero, 128.0 / 256.0, 160.0 / 256.0), CENTER + d));
    }

    #[test]
    fn el_giro_coincide_con_el_de_bevy() {
        // `UiTransform` rotates clockwise: `Rot2::degrees(-t)` must match `turn`.
        let v = Vec2::new(3.0, -7.0);
        for t in [0.0, 45.0, 90.0, 135.0, 270.0] {
            assert!(close(Mat2::from(Rot2::degrees(-t)) * v, turn(v, t)), "{t}");
        }
    }

    #[test]
    fn cajas_de_las_marcas() {
        let hero = (100, 100);
        let px = 640.0 / 1024.0;
        // Unrotated NPC: 7.5×7.5 from the top-left corner of the 15 px icon.
        let m = marker(KIND_NPC, 100, 100, 0);
        let c = marker_center(hero, &m, px);
        assert!(close(c, CENTER + Vec2::new(25.0 * px, 0.0)));
        let b = marker_box(c, &m, px);
        assert!(close(Vec2::new(b[0], b[1]), c - Vec2::splat(7.5 * px)));
        assert_eq!((b[2], b[3]), (7.5, 7.5));
        // Portal rotated 90°: the first corner moves to bottom-left; 30 box centered on it.
        let m = marker(KIND_PORTAL, 100, 100, 90);
        let b = marker_box(c, &m, px);
        let corner = c + Vec2::new(-15.0 * px, 15.0 * px);
        assert!(close(Vec2::new(b[0] + 15.0, b[1] + 15.0), corner));
        assert_eq!((b[2], b[3]), (30.0, 30.0));
        // `Check_Btn` is strict and uses integer mouse coordinates.
        assert!(in_box(Vec2::new(5.9, 5.2), [4.0, 4.0, 3.0, 3.0]));
        assert!(!in_box(Vec2::new(4.9, 5.0), [4.0, 4.0, 3.0, 3.0]));
    }

    #[test]
    fn crywolf_ocupado_solo_deja_un_npc() {
        assert!(marker_drawn(&marker(KIND_NPC, 1, 2, 0), CRYWOLF, false));
        assert!(!marker_drawn(&marker(KIND_NPC, 1, 2, 0), CRYWOLF, true));
        assert!(marker_drawn(&marker(KIND_NPC, 228, 48, 0), CRYWOLF, true));
        assert!(marker_drawn(&marker(KIND_PORTAL, 1, 2, 0), CRYWOLF, true));
        assert!(marker_drawn(&marker(KIND_NPC, 1, 2, 0), 1, true));
    }

    #[test]
    fn ayuda_de_la_marca_bajo_el_raton() {
        let mm = MiniMap { success: true, markers: vec![marker(KIND_NPC, 100, 100, 0)], ..default() };
        let px = 640.0 / 1024.0;
        let c = marker_center((100, 100), &mm.markers[0], px);
        let inside = c - Vec2::splat(7.5 * px) + Vec2::splat(2.0);
        let tip = |cursor| pieces(&mm, 1, (100, 100), px, Some(cursor)).into_iter().find(|p| matches!(p, Piece::Tip { right: false, .. }));
        assert!(matches!(tip(inside), Some(Piece::Tip { text, .. }) if text == "n"));
        assert!(tip(Vec2::new(10.0, 300.0)).is_none());
    }
}
