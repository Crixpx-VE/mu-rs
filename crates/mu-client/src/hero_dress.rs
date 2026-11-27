//! Equipment worn by the characters, with its glow: the hero's from the inventory, the others'
//! (character select, other players) from their appearance ([`dress_from_appearance`]).
//!
//! - **Pieces**: equipped helm, armor, pants, gloves and boots replace the class piece
//!   with the item's model (`BodyPart[BODYPART_*].Type = MODEL_HELM + index`...), posed with the
//!   same skeleton; the weapons (`Weapon[0]`, `Weapon[1]`) and the wings (`Wing`) use the item's,
//!   linked to a bone (character.rs). The skin meshes of
//!   those pieces (`BITMAP_SKIN`, the helm's face...) get the class skin (`BITMAP_SKIN + Skin`,
//!   ZzzBMD.cpp:949-953), i.e. the skin texture of the class piece; `hid*` meshes are not drawn.
//! - **Passes**: the `RenderPartObjectEffect` calls ([`item_passes`]: the item's own drawing, then the
//!   glow) with the hero's light (`AbilityLight` = (1, 1, 1), character.rs): one entity per look, with
//!   the same mesh (posed by the usual animation) and its own material (item_draw.rs).
//! - **When**: when the equipment changes (`Inventory::equipment_rev`) or the effects limit does; also
//!   when the character appears on a map.
//!
//! Not included: pets and mounts, the cloth capes, the wings' particles, nor the +15 ornaments
//! (`NextGradeObjectRender`).

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::items::glow::{ItemCtx, item_passes};
use mu_data::items::{Item, slot};
use mu_data::objects::render::{Bitmap, MeshInfo, ObjectDraw};
use mu_protocol::appearance::{Appearance, VisibleItem, slot as look_slot};

use crate::character::{BodyMaterials, Dress, Gear, Player, load_character_dressed};
use crate::inventory::Inventory;
use crate::item_draw::{self, PassAnim};
use crate::item_view::ItemData;
use crate::model_light::ModelMaterial;
use crate::scene::GameScene;

pub struct HeroDressPlugin;

impl Plugin for HeroDressPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (redress, animate_glow).chain().run_if(in_state(GameScene::Game)))
            .add_systems(Update, animate_glow.run_if(in_state(GameScene::CharacterSelect)));
    }
}

/// Equipment glow materials whose color changes over time (the hero, other players and the
/// characters in character select).
#[derive(Component)]
pub struct EquipmentGlow(pub Vec<PassAnim>);

/// The texture of one mesh of a piece: its own, the class skin for skin meshes of equipment
/// pieces, or none (`hid*`, `BITMAP_HIDE`). Class pieces are left as they are.
pub fn part_texture(name: &str, equipped: bool, class_part: &Bmd) -> Option<String> {
    if !equipped {
        return Some(name.to_string());
    }
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    if file.starts_with("hid") {
        return None;
    }
    let skin = |f: &str| f.starts_with("ski") || f.to_ascii_lowercase().starts_with("level");
    if skin(file) {
        let own = class_part.meshes.iter().map(|m| m.texture.as_str()).find(|t| skin(t.rsplit(['\\', '/']).next().unwrap_or(t)));
        return Some(own.unwrap_or(name).to_string());
    }
    Some(name.to_string())
}

/// Draw calls of the equipment meshes (for `load_character_dressed`).
#[derive(Default)]
pub struct GlowBuilder {
    bitmaps: HashMap<Bitmap, Option<Handle<Image>>>,
    pub animated: Vec<PassAnim>,
}

/// One of an equipment mesh's looks, and whether it takes the light of the place (`BodyMaterials`).
pub type EquipmentPass = (Mesh3d, MeshMaterial3d<ModelMaterial>, bool);

