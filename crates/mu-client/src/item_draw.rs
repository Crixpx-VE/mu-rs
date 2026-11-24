//! Materials of an item's draw calls ([`mu_data::items::glow::item_passes`]: its own drawing, then the
//! glow), resolved mesh by mesh like map objects. Shared by the window items (item_view.rs) and the
//! worn equipment (hero_dress.rs, character.rs).
//!
//! Each look is its own material on the same mesh, drawn in pass order (`depth_bias`, as `GL_LEQUAL`
//! does). Its color is the pass's `b->BodyLight` ([`PassLight`]), times `BlendMeshLight` on the
//! additive `BlendMesh`, in the material tint; with the light by the normal (characters) the light of
//! the place goes in `extension.light` like the rest of the body.

use bevy::math::Affine2;
use bevy::prelude::*;
use mu_data::items::glow::{ItemPass, PassLight};
use mu_data::objects::Glow;
use mu_data::objects::render::{self, Bitmap, LookBlend, LookColor, MeshInfo, MeshLook, ObjectDraw, Saw};

use crate::model_light::ModelMaterial;

/// The color of one look over time.
#[derive(Clone, Copy, Debug)]
struct PassColor {
    light: PassLight,
    /// `glColor3f` constant used instead of `BodyLight` (unlit `RENDER_TEXTURE` meshes).
    gl_color: Option<[f32; 3]>,
    /// `BlendMeshLight` (the `BlendMesh` branch only).
    glow: Option<Glow>,
    /// `BodyLight *= Alpha` of translucent bright chrome.
    scale: f32,
}

impl PassColor {
    fn at(self, world_time: f32) -> [f32; 3] {
        let mut c = self.gl_color.unwrap_or_else(|| self.light.at(world_time));
        if let Some(g) = self.glow {
            // `Glow` counts time in seconds.
            let g = g.at(world_time / 1000.0);
            c = c.map(|v| v * g);
        }
        c.map(|v| v * self.scale)
    }

    fn animated(self) -> bool {
        (self.gl_color.is_none() && self.light.animated()) || self.glow.is_some_and(Glow::animated)
    }
}

/// A material whose color or texture coordinates change over time.
#[derive(Clone, Debug)]
pub struct PassAnim {
    handle: Handle<ModelMaterial>,
    color: Option<PassColor>,
    wave: Option<[Saw; 2]>,
}

impl PassAnim {
    /// Brings the material to `world_time` (ms).
    pub fn update(&self, materials: &mut Assets<ModelMaterial>, world_time: f32) {
        let Some(mut m) = materials.get_mut(&self.handle) else { return };
        if let Some(color) = self.color {
            let c = color.at(world_time);
            m.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
        }
        if let Some([u, v]) = self.wave {
            m.base.uv_transform = Affine2::from_translation(Vec2::new(u.at(world_time), v.at(world_time)));
        }
    }
}

/// One look of a mesh.
pub struct MeshPass {
    pub handle: Handle<ModelMaterial>,
    pub anim: Option<PassAnim>,
    /// Lit by the normal with a light relative to the caller's: it takes the light of the place
    /// (`BodyMaterials`).
    pub follows_light: bool,
}

/// The looks of mesh `info` in `passes`. `own` is the mesh's texture; `unlit` = `o->LightEnable` off
/// (windows: the normal light becomes the flat `BodyLight`, ZzzInventory.cpp:10585); `bitmap` loads the
/// global textures.
#[allow(clippy::too_many_arguments)]
pub fn mesh_passes(
    passes: &[ItemPass],
    draw: &ObjectDraw,
    info: &MeshInfo,
    own: Option<&Handle<Image>>,
    unlit: bool,
    mut bitmap: impl FnMut(Bitmap) -> Option<Handle<Image>>,
    materials: &mut Assets<ModelMaterial>,
    world_time: f32,
) -> Vec<MeshPass> {
    let only: Vec<render::Pass> = passes.iter().map(|p| p.pass).collect();
    let mut out = Vec::new();
    for (k, look) in render::resolve_all(&only, draw, info) {
        let ip = passes[k];
        let lit = look.color == LookColor::Lit && !unlit;
        let color = PassColor {
            light: ip.light,
            gl_color: ip.gl_color.filter(|_| unlit && look.color == LookColor::Lit),
            // `o->BlendMeshLight` is 1 for items (`ItemObjectAttribute`, ZzzObject.cpp:5302).
            glow: (look.color == LookColor::BlendLight).then(|| ip.pass.glow.unwrap_or(Glow::Steady(1.0))),
            scale: look.body_scale,
        };
        let look = if unlit && look.color == LookColor::Lit { MeshLook { color: LookColor::Flat, ..look } } else { look };
        let image = match (look.textured, look.texture) {
            (false, _) => None,
            (true, Some(b)) => bitmap(b),
            (true, None) => own.cloned(),
        };
        let c = color.at(world_time);
        // The `BlendMeshLight` is already in `c`.
        let mut mat = crate::objects::object_material(image, &look, if lit { [1.0; 3] } else { c }, None, k, false);
        if lit {
            mat.extension.params.tint = Vec4::new(c[0], c[1], c[2], 1.0);
        }
        // `glColor4f(..., Alpha)` with `EnableAlphaTest`: blends with what is behind (as objects.rs).
        if look.alpha < 0.99 && look.blend == LookBlend::AlphaTest {
            mat.base.base_color.set_alpha(look.alpha);
            mat.base.alpha_mode = AlphaMode::Blend;
        }
        if let Some([u, v]) = look.wave {
            mat.base.uv_transform = Affine2::from_translation(Vec2::new(u.at(world_time), v.at(world_time)));
        }
        let handle = materials.add(mat);
        let animated = color.animated();
        let anim = (animated || look.wave.is_some()).then(|| PassAnim {
            handle: handle.clone(),
            color: animated.then_some(color),
            wave: look.wave,
        });
        out.push(MeshPass { handle, anim, follows_light: lit && color.gl_color.is_none() && ip.light.relative() });
    }
    out
}
