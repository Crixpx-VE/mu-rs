//! Minimapa del Tab (`SEASON3B::CNewUIMiniMap`, NewUIMiniMap.cpp).
//!
//! **Cuándo hay:** al cargar cada mapa, `CMapManager::LoadWorld` (MapManager.cpp:1586-1590) llama a
//! `LoadImages("WorldN")` con la carpeta del mapa (menos en World74/75, la pantalla del login y la
//! de selección, donde se queda lo del mapa anterior). `m_bSuccess` = existe `Data/WorldN/mini_map.ozt`
//! (1024×1024); las marcas salen de `Data/Local/Eng/Minimap/Minimap_WorldN_eng.bmd`
//! (`mu_data::minimap`). Sin `m_bSuccess`, Tab (y "Map (Tab)" del menú de ventanas) solo cierra.
//!
//! **Teclado** (`GetKeyEventOrder`: cajas de mensajes, menú de ventanas y opciones 10, minimapa 3,
//! teclas rápidas 1):
//! - abierto, Esc o Tab lo cierran con `SOUND_CLICK01` (`UpdateKeyEvent`, NewUIMiniMap.cpp:114-126),
//!   salvo que el Esc se lo quede antes el menú de ventanas o las opciones;
//! - abierto, el resto de teclas rápidas no hacen nada (NewUIHotKey.cpp:135-138);
//! - cerrado, Tab lo abre si hay minimapa (si no, lo "cierra") con `SOUND_CLICK01`
//!   (NewUIHotKey.cpp:283-293); en el Valle de Loren (Battle Castle) Tab es la ventana del asedio
//!   (277-282), que no tenemos: solo suena.
//! - con una caja de mensajes abierta no llega ninguna tecla (`CNewUIMessageBoxMng::UpdateKeyEvent`).
//!
//! **Ratón** (`UpdateMouseEvent`, 279-305): la X de arriba a la derecha (`m_BtnExit`, 613,3 30×25)
//! cierra al soltar encima (sin sonido); todo (0, 0, 640, 430) es interfaz: un clic ahí no hace
//! caminar (`GameUi::blocks`), aunque el mapa no tenga minimapa y no se vea nada.
//!
//! **Dibujo** (`Render`, 128-203), por orden, en la pantalla de 640×480 y por debajo de la barra
//! (capa 8.1 contra 10.6):
//! 1. negro al 85 % en (0, 0, 640, 430) (`RenderColor`);
//! 2. el mapa: un cuadrado de 800×800 (`m_Lenth[m_MiniPos]`, `m_MiniPos` siempre 0: sin zoom ni
//!    desplazamiento) con la casilla del personaje (`Hero->PositionX/Y`, casilla entera) en el centro
//!    de la pantalla y girado 45° antihorario (`RenderBitRotate`, ZzzOpenglUtil.cpp:1315-1360). La U
//!    de la textura es la Y de MU y la V la X;
//! 3. las marcas (`RenderPointRotate`, ZzzOpenglUtil.cpp:1362-1429): mismo giro, pero luego 25 px
//!    *reales* a la derecha, y cada icono de 15 (NPC) o 30 (portal) px reales girado `Rotation`
//!    grados. En Crywolf con el castillo ocupado solo se ve el NPC de (228, 48) (no tenemos ese
//!    estado del servidor: se ven todos, como en paz);
//! 4. la flecha del personaje, quieta y sin girar en (325, 230) 12×12;
//! 5. el marco: 25 trozos de `mini_map_ui_line` arriba (volteado) y abajo, 20 a cada lado girados
//!    ±90°, y las cuatro esquinas `mini_map_ui_corner` volteadas;
//! 6. la X (`RenderImage` con UV: arriba de `mini_map_ui_cancel`; apretada, la V sale de la
//!    textura y no se ve nada) con su ayuda "Close" (`GlobalText[1002]`, arriba y pegada al borde);
//! 7. el nombre de la marca bajo el ratón (`Check_Btn`, 312-351): la primera de la lista cuya caja
//!    lo contiene. La caja la guarda `SetBtnPos` desde la primera esquina del icono girado: de
//!    7.5×7.5 hacia abajo-derecha en los NPC y de 30×30 centrada en ella en los portales. Texto
//!    blanco sobre negro de alfa 180, en una caja 6 más ancha que el texto, encima de la caja.
//!
//! Las texturas se cargan ampliadas a potencias de 2 (`CGlobalBitmap::OpenTga`), así que las UV del
//! original (17.5/32, 41.7/64, 36/64…) son píxeles de la imagen: 17.5, 41.7, 36…
//!
//! **No está:** la pantalla panorámica del original estira todo; aquí, como la barra, va en el
//! 640×480 centrado (recortado a él). `ClosingProcess` manda `SendExitInventory` al servidor (no
//! hay ese paquete todavía). El original no pinta compañeros de grupo ni otros jugadores (la
//! textura `mini_map_ui_party` existe pero el S6 no la usa).

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
            // Antes que el resto: se queda el Esc/Tab y tapa las teclas rápidas.
            .add_systems(
                PreUpdate,
                // Después del chat: mientras se escribe, las teclas son suyas (chat.rs).
                minimap_keys.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .add_systems(Update, (load_minimap, minimap_mouse, draw_minimap).chain().run_if(in_state(GameScene::Game)));
    }
}

