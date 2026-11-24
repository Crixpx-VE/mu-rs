//! MU mouse cursor (`RenderCursor`, ZzzInterface.cpp:8718-8829).
//!
//! The original hides the system cursor (`ShowCursor(FALSE)`, Winmain.cpp:1389) and draws its own
//! on top of everything: `Interface/Cursor.tga`, 24×24 units of the 640×480 screen, with the tip
//! 2 units left of and above the mouse (`MouseX − 2, MouseY − 2`). With the left button held it
//! shows `CursorPush.tga` (the last branch of `RenderCursor`).
//!
//! - Over a sitting object (`SelectedOperate`): `CursorSitDown`, or `CursorLeanAgainst` on the
//!   posing boxes of Lorencia, Dungeon and Devias and on the Noria fountain.
//! - With the button held, if the last ground click was on a tile without ground or with water
//!   (`DontMove`): `CursorDontMove`.
//!
//! The ground item, NPC, attackable monster and repair branches will come with those features.
//! The original only draws it in login, character selection and the game; here also on the
//! title and loading screens so the screen is never without a cursor.
//!
//! It is placed every frame in `PostUpdate`, before UI layout, using that same frame's mouse
//! position, so it does not lag one frame behind.

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

/// Cursor textures (`ZzzOpenData.cpp:4963-4964`).
#[derive(Resource)]
struct CursorImages {
    /// `BITMAP_CURSOR`.
    normal: Handle<Image>,
    /// `BITMAP_CURSOR+1`: with the left button held.
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
        // Above all UI (title and loading screens use 100).
        GlobalZIndex(i32::MAX),
        Visibility::Hidden,
    ));
    let lean = load("CursorLeanAgainst.tga");
    let sit = load("CursorSitDown.tga");
    let dont_move = load("CursorDontMove.tga");
    commands.insert_resource(CursorImages { normal, push, lean, sit, dont_move });
}

/// The system cursor is hidden inside the window.
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
    // Window pixels → UI units (1 unit = 1 pixel of the 480-high screen).
    let p = c / scale.0.max(1e-3) - Vec2::splat(OFFSET);
    let (left, top) = (Val::Px(p.x), Val::Px(p.y));
    if node.left != left || node.top != top {
        node.left = left;
        node.top = top;
    }
    if let Some(images) = images {
        // Same order as `RenderCursor`: sitting object; otherwise CursorPush (or CursorDontMove)
        // while the left button is held.
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