impl GlowBuilder {
    /// Every look of mesh `index` (of `num_meshes`) of `item`: its own drawing
    /// (`RenderPartObjectBody`) and the glow, on top of each other. `base` is the mesh's normal
    /// material, which gives its texture; `draw` is the item's `o` (`ItemObjectAttribute`).
    #[allow(clippy::too_many_arguments)]
    pub fn passes(
        &mut self,
        data: &Path,
        mesh: &Handle<Mesh>,
        m: &bmd::Mesh,
        (index, num_meshes): (usize, usize),
        (item, render_level, white_light): (&Item, u8, bool),
        draw: &ObjectDraw,
        base: &Handle<ModelMaterial>,
        materials: &mut Assets<ModelMaterial>,
        images: &mut Assets<Image>,
    ) -> Vec<EquipmentPass> {
        // `b->HideSkin` is off for characters (ZzzCharacter.cpp:9276).
        let ctx = ItemCtx { hide_skin: false, num_meshes, white_light };
        let passes = item_passes(item, [1.0; 3], render_level, &ctx);
        let (own, has_alpha) = materials
            .get(base)
            .map_or((None, false), |b| (b.base.base_color_texture.clone(), !matches!(b.base.alpha_mode, AlphaMode::Opaque)));
        let info = MeshInfo { index, texture_index: m.texture_index as i32, script: bmd::texture_script(&m.texture), has_alpha };
        let bitmap = |b| self.bitmaps.entry(b).or_insert_with(|| crate::file_cache::bitmap(data, b, images)).clone();
        let looks = item_draw::mesh_passes(&passes, draw, &info, own.as_ref(), false, bitmap, materials, 0.0);
        looks
            .into_iter()
            .map(|mp| {
                self.animated.extend(mp.anim);
                (Mesh3d(mesh.clone()), MeshMaterial3d(mp.handle), mp.follows_light)
            })
            .collect()
    }
}

/// The equipment shown on the character (`None` = nothing worn).
fn dress_from(inv: &Inventory, data: &ItemData, render_level: u8) -> Option<Dress> {
    let piece = |s: u8| inv.equipment[s as usize].and_then(|item| Some((data.model_path(&item)?, item)));
    let dress = Dress {
        parts: [piece(slot::HELM), piece(slot::ARMOR), piece(slot::PANTS), piece(slot::GLOVES), piece(slot::BOOTS)],
        weapons: [piece(slot::WEAPON_RIGHT), piece(slot::WEAPON_LEFT)],
        wing: piece(slot::WING),
        render_level,
        // The hero's `c->Light` is `AbilityLight` = (1, 1, 1) (character.rs, `light_body`).
        white_light: true,
        ..Dress::default()
    };
    dress.any().then_some(dress)
}

/// The item a visible piece of an appearance stands for: `Type`, `Level` (already `LevelConvert`ed),
/// `Option1` (excellent) and `ExtOption` (ancient), as `RenderPartObjectEffect` gets them.
pub fn appearance_item(v: &VisibleItem) -> Item {
    Item { group: v.group, index: v.index, level: v.level, excellent: v.excellent, ancient: v.ancient, ..Item::default() }
}

/// What another character (character select, other players) shows, from its appearance
/// (`ChangeCharacterExt` / `ReceiveChangePlayer`). TODO: pets and mounts (`Helper`, `CreateBug`)
/// are not drawn yet.
pub fn dress_from_appearance(look: &Appearance, data: &ItemData, render_level: u8) -> Dress {
    let piece = |s: usize| look.equipment[s].map(|v| appearance_item(&v)).and_then(|item| Some((data.model_path(&item)?, item)));
    Dress {
        parts: [piece(look_slot::HELM), piece(look_slot::ARMOR), piece(look_slot::PANTS), piece(look_slot::GLOVES), piece(look_slot::BOOTS)],
        weapons: [piece(look_slot::WEAPON_RIGHT), piece(look_slot::WEAPON_LEFT)],
        wing: piece(look_slot::WING),
        render_level,
        // Other characters are lit by the place (remote.rs, charselect.rs), not by (1, 1, 1).
        white_light: false,
        ..Dress::default()
    }
}

/// What changes the stand and walk animations of a character dressed from its appearance.
pub fn gear_from_appearance(look: &Appearance, data: &ItemData) -> Gear {
    let item = |s: usize| look.equipment[s].map(|v| appearance_item(&v));
    let weapons = [look_slot::WEAPON_RIGHT, look_slot::WEAPON_LEFT].map(item);
    let wing = item(look_slot::WING);
    Gear {
        wing: wing.map(|w| (w.group, w.index)),
        gloves_level: item(look_slot::GLOVES).map(|i| i.level),
        boots_level: item(look_slot::BOOTS).map(|i| i.level),
        ..gear(weapons, wing.is_some(), data)
    }
}