// --- Medidas (pantalla de 640×480) -------------------------------------------------------------

/// `m_Lenth[0]` (NewUIMiniMap.cpp:54-65): lado del mapa dibujado.
const LENGTH: f32 = 800.0;
/// `Rot` de `Render` (130): giro del mapa, antihorario.
const ROTATION: f32 = 45.0;
/// Centro de la pantalla (`WindowWidth/2`, `WindowHeight/2`).
const CENTER: Vec2 = Vec2::new(320.0, 240.0);
/// Desplazamiento de las marcas, en px reales (`Matrix[0][3] = p3[0]+25`, ZzzOpenglUtil.cpp:1405).
const MARKER_SHIFT_PX: f32 = 25.0;
/// `NpcWidth` y `NpcWidthP` (154-155), en px reales.
const NPC_PX: f32 = 15.0;
const PORTAL_PX: f32 = 30.0;
/// `m_BtnExit` (`SetPos`, 103): (640 − 27, 3) 30×25.
const CLOSE: [f32; 4] = [640.0 - 27.0, 3.0, 30.0, 25.0];
/// `GlobalText[1002]`.
const CLOSE_TEXT: &str = "Close";
/// Alto de `RenderColor` y de lo que el ratón da por interfaz.
const AREA_H: f32 = 430.0;
/// `g_hFont` a 1024×768: 13 px (como options.rs).
const FONT: f32 = 13.0 * 480.0 / 768.0;
/// `WD_30BATTLECASTLE` + 1.
const BATTLE_CASTLE: u16 = 31;
/// `WD_34CRYWOLF_1ST` + 1.
const CRYWOLF: u16 = 35;
/// Idioma (`g_strSelectedML`): la carpeta `Data/Local/Eng`.
const LANG: &str = "Eng";

/// Teclas de `CNewUIHotKey::UpdateKeyEvent` después de la línea 135 (F, I/V, C, T, P, G, A, U,
/// D, F1, M, X y B): con el minimapa abierto no llegan.
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

// --- Estado ------------------------------------------------------------------------------------

