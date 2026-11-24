//! Equipment worn by the characters, with its glow: the hero's from the inventory, the others'
//! (character select, other players) from their appearance ([`dress_from_appearance`]).
//!
//! - **Pieces**: equipped helm, armor, pants, gloves and boots replace the class piece
//!   with the item's model (`BodyPart[BODYPART_*].Type = MODEL_HELM + index`...), posed with the
//!   same skeleton; the weapons (`Weapon[0]`, `Weapon[1]`) and the wings (`Wing`) use the item's,
//!   linked to a bone (character.rs). The skin meshes of
//!   those pieces (`BITMAP_SKIN`, the helm's face...) get the class skin (`BITMAP_SKIN + Skin`,
//!   ZzzBMD.cpp:949-953), i.e. the skin texture of the class piece; `hid*` meshes are not drawn.
//! - **Glow**: the `RenderPartObjectEffect` passes ([`item_passes`]) with the hero's light
//!   (`AbilityLight` = (1, 1, 1), character.rs): the normal pass goes in the piece's tint and the rest
//!   are extra entities with the same mesh (posed by the usual animation) and their own material.
//! - **When**: when the equipment changes (`Inventory::equipment_rev`) or the effects limit does; also
//!   when the character appears on a map.
//!
//! Not included: pets and mounts, the cloth capes, the wings' particles, nor the +15 ornaments
//! (`NextGradeObjectRender`).

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::items::glow::{GlowLight, item_passes};
use mu_data::items::{Item, slot};
use mu_data::objects::render::{self, Bitmap, LookColor, MeshInfo, ObjectDraw, Pass, Saw};
use mu_protocol::appearance::{Appearance, VisibleItem, slot as look_slot};

use crate::character::{BodyMaterials, Dress, Gear, Player, load_character_dressed};
use crate::inventory::Inventory;
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
pub struct EquipmentGlow(pub Vec<(Handle<ModelMaterial>, GlowLight)>);

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

/// Glow passes of the equipment meshes (for `load_character_dressed`).
#[derive(Default)]
pub struct GlowBuilder {
    bitmaps: HashMap<Bitmap, Option<Handle<Image>>>,
    pub animated: Vec<(Handle<ModelMaterial>, GlowLight)>,
}

impl GlowBuilder {
    /// Puts the normal pass's light in `base`'s tint and returns the mesh's other passes.
    #[allow(clippy::too_many_arguments)]
    pub fn passes(
        &mut self,
        data: &Path,
        mesh: &Handle<Mesh>,
        m: &bmd::Mesh,
        item: &Item,
        render_level: u8,
        base: &Handle<ModelMaterial>,
        materials: &mut Assets<ModelMaterial>,
        images: &mut Assets<Image>,
    ) -> Vec<(Mesh3d, MeshMaterial3d<ModelMaterial>)> {
        let passes = item_passes(item, [1.0; 3], render_level);
        let (own, has_alpha) = materials
            .get(base)
            .map_or((None, false), |b| (b.base.base_color_texture.clone(), !matches!(b.base.alpha_mode, AlphaMode::Opaque)));
        if let Some(p0) = passes.first() {
            let c = p0.light.at(0.0);
            if let Some(mut b) = materials.get_mut(base) {
                b.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
            }
            if p0.light.animated() {
                self.animated.push((base.clone(), p0.light));
            }
        }
        let info = MeshInfo { index: 0, texture_index: m.texture_index as i32, script: bmd::texture_script(&m.texture), has_alpha };
        let draw = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        let mut out = Vec::new();
        for (k, p) in passes.iter().enumerate().skip(1) {
            let Some(look) = render::resolve(&Pass::body(p.flags), &draw, &info) else { continue };
            let image = match (look.textured, look.texture) {
                (false, _) => None,
                (true, Some(b)) => self.bitmaps.entry(b).or_insert_with(|| crate::objects::load_bitmap(data, b, images)).clone(),
                (true, None) => own.clone(),
            };
            let c = p.light.at(0.0);
            // Lit (`LightTransform`): the hero's light by the normal and the pass color in the tint.
            let lit = look.color == LookColor::Lit;
            let mut mat = crate::objects::object_material(image, &look, if lit { [1.0; 3] } else { c }, None, k, false);
            mat.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
            let h = materials.add(mat);
            if p.light.animated() {
                self.animated.push((h.clone(), p.light));
            }
            out.push((Mesh3d(mesh.clone()), MeshMaterial3d(h)));
        }
        out
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
    }
}

/// What changes the stand and walk animations of a character dressed from its appearance.
pub fn gear_from_appearance(look: &Appearance, data: &ItemData) -> Gear {
    let weapons = [look_slot::WEAPON_RIGHT, look_slot::WEAPON_LEFT].map(|s| look.equipment[s].map(|v| appearance_item(&v)));
    gear(weapons, look.equipment[look_slot::WING].is_some(), data)
}

/// The same for the hero, from its inventory (`SetCharacterClass`, ZzzCharacter.cpp:11814-11841).
fn gear_from_inventory(inv: &Inventory, data: &ItemData) -> Gear {
    let weapons = [slot::WEAPON_RIGHT, slot::WEAPON_LEFT].map(|s| inv.equipment[s as usize]);
    gear(weapons, inv.equipment[slot::WING as usize].is_some(), data)
}

/// `ItemAttribute[Weapon[0].Type].TwoHand` is the only attribute `SetPlayerStop` / `SetPlayerWalk` read.
fn gear(weapons: [Option<Item>; 2], wings: bool, data: &ItemData) -> Gear {
    let two_hand = weapons[0].and_then(|w| data.attr(&w)).is_some_and(|a| a.two_hand);
    Gear { weapons, two_hand, wings }
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
    mut last: Local<Option<(u32, u8)>>,
) {
    let Ok((e, mut p, children)) = players.single_mut() else {
        // No character (loading another map): it gets dressed again when it reappears.
        *last = None;
        return;
    };
    let key = (inv.equipment_rev, opts.render_level);
    if *last == Some(key) {
        return;
    }
    let first = last.is_none();
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

/// Color of the pulsing passes (excellent, +3 to +6, ancient).
fn animate_glow(time: Res<Time<Real>>, glows: Query<&EquipmentGlow>, mut materials: ResMut<Assets<ModelMaterial>>) {
    let t = time.elapsed_secs() * 1000.0;
    for g in &glows {
        for (h, light) in &g.0 {
            if let Some(mut m) = materials.get_mut(h) {
                let c = light.at(t);
                m.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
            }
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