/// The same for the hero, from its inventory (`SetCharacterClass`, ZzzCharacter.cpp:11814-11841).
fn gear_from_inventory(inv: &Inventory, data: &ItemData) -> Gear {
    let item = |s: u8| inv.equipment[s as usize];
    let weapons = [slot::WEAPON_RIGHT, slot::WEAPON_LEFT].map(item);
    let wing = item(slot::WING);
    Gear {
        wing: wing.map(|w| (w.group, w.index)),
        gloves_level: item(slot::GLOVES).map(|i| i.level),
        boots_level: item(slot::BOOTS).map(|i| i.level),
        ..gear(weapons, wing.is_some(), data)
    }
}

/// `ItemAttribute[Weapon[0].Type].TwoHand` is the only attribute `SetPlayerStop` / `SetPlayerWalk` read.
pub fn gear(weapons: [Option<Item>; 2], wings: bool, data: &ItemData) -> Gear {
    let two_hand = weapons[0].and_then(|w| data.attr(&w)).is_some_and(|a| a.two_hand);
    Gear { weapons, two_hand, wings, ..Gear::default() }
}

#[allow(clippy::too_many_arguments)]
fn redress(
    mut commands: Commands,
    args: Res<crate::Args>,
    inv: Res<Inventory>,
    data: Res<ItemData>,
    opts: Res<crate::options::GameOptions>,
    mut players: Query<(Entity, &mut Player, Option<&Children>)>,
    mesh_children: Query<(), With<Mesh3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Option<(Entity, u32, u8)>>,
) {
    let Ok((e, mut p, children)) = players.single_mut() else {
        return;
    };
    // The entity is part of the key: a map change respawns the hero (`Step::Player`) while this
    // system is paused (it only runs in `GameScene::Game`), so the new one must be dressed again
    // even though the equipment did not change.
    let key = (e, inv.equipment_rev, opts.render_level);
    if *last == Some(key) {
        return;
    }
    let first = last.is_none_or(|(prev, ..)| prev != e);
    *last = Some(key);
    p.gear = gear_from_inventory(&inv, &data);
    let dress = dress_from(&inv, &data, opts.render_level);
    // Just spawned with nothing worn: already as `spawn_player` loaded it.
    if first && dress.is_none() {
        return;
    }
    let Some(model) = load_character_dressed(&args, p.class, p.action, dress.as_ref(), &mut meshes, &mut materials, &mut images) else {
        return;
    };
    for c in children.into_iter().flatten() {
        if mesh_children.contains(*c) {
            commands.entity(*c).despawn();
        }
    }
    commands.entity(e).insert((model.skinned, BodyMaterials(model.materials), EquipmentGlow(model.glow)));
    for child in model.children {
        commands.entity(e).with_child(child);
    }
}

/// Color and texture scroll of the varying passes (excellent, +3 to +6, ancient, the items' own).
fn animate_glow(time: Res<Time<Real>>, glows: Query<&EquipmentGlow>, mut materials: ResMut<Assets<ModelMaterial>>) {
    let t = time.elapsed_secs() * 1000.0;
    for g in &glows {
        for a in &g.0 {
            a.update(&mut materials, t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texturas_de_las_piezas() {
        let mesh = mu_data::bmd::Mesh {
            vertices: vec![],
            normals: vec![],
            uvs: vec![],
            triangles: vec![],
            texture_index: 0,
            texture: "skin_warrior_01.jpg".into(),
        };
        let class = Bmd { version: 10, name: String::new(), meshes: vec![mesh], actions: vec![], bones: vec![] };
        assert_eq!(part_texture("hide.jpg", true, &class), None);
        assert_eq!(part_texture("skin_barbarian_01.jpg", true, &class).as_deref(), Some("skin_warrior_01.jpg"));
        assert_eq!(part_texture("upper_03_m.jpg", true, &class).as_deref(), Some("upper_03_m.jpg"));
        // Class pieces do not change.
        assert_eq!(part_texture("hide.jpg", false, &class).as_deref(), Some("hide.jpg"));
    }
}
