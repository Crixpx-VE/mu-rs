//! Cursor del ratón de MU (`RenderCursor`, ZzzInterface.cpp:8718-8829).
//!
//! El original esconde el del sistema (`ShowCursor(FALSE)`, Winmain.cpp:1389) y dibuja el suyo
//! encima de todo: `Interface/Cursor.tga` de 24×24 unidades de la pantalla de 640×480, con la
//! punta 2 unidades a la izquierda y arriba del ratón (`MouseX − 2, MouseY − 2`). Con el botón
//! izquierdo apretado se ve `CursorPush.tga` (la última rama de `RenderCursor`).
//!
//! - Sobre un objeto de sentarse (`SelectedOperate`): `CursorSitDown`, o `CursorLeanAgainst` en las
//!   cajas de pose de Lorencia, Dungeon y Devias y en la fuente de Noria.
//! - Con el botón apretado, si el último clic en el suelo fue a una casilla sin suelo o de agua
//!   (`DontMove`): `CursorDontMove`.
//!
//! Las ramas de objeto del suelo, NPC, monstruo atacable y reparar llegarán con esas cosas.
//! El original solo lo dibuja en el login, la selección de personaje y el juego; aquí también en
//! el título y la carga para que no se quede la pantalla sin cursor.
//!
//! Se coloca cada fotograma en `PostUpdate`, antes de la maquetación de la interfaz, con la
//! posición del ratón de ese mismo fotograma: no va con un fotograma de retraso.

use bevy::prelude::*;
use bevy::ui::{UiScale, UiSystems};
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::Args;

pub struct CursorPlugin;

impl Plugin for CursorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_cursor)
            .add_systems(PostUpdate, (hide_system_cursor, place_cursor).before(UiSystems::Prepare));
    }
}

/// `RenderBitmap(BITMAP_CURSOR, MouseX − 2, MouseY − 2, 24, 24)`.
const SIZE: f32 = 24.0;
const OFFSET: f32 = 2.0;

/// Texturas del cursor (`ZzzOpenData.cpp:4963-4964`).
#[derive(Resource)]
struct CursorImages {
    /// `BITMAP_CURSOR`.
    normal: Handle<Image>,
    /// `BITMAP_CURSOR+1`: con el botón izquierdo apretado.
    push: Handle<Image>,
    /// `BITMAP_CURSOR+6`.
    lean: Handle<Image>,
    /// `BITMAP_CURSOR+7`.
    sit: Handle<Image>,
    /// `BITMAP_CURSOR+8`.
    dont_move: Handle<Image>,
}

#[derive(Component)]
struct MuCursor;

fn spawn_cursor(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let mut load = |n: &str| crate::scene::interface_image(&args, &mut images, n).map(|i| i.0).unwrap_or_default();
    let normal = load("Cursor.tga");
    let push = load("CursorPush.tga");
    commands.spawn((
        Name::new("Cursor"),
        MuCursor,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(SIZE),
            height: Val::Px(SIZE),
            ..default()
        },
        ImageNode::new(normal.clone()),
        // Por encima de toda la interfaz (el título y la carga van en 100).
        GlobalZIndex(i32::MAX),
        Visibility::Hidden,
    ));
    let lean = load("CursorLeanAgainst.tga");
    let sit = load("CursorSitDown.tga");
    let dont_move = load("CursorDontMove.tga");
    commands.insert_resource(CursorImages { normal, push, lean, sit, dont_move });
}

/// El del sistema no se ve dentro de la ventana.
fn hide_system_cursor(mut q: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    for mut c in &mut q {
        if c.visible {
            c.visible = false;
        }
    }
}

fn place_cursor(
    windows: Query<&Window, With<PrimaryWindow>>,
    scale: Res<UiScale>,
    mouse: Res<ButtonInput<MouseButton>>,
    images: Option<Res<CursorImages>>,
    pointer: Res<crate::operate::Pointer>,
    operates: Res<crate::operate::Operates>,
    mut q: Query<(&mut Node, &mut ImageNode, &mut Visibility), With<MuCursor>>,
) {
    let Ok((mut node, mut img, mut vis)) = q.single_mut() else { return };
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let Some(c) = cursor else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    vis.set_if_neq(Visibility::Inherited);
    // Píxeles de la ventana → unidades de la interfaz (1 unidad = 1 de la pantalla de 480 de alto).
    let p = c / scale.0.max(1e-3) - Vec2::splat(OFFSET);
    let (left, top) = (Val::Px(p.x), Val::Px(p.y));
    if node.left != left || node.top != top {
        node.left = left;
        node.top = top;
    }
    if let Some(images) = images {
        // Mismo orden que `RenderCursor`: objeto de sentarse; si no, CursorPush (o CursorDontMove)
        // mientras el botón izquierdo está apretado.
        let want = match pointer.operate_cursor(&operates) {
            Some(true) => &images.lean,
            Some(false) => &images.sit,
            None if !mouse.pressed(MouseButton::Left) => &images.normal,
            None if pointer.dont_move => &images.dont_move,
            None => &images.push,
        };
        if img.image != *want {
            img.image = want.clone();
        }
    }
}
