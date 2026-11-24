//! MU mouse cursor (`RenderCursor`, ZzzInterface.cpp:8718-8829).
//!
//! The original hides the system cursor (`ShowCursor(FALSE)`, Winmain.cpp:1389) and draws its own
//! on top of everything: `Interface/Cursor.tga`, 24×24 units of the 640×480 screen, with the tip
//! 2 units left of and above the mouse (`MouseX − 2, MouseY − 2`). With the left button held it
//! shows `CursorPush.tga` (the last branch of `RenderCursor`).
//!
//! - Over an NPC (`SelectedNpc`): `CursorTalk.tga`, animated (see [`talk_uv`]).
//! - Over a sitting object (`SelectedOperate`): `CursorSitDown`, or `CursorLeanAgainst` on the
//!   posing boxes of Lorencia, Dungeon and Devias and on the Noria fountain.
//! - Outside the safe zone, over a monster or player that can be attacked (`CheckAttack`):
//!   `CursorAttack.tga`.
//! - With the button held, if the last ground click was on a tile without ground or with water
//!   (`DontMove`): `CursorDontMove`.
//!
//! The ground item and repair branches will come with those features. The original only draws it
//! in login, character selection and the game; here also on the title and loading screens so the
//! screen is never without a cursor.
//!
//! It is placed every frame in `PostUpdate`, before UI layout, using that same frame's mouse
//! position, so it does not lag one frame behind.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui::{UiScale, UiSystems};
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::Args;
use crate::names::{Kind, Selectable};

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

/// Cursor textures (`ZzzOpenData.cpp:4963-4971`).
#[derive(Resource)]
struct CursorImages {
    /// `BITMAP_CURSOR`.
    normal: Handle<Image>,
    /// `BITMAP_CURSOR+1`: with the left button held.
    push: Handle<Image>,
    /// `BITMAP_CURSOR+2` (`Interface/CursorAttack.tga`, ZzzOpenData.cpp:4965).
    attack: Handle<Image>,
    /// `BITMAP_CURSOR+4` (`Interface/CursorTalk.tga`, ZzzOpenData.cpp:4967) and its size in pixels:
    /// four 0.5 × 0.5 frames.
    talk: (Handle<Image>, Vec2),
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
        // Above the whole UI (the title and loading screens are at 100).
        GlobalZIndex(i32::MAX),
        Visibility::Hidden,
    ));
    let lean = load("CursorLeanAgainst.tga");
    let sit = load("CursorSitDown.tga");
    let dont_move = load("CursorDontMove.tga");
    let attack = load("CursorAttack.tga");
    let talk = crate::scene::interface_image(&args, &mut images, "CursorTalk.tga").unwrap_or_default();
    commands.insert_resource(CursorImages { normal, push, attack, talk, lean, sit, dont_move });
}

/// The cursors of `RenderCursor` handled here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Normal,
    Push,
    Attack,
    Talk,
    Lean,
    Sit,
    DontMove,
}

/// What is under the mouse, as `RenderCursor` looks at it.
#[derive(Clone, Copy, Default)]
struct Under {
    /// `SelectedNpc != -1`.
    npc: bool,
    /// `SelectedOperate != -1`: `Some(true)` with the lean cursor.
    operate: Option<bool>,
    /// `!Hero->SafeZone && SelectedCharacter != -1`: whether `CheckAttack() && !MouseOnWindow`.
    attack: Option<bool>,
    /// `MouseLButton`.
    pressed: bool,
    /// `DontMove`.
    dont_move: bool,
}

/// `RenderCursor` (ZzzInterface.cpp:8718-8829), in its order. Not here: `g_iKeyPadEnable` /
/// `ErrorMessage` (8728), the ground item `CursorGet` (8732-8735; no ground items yet), Kanturu 2's
/// NPC cursor and Battle Castle's attack one (`BITMAP_CURSOR2`, 8738-8741 and 8762-8765), the
/// command window cursors (8774-8788) and the repair ones (8789-8816).
fn choose(u: Under) -> Shape {
    if u.npc {
        Shape::Talk
    } else if let Some(lean) = u.operate {
        if lean { Shape::Lean } else { Shape::Sit }
    } else if let Some(attack) = u.attack {
        if attack { Shape::Attack } else { Shape::Normal }
    } else if !u.pressed {
        Shape::Normal
    } else if u.dont_move {
        Shape::DontMove
    } else {
        Shape::Push
    }
}

/// The talk cursor frame (ZzzInterface.cpp:8723-8727): `Frame = (int)(WorldTime*0.01f)%6`, `u` = 0.5 in
/// frames 1, 3 and 5 and `v` = 0.5 in 2, 3 and 4 (a 0, 1, 2, 3, 2, 1 loop over the four quarters, one
/// every 100 ms). `world_time` in ms.
fn talk_uv(world_time: f64) -> Vec2 {
    let frame = (world_time * 0.01) as i64 % 6;
    let u = if matches!(frame, 1 | 3 | 5) { 0.5 } else { 0.0 };
    let v = if matches!(frame, 2..=4) { 0.5 } else { 0.0 };
    Vec2::new(u, v)
}

/// `CheckAttack` (ZzzInterface.cpp:1586-1821) with what exists. Not yet: GMs (1598-1601), the gens
/// strife map (1609-1674), guild wars, soccer and the duel-watch buff (1678-1689), Battle Castle
/// (1695-1764), duels (1776-1790) and the Cursed Temple parties (1806-1809).
fn check_attack(target: &Selectable, world: u16, item_picked: bool, ctrl: bool) -> bool {
    use crate::names::{GR_RIVAL, GR_RIVALUNION, PVP_MURDERER2};
    if item_picked {
        return false;
    }
    // `c != Hero` always holds: the hero is never selected.
    if crate::names::in_chaos_castle(world) {
        return true;
    }
    match target.kind {
        Kind::Monster => true,
        Kind::Player => {
            let rival = target.guild.as_ref().is_some_and(|g| g.relation == GR_RIVAL || g.relation == GR_RIVALUNION);
            rival || target.pk >= PVP_MURDERER2 || ctrl
        }
        Kind::Npc => false,
    }
}