/// Lo que guarda `CNewUIMiniMap` del mapa cargado (no se borra al salir del juego: el original
/// tampoco lo vuelve a leer si no cambia de mapa).
#[derive(Resource, Default)]
struct MiniMap {
    /// Carpeta del último `LoadWorld` (`None` = ninguno todavía).
    world: Option<u16>,
    /// `m_bSuccess`.
    success: bool,
    /// `IMAGE_MINIMAP_INTERFACE`: `Data/WorldN/mini_map.ozt`.
    map: Option<Handle<Image>>,
    /// `m_Mini_Map_Data`, hasta el primer `Kind` 0.
    markers: Vec<Marker>,
    /// `m_BtnExit` en `BUTTON_STATE_DOWN`.
    close_down: bool,
}

/// `CNewUIMiniMap::UpdateKeyEvent` y la parte del Tab de `CNewUIHotKey::UpdateKeyEvent`.
fn minimap_keys(mut keys: ResMut<ButtonInput<KeyCode>>, mut ui: ResMut<GameUi>, args: Res<Args>, mut sound: ResMut<SoundQueue>) {
    if ui.modal() {
        return;
    }
    let tab = keys.just_pressed(KeyCode::Tab);
    if ui.minimap {
        // El Esc lo atienden antes el menú de ventanas y las opciones (orden 10).
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
            // `Hide` sin `m_bSuccess`, `Toggle` con él (aquí estaba cerrado: lo abre).
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
                // Las marcas solo se vuelven a leer si hay mapa (el `return` de la línea 220).
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

/// `CheckMouseIn(x, y, ancho, alto)`.
fn in_rect(c: Vec2, r: [f32; 4]) -> bool {
    c.x >= r[0] && c.x < r[0] + r[2] && c.y >= r[1] && c.y < r[1] + r[3]
}

// --- Geometría ---------------------------------------------------------------------------------

/// `AngleMatrix` con solo ángulo en z: giro antihorario en la pantalla (la y del original va hacia
/// arriba), escrito para coordenadas con la y hacia abajo.
fn turn(v: Vec2, degrees: f32) -> Vec2 {
    let (s, c) = degrees.to_radians().sin_cos();
    Vec2::new(v.x * c + v.y * s, -v.x * s + v.y * c)
}

/// Punto del cuadrado del mapa (sin girar) de la casilla (x, y): `(y/256·800, x/256·800)`.
fn map_point(x: f32, y: f32) -> Vec2 {
    Vec2::new(y / 256.0 * LENGTH, x / 256.0 * LENGTH)
}

/// Centro del cuadrado del mapa en la pantalla (`RenderBitRotate`): la casilla del personaje
/// queda en el centro de la pantalla y todo gira 45° alrededor de él.
fn map_center(hero: (i32, i32)) -> Vec2 {
    let t = map_point(hero.0 as f32, hero.1 as f32);
    CENTER + turn(Vec2::splat(LENGTH / 2.0) - t, ROTATION)
}

/// Dónde cae en la pantalla el punto (u, v) de la textura del mapa (para las pruebas).
#[cfg(test)]
fn map_uv_to_screen(hero: (i32, i32), u: f32, v: f32) -> Vec2 {
    map_center(hero) + turn(Vec2::new((u - 0.5) * LENGTH, (v - 0.5) * LENGTH), ROTATION)
}

/// Centro del icono de una marca (`RenderPointRotate`). `px` = unidades de 640×480 por px real.
fn marker_center(hero: (i32, i32), m: &Marker, px: f32) -> Vec2 {
    let d = map_point(m.location[0] as f32, m.location[1] as f32) - map_point(hero.0 as f32, hero.1 as f32);
    CENTER + turn(d, ROTATION) + Vec2::new(MARKER_SHIFT_PX * px, 0.0)
}

/// Lado del icono de una marca en px reales.
fn marker_px(m: &Marker) -> f32 {
    if m.kind == KIND_PORTAL { PORTAL_PX } else { NPC_PX }
}

/// La caja de `SetBtnPos` (x, y, ancho, alto): desde la primera esquina del icono girado
/// (`p4[0]`, arriba a la izquierda antes de girar).
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

/// `Check_Btn`: dentro estricto, con el ratón en unidades enteras (`MouseX`, `MouseY`).
fn in_box(c: Vec2, b: [f32; 4]) -> bool {
    let (x, y) = (c.x.floor(), c.y.floor());
    x > b[0] && x < b[0] + b[2] && y > b[1] && y < b[1] + b[3]
}

/// Se dibuja la marca (NewUIMiniMap.cpp:164-171): en Crywolf ocupado, de los NPC solo el de (228, 48).
fn marker_drawn(m: &Marker, world: u16, crywolf_occupied: bool) -> bool {
    match m.kind {
        KIND_NPC => !(world == CRYWOLF && crywolf_occupied) || (m.location == [228, 48] && world == CRYWOLF),
        KIND_PORTAL => true,
        _ => false,
    }
}

// --- Dibujo ------------------------------------------------------------------------------------

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

/// Una pieza en la pantalla de 640×480.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderColor` negro.
    Shade { x: f32, y: f32, w: f32, h: f32, alpha: f32 },
    /// Imagen de `w`×`h` centrada en `c`, con el trozo `src` de la textura (px; `None` = entera),
    /// volteada y girada `turn` grados antihorarios.
    Image { tex: Tex, c: Vec2, w: f32, h: f32, src: Option<Rect>, flip: (bool, bool), turn: f32 },
    /// Ayuda: caja negra de alfa 180 con el texto en blanco; `x` es el centro del texto o, con
    /// `right`, la caja va pegada al borde derecho.
    Tip { x: f32, y: f32, right: bool, text: String },
}

fn image(tex: Tex, x: f32, y: f32, w: f32, h: f32, src: Rect, flip: (bool, bool)) -> Piece {
    Piece::Image { tex, c: Vec2::new(x + w / 2.0, y + h / 2.0), w, h, src: Some(src), flip, turn: 0.0 }
}

/// Lo que se dibuja, de atrás a delante. `hero` es la casilla del personaje y `px` las unidades
/// de 640×480 por px real.
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

    // Marcas: 17.5/32 de la textura de 18×18 ampliada a 32×32.
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
        // Las que no se dibujan se quedan sin caja (en el original, la del fotograma anterior).
        boxes.push(drawn.then(|| marker_box(c, m, px)));
    }

    // La flecha del personaje, sin girar.
    out.push(image(Tex::Hero, 325.0, 230.0, 12.0, 12.0, icon, (false, false)));

    // Marco (`uvxy` = 41.7/64 de la textura de 42 de ancho ampliada a 64; `Ui_wid` 35, `Ui_Hig` 6).
    let (wid, hig) = (35.0, 6.0);
    let line = Rect::new(0.0, 0.0, 41.7, 8.0);
    for i in 0..25 {
        let x = i as f32 * wid;
        out.push(image(Tex::Line, x, 0.0, wid, hig, line, (false, true)));
        out.push(image(Tex::Line, x, AREA_H - hig, wid, hig, line, (false, false)));
    }
    for i in 0..20 {
        // `RenderBitmapRotate`: (x, y) es el centro; −90° a la izquierda y 90° a la derecha.
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

    // La X: 36×29 de arriba de la textura de 36×58 (ampliada a 64×64). Apretada (`m_CurImgState`
    // 1), la V empieza en 25 y se sale de la textura: el borde de relleno, vacío.
    let [bx, by, bw, bh] = CLOSE;
    if !mm.close_down {
        out.push(image(Tex::Cancel, bx, by, bw, bh, Rect::new(0.0, 0.0, 36.0, 29.0), (false, false)));
    }
    if cursor.is_some_and(|c| in_rect(c, CLOSE)) {
        // `m_IsTopPos`: encima del botón; más ancha que la pantalla, se arrima al borde.
        out.push(Piece::Tip { x: 0.0, y: by - (FONT + 2.0), right: true, text: CLOSE_TEXT.into() });
    }

    // `Check_Btn`: la primera marca con el ratón dentro.
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
    // La pantalla de 640×480 como la barra, recortada a ella y por debajo de la barra (capa 8.1).
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

/// Rehace el minimapa cuando cambia algo de lo que se dibuja.
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
    // Unidades de 640×480 por px real de la ventana (sin ventana, la de 1024×768 del original).
    let px = window.map_or(640.0 / 1024.0, |w| 1.0 / (scale.0 * w.scale_factor()).max(1e-3));
    let hero = players.iter().next().map(|p| (p.pos.x.floor() as i32, p.pos.y.floor() as i32));
    let pieces = match hero {
        // `Render` devuelve en seguida sin `m_bSuccess`.
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
                    // Bevy gira en sentido horario: el antihorario del original va en negativo.
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
                    // Caja de ancho del texto + 6 que empieza en `centro − texto/2`: su centro
                    // queda 3 a la derecha del de la caja de la marca.
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
            // Sin el desplazamiento de 25 px, una marca en la casilla del personaje también.
            assert!(close(marker_center(hero, &marker(1, hero.0, hero.1, 0), 0.0), CENTER));
        }
    }

    #[test]
    fn giro_de_45_antihorario() {
        let hero = (128, 128);
        // Una casilla más de Y es +U: sin girar iría a la derecha; girado 45° antihorario, a la
        // derecha y hacia arriba.
        let d = marker_center(hero, &marker(1, 128, 160, 0), 0.0) - CENTER;
        let step = 32.0 / 256.0 * LENGTH;
        assert!(close(d, Vec2::new(step, -step) * std::f32::consts::FRAC_1_SQRT_2));
        // Una más de X es +V: sin girar, abajo; girado, abajo a la derecha.
        let d = marker_center(hero, &marker(1, 160, 128, 0), 0.0) - CENTER;
        assert!(close(d, Vec2::new(step, step) * std::f32::consts::FRAC_1_SQRT_2));
        // La marca y el mapa coinciden: el punto de la textura de esa casilla (U = Y, V = X) cae
        // en el mismo sitio.
        assert!(close(map_uv_to_screen(hero, 128.0 / 256.0, 160.0 / 256.0), CENTER + d));
    }

    #[test]
    fn el_giro_coincide_con_el_de_bevy() {
        // `UiTransform` gira en sentido horario: `Rot2::degrees(-t)` debe dar lo mismo que `turn`.
        let v = Vec2::new(3.0, -7.0);
        for t in [0.0, 45.0, 90.0, 135.0, 270.0] {
            assert!(close(Mat2::from(Rot2::degrees(-t)) * v, turn(v, t)), "{t}");
        }
    }

    #[test]
    fn cajas_de_las_marcas() {
        let hero = (100, 100);
        let px = 640.0 / 1024.0;
        // NPC sin girar: 7.5×7.5 desde la esquina de arriba a la izquierda del icono de 15 px.
        let m = marker(KIND_NPC, 100, 100, 0);
        let c = marker_center(hero, &m, px);
        assert!(close(c, CENTER + Vec2::new(25.0 * px, 0.0)));
        let b = marker_box(c, &m, px);
        assert!(close(Vec2::new(b[0], b[1]), c - Vec2::splat(7.5 * px)));
        assert_eq!((b[2], b[3]), (7.5, 7.5));
        // Portal girado 90°: la primera esquina pasa a abajo a la izquierda; caja de 30 centrada en ella.
        let m = marker(KIND_PORTAL, 100, 100, 90);
        let b = marker_box(c, &m, px);
        let corner = c + Vec2::new(-15.0 * px, 15.0 * px);
        assert!(close(Vec2::new(b[0] + 15.0, b[1] + 15.0), corner));
        assert_eq!((b[2], b[3]), (30.0, 30.0));
        // `Check_Btn` es estricto y con el ratón en enteros.
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