/// What the attack branch of `RenderCursor` needs.
#[derive(SystemParam)]
struct Aim<'w, 's> {
    hovered: Res<'w, crate::names::Hovered>,
    selectables: Query<'w, 's, &'static Selectable>,
    hero: Query<'w, 's, &'static crate::character::Player>,
    terrain: Option<Res<'w, crate::terrain::LoadedTerrain>>,
    args: Res<'w, Args>,
    inventory: Res<'w, crate::inventory::Inventory>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    over_ui: Res<'w, crate::mainframe::PointerOverUi>,
}

impl Aim<'_, '_> {
    /// `!Hero->SafeZone && SelectedCharacter != -1` → `Some(CheckAttack() && !MouseOnWindow)`.
    fn attack(&self) -> Option<bool> {
        let target = self.selectables.get(self.hovered.character?).ok()?;
        let p = self.hero.single().ok()?;
        let safe = self.terrain.as_ref().is_some_and(|t| {
            let (x, y) = (p.pos.x.max(0.0) as usize, p.pos.y.max(0.0) as usize);
            t.0.attribute(x, y) & mu_data::terrain::attr::SAFE_ZONE != 0
        });
        if safe {
            return None;
        }
        let ctrl = self.keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
        Some(check_attack(target, self.args.world, self.inventory.picked.is_some(), ctrl) && !self.over_ui.0)
    }
}

/// The system cursor is hidden inside the window.
fn hide_system_cursor(mut q: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    for mut c in &mut q {
        if c.visible {
            c.visible = false;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn place_cursor(
    windows: Query<&Window, With<PrimaryWindow>>,
    scale: Res<UiScale>,
    mouse: Res<ButtonInput<MouseButton>>,
    images: Option<Res<CursorImages>>,
    pointer: Res<crate::operate::Pointer>,
    operates: Res<crate::operate::Operates>,
    time: Res<Time>,
    aim: Aim,
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
        let under = Under {
            npc: aim.hovered.npc.is_some(),
            operate: pointer.operate_cursor(&operates),
            attack: aim.attack(),
            pressed: mouse.pressed(MouseButton::Left),
            dont_move: pointer.dont_move,
        };
        let shape = choose(under);
        let want = match shape {
            Shape::Normal => &images.normal,
            Shape::Push => &images.push,
            Shape::Attack => &images.attack,
            Shape::Talk => &images.talk.0,
            Shape::Lean => &images.lean,
            Shape::Sit => &images.sit,
            Shape::DontMove => &images.dont_move,
        };
        if img.image != *want {
            img.image = want.clone();
        }
        // `RenderBitmap(BITMAP_CURSOR+4, …, u, v, 0.5f, 0.5f)`: one quarter of the texture.
        let rect = (shape == Shape::Talk).then(|| {
            let size = images.talk.1;
            let uv = talk_uv(time.elapsed_secs_f64() * 1000.0);
            Rect::from_corners(uv * size, (uv + Vec2::splat(0.5)) * size)
        });
        if img.rect != rect {
            img.rect = rect;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_de_hablar_animado() {
        // Frames 0..5 every 100 ms: quarters (0,0) (½,0) (0,½) (½,½) (0,½) (½,0), then again.
        let quarters: Vec<Vec2> = (0..7).map(|f| talk_uv(f as f64 * 100.0 + 50.0)).collect();
        let (a, b, c, d) = (Vec2::ZERO, Vec2::new(0.5, 0.0), Vec2::new(0.0, 0.5), Vec2::splat(0.5));
        assert_eq!(quarters, vec![a, b, c, d, c, b, a]);
        assert_eq!(talk_uv(99.9), a);
    }

    #[test]
    fn orden_de_render_cursor() {
        let base = Under::default();
        assert_eq!(choose(base), Shape::Normal);
        assert_eq!(choose(Under { pressed: true, ..base }), Shape::Push);
        assert_eq!(choose(Under { pressed: true, dont_move: true, ..base }), Shape::DontMove);
        // The NPC goes before everything else, and the button does not change it.
        let all = Under { npc: true, operate: Some(true), attack: Some(true), pressed: true, dont_move: true };
        assert_eq!(choose(all), Shape::Talk);
        assert_eq!(choose(Under { npc: false, ..all }), Shape::Lean);
        assert_eq!(choose(Under { operate: Some(false), ..base }), Shape::Sit);
        assert_eq!(choose(Under { attack: Some(true), pressed: true, ..base }), Shape::Attack);
        // Over a character that cannot be attacked, the plain cursor even with the button down.
        assert_eq!(choose(Under { attack: Some(false), pressed: true, ..base }), Shape::Normal);
    }

    #[test]
    fn se_puede_atacar() {
        let mut monster = Selectable::player(String::new(), 3);
        monster.kind = Kind::Monster;
        let player = Selectable::player("otro".into(), 3);
        let lorencia = 1;
        assert!(check_attack(&monster, lorencia, false, false));
        assert!(!check_attack(&monster, lorencia, true, false), "with an item on the mouse");
        assert!(!check_attack(&player, lorencia, false, false));
        assert!(check_attack(&player, lorencia, false, true), "Ctrl");
        assert!(check_attack(&Selectable::player("pk".into(), 6), lorencia, false, false), "murderer");
        assert!(check_attack(&player, 19, false, false), "Chaos Castle");
    }
}
