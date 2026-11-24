//! Item glow: the `RenderPartObjectEffect` passes (ZzzObject.cpp:9327-10385) depending on
//! level (+3 to +15), excellent options and ancient sets.
//!
//! `RenderPartObjectEffect` sets `b->BodyLight` from the level ([`level_light`]), calls
//! `RenderPartObjectBody` (the item's own drawing, [`super::body`]) and then the glow calls
//! `RenderPartObjectBodyColor(2)` and the excellent / ancient ones ([`glow_passes`]), each with its
//! `RENDER_*` flags ([`crate::objects::render::flags`]) and its `b->BodyLight` ([`GlowLight`]).
//! [`item_passes`] puts them in that order as [`Pass`]es, which the drawer resolves mesh by mesh with
//! [`crate::objects::render::resolve_all`] like any other model. This is the general branch: the items
//! with their own drawing at the start of the function (the Kundun spear, the event rings...) do not
//! have it yet. TODO: neither do the extra `RenderPartObjectBodyColor2` calls of some items below +3
//! (ZzzObject.cpp:10134-10176).

use super::body::{BodyCtx, LightOp, body_passes};
use super::group::*;
use super::{Item, kind};
use crate::objects::render::flags::*;
use crate::objects::render::{Pass, Saw};

/// `b->BodyLight` of a pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlowLight {
    Fixed([f32; 3]),
    /// `g_Luminosity · k`, with `g_Luminosity = sin(WorldTime · 0.004) · 0.15 + 0.6` (ZzzScene.cpp:2765).
    Luminosity([f32; 3]),
    /// Excellent: `L = sin(WorldTime · 0.002) · 0.5 + 0.5` → `(L, 0.3 L, 1 − L)` (ZzzObject.cpp:10306-10307).
    Excellent,
    /// Ancient set: `PartObjectColor3` times `Alpha = sin(WorldTime · 0.001) · 0.5 + 0.4`
    /// (`RENDER_CHROME3` with `Alpha` < 0.99 multiplies `BodyLight` by it, ZzzBMD.cpp:1110-1118).
    Ancient([f32; 3]),
}

impl GlowLight {
    /// The color with `WorldTime` in ms (OpenGL clamps it to [0, 1] when drawing).
    pub fn at(self, world_time: f32) -> [f32; 3] {
        match self {
            GlowLight::Fixed(l) => l,
            GlowLight::Luminosity(k) => {
                let g = (world_time * 0.004).sin() * 0.15 + 0.6;
                k.map(|v| v * g)
            }
            GlowLight::Excellent => {
                let l = (world_time * 0.002).sin() * 0.5 + 0.5;
                [l, l * 0.3, 1.0 - l]
            }
            GlowLight::Ancient(c) => {
                let a = (world_time * 0.001).sin() * 0.5 + 0.4;
                c.map(|v| (v * a).max(0.0))
            }
        }
    }

    /// Changes over time.
    pub fn animated(self) -> bool {
        !matches!(self, GlowLight::Fixed(_))
    }
}

/// Which function draws a glow pass: they pick the meshes differently for some types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowCall {
    /// `RenderPartObjectBodyColor` (ZzzObject.cpp:8908-9080): the chrome and metal passes.
    Color,
    /// `RenderPartObjectBodyColor2` (ZzzObject.cpp:9082-9171): `RENDER_CHROME2/4` and the ancient pass.
    Color2,
    /// The excellent pass, drawn directly (ZzzObject.cpp:10296-10329).
    Excellent,
}

/// One glow call with its `RENDER_*` flags and its light (`b->BodyLight`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlowPass {
    pub flags: u32,
    pub light: GlowLight,
    pub call: GlowCall,
}

/// The light of an item pass: the level light ([`GlowLight`]) as `RenderPartObjectEffect` leaves it
/// in `b->BodyLight`, and what the item's own drawing does with it ([`LightOp`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassLight {
    pub base: GlowLight,
    pub op: LightOp,
}

impl PassLight {
    /// The color with `WorldTime` in ms.
    pub fn at(self, world_time: f32) -> [f32; 3] {
        self.op.apply(self.base.at(world_time), world_time)
    }

    /// Changes over time.
    pub fn animated(self) -> bool {
        self.op.animated() || (self.op.relative() && self.base.animated())
    }

    /// Relative to the light the caller gave (the character's `c->Light`); otherwise an absolute
    /// color (`Vector(..., b->BodyLight)`, the glow colors).
    pub fn relative(self) -> bool {
        self.op.relative() && matches!(self.base, GlowLight::Fixed(_) | GlowLight::Luminosity(_))
    }
}

/// A draw call of the item (`RenderPartObjectBody`, then the glow), resolved mesh by mesh with
/// [`crate::objects::render::resolve_all`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemPass {
    pub pass: Pass,
    pub light: PassLight,
    /// See [`super::body::BodyPass::gl_color`].
    pub gl_color: Option<[f32; 3]>,
}

/// Where an item is drawn (see [`BodyCtx`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemCtx {
    /// `b->HideSkin` (windows).
    pub hide_skin: bool,
    /// `b->NumMeshs`.
    pub num_meshes: usize,
    /// The caller's `Light` is (1, 1, 1): the window's (ZzzInventory.cpp:10600) or the hero's
    /// `AbilityLight`; not the terrain light other characters get.
    pub white_light: bool,
}

/// The level used to choose the glow (the `switch (Type)` at the start and the effects limit:
/// `min(Level, RenderLevel · 2 + 5)` with `GetRenderLevel()` < 4).
pub fn effect_level(item: &Item, render_level: u8) -> u8 {
    let mut level = item.level.min(15);
    let (g, i) = (item.group, item.index);
    level = match (g, i) {
        (HELPER, 3) | (HELPER, 10) => 8,
        (HELPER, 39..=42) | (POTION, 51) => 13,
        (HELPER, 30) | (HELPER, 50) | (HELPER, 52) | (HELPER, 53) => 0,
        (POTION, 13) | (POTION, 14) | (POTION, 16) | (POTION, 31) | (POTION, 12) | (POTION, 22) => 8,
        (POTION, 25) | (POTION, 26) => 8,
        (WING, 136) | (WING, 138) | (WING, 141) | (WING, 137) | (WING, 15) => 8,
        // `MODEL_COMPILED_CELE` and `MODEL_COMPILED_SOUL` (_define.h:609-610).
        (WING, 30) | (WING, 31) => 8,
        (WING, 0..=6) | (WING, 36..=43) | (WING, 11) => 0,
        (WING, 7) | (WING, 12..=14) | (WING, 16..=19) | (WING, 44..=47) => 9,
        (POTION, 17..=19) => {
            if level <= 6 {
                level * 2
            } else {
                13
            }
        }
        (POTION, 20) => 9,
        (WING, 139) | (WING, 140) | (WING, 142) | (WING, 143) | (POTION, 41..=44) | (POTION, 64) => 0,
        (BOW, 7) | (BOW, 15) => {
            if level >= 1 {
                level * 2 + 1
            } else {
                0
            }
        }
        _ => level,
    };
    if render_level < 4 {
        level = level.min(render_level * 2 + 5);
    }
    level
}

/// `PartObjectColor(Type, 1, 1, ...)` (ZzzObject.cpp:6483-6770): the chrome color.
pub fn part_object_color(item: &Item) -> [f32; 3] {
    let (g, i) = (item.group, item.index);
    let color = match (g, i) {
        (SPEAR, 9) => 1,
        (BOW, 5) | (BOW, 13) => 5,
        (SWORD, 14) | (STAFF, 5) | (POTION, 25) | (POTION, 26) => 2,
        (BOW, 17) | (BOW, 19) => 9,
        (WING, 14) => 2,
        (BOW, 18) => 10,
        (STAFF, 9) => 5,
        (SWORD, 31) => 10,
        (SHIELD, 16) => 6,
        (SPEAR, 10) => 9,
        (MACE, 8) => 9,
        (MACE, 9) => 10,
        (MACE, 10) => 12,
        (SWORD, 20) => 10,
        (SWORD, 21) => 5,
        (BOW, 20) => 16,
        (STAFF, 11) => 17,
        (MACE, 12) => 16,
        (SWORD, 22) => 18,
        (STAFF, 12) => 19,
        (BOW, 21) => 20,
        (SWORD, 23) => 23,
        (MACE, 14) => 22,
        (SWORD, 24) => 24,
        (STAFF, 13) => 25,
        (BOW, 22) => 26,
        (MACE, 15) => 28,
        (SWORD, 25) => 27,
        (STAFF, 14) => 24,
        (STAFF, 15) => 15,
        (STAFF, 16) => 1,
        (STAFF, 17) => 3,
        (STAFF, 18) => 30,
        (STAFF, 19) => 21,
        (STAFF, 20) => 5,
        (STAFF, 22) => 1,
        (SWORD, 28) => 8,
        (STAFF, 30) => 1,
        (STAFF, 31) => 19,
        (MACE, 17) => 40,
        (SHIELD, 19) => 29,
        (BOW, 23) => 35,
        (SHIELD, 20) => 36,
        (SHIELD, 21) => 30,
        (SPEAR, 11) => 20,
        (STAFF, 33) => 43,
        (STAFF, 34) => 5,
        (MACE, 18) => 5,
        (BOW, 24) => 36,
        (HELM..=BOOTS, _) => match i {
            1 | 48 => 1,
            9 | 12 => 2,
            3 => 3,
            13 => 4,
            4 | 14 | 18 => 5,
            6 => 6,
            15 => 7,
            16 | 27 => 10,
            17 | 19 | 20 => 9,
            21 | 24 => 16,
            22 => 17,
            23 | 25 => 11,
            26 => 12,
            28 => 15,
            29 => 18,
            30 => 19,
            31 => 20,
            32 => 21,
            33 => 22,
            34 => 24,
            35 => 25,
            36 => 26,
            45 | 52 => 36,
            37 => 27,
            38 => 28,
            39 => 29,
            40 => 30,
            41 => 31,
            42 => 32,
            43 => 33,
            44 => 34,
            46 => 42,
            47 => 37,
            49 => 35,
            50 => 39,
            51 => 40,
            53 => 41,
            _ => 0,
        },
        _ => 0,
    };
    match color {
        1 => [1.0, 0.2, 0.0],
        2 | 3 => [0.0, 0.5, 1.0],
        4 => [0.0, 0.8, 0.4],
        5 => [1.0, 1.0, 1.0],
        6 => [0.6, 0.8, 0.4],
        7 => [0.9, 0.8, 1.0],
        8 => [0.8, 0.8, 1.0],
        9 => [0.5, 0.5, 0.8],
        10 => [0.75, 0.65, 0.5],
        11 => [0.35, 0.35, 0.6],
        12 => [0.47, 0.67, 0.6],
        13 => [0.0, 0.3, 0.6],
        14 => [0.65, 0.65, 0.55],
        15 => [0.2, 0.3, 0.6],
        16 => [0.8, 0.46, 0.25],
        17 => [0.65, 0.45, 0.3],
        18 => [0.5, 0.4, 0.3],
        19 => [0.37, 0.37, 1.0],
        20 | 26 => [0.3, 0.7, 0.3],
        21 => [0.5, 0.4, 1.0],
        22 | 28 => [0.45, 0.45, 0.23],
        23 => [0.3, 0.3, 0.45],
        24 => [0.6, 0.5, 0.2],
        25 => [0.6, 0.6, 0.6],
        27 => [0.5, 0.6, 0.7],
        29 => [0.2, 0.7, 0.3],
        30 => [0.7, 0.3, 0.3],
        31 => [0.7, 0.5, 0.3],
        32 => [0.5, 0.2, 0.7],
        33 => [0.8, 0.4, 0.6],
        34 => [0.6, 0.4, 0.8],
        35 => [0.7, 0.4, 0.4],
        36 => [0.5, 0.5, 0.7],
        37 => [0.7, 0.5, 0.7],
        38 => [0.2, 0.4, 0.7],
        39 => [0.3, 0.6, 0.4],
        40 => [0.7, 0.2, 0.2],
        41 => [0.7, 0.2, 0.7],
        42 => [0.8, 0.4, 0.0],
        43 => [0.8, 0.6, 0.2],
        _ => [1.0, 0.5, 0.0],
    }
}

/// `PartObjectColor2(Type, 1, 1, Light, ...)` (ZzzObject.cpp:6771-6854): on top of the previous light.
pub fn part_object_color2(item: &Item, light: [f32; 3]) -> [f32; 3] {
    let (g, i) = (item.group, item.index);
    let color = match (g, i) {
        (BOW, 5) | (BOW, 13) | (SWORD, 14) | (STAFF, 5) => 2,
        (SWORD, 18) | (BOW, 17) | (STAFF, 9) => 0,
        (HELM..=BOOTS, _) => match i {
            4 | 14 | 15 | 17 | 39..=42 => 1,
            18 | 43 => 2,
            21 | 44 => 3,
            _ => 0,
        },
        _ => 0,
    };
    match color {
        1 => [light[0], light[1] * 0.5, 0.0],
        2 => [0.0, light[1] * 0.5, light[2]],
        3 => [1.0, 1.0, 1.0],
        _ => light,
    }
}

/// `PartObjectColor3` (ZzzObject.cpp:6855-6892): the ancient set glow color.
pub fn part_object_color3(item: &Item) -> [f32; 3] {
    let gold = (HELM..=BOOTS).contains(&item.group) && matches!(item.index, 3 | 9 | 17);
    if gold { [1.0, 0.7, 0.2] } else { [0.1, 0.6, 1.0] }
}

/// `b->BodyLight` when `RenderPartObjectEffect` calls `RenderPartObjectBody` (ZzzObject.cpp:10132-10274),
/// with the caller's `light` (the inventory's is (1, 1, 1)) and the effects limit
/// (`g_pOption->GetRenderLevel()`, 0 to 4).
pub fn level_light(item: &Item, light: [f32; 3], render_level: u8) -> GlowLight {
    let scaled = |k: f32| GlowLight::Fixed(light.map(|v| v * k));
    match effect_level(item, render_level) {
        0..=2 => GlowLight::Fixed(light),
        3 | 4 => GlowLight::Luminosity([light[0], light[1] * 0.6, light[2] * 0.6]),
        5 | 6 => GlowLight::Luminosity([light[0] * 0.5, light[1] * 0.7, light[2]]),
        _ if render_level == 0 => GlowLight::Fixed(light),
        7 | 8 => scaled(0.8),
        9 | 10 if render_level >= 2 => scaled(0.9),
        11 | 12 if render_level >= 3 => scaled(0.9),
        13..=15 if render_level >= 4 => scaled(0.9),
        _ => GlowLight::Fixed(light),
    }
}

/// The glow calls after `RenderPartObjectBody` (ZzzObject.cpp:10197-10339): level chrome, excellent and
/// ancient.
pub fn glow_passes(item: &Item, light: [f32; 3], render_level: u8) -> Vec<GlowPass> {
    let level = effect_level(item, render_level);
    let rl = render_level;
    let color = GlowLight::Fixed(part_object_color(item));
    let chrome = GlowPass { flags: CHROME | BRIGHT, light: color, call: GlowCall::Color };
    let metal = GlowPass { flags: METAL | BRIGHT, light: color, call: GlowCall::Color };
    let color2 = GlowLight::Fixed(part_object_color2(item, light.map(|v| v * 0.9)));
    let color2 = |flags: u32| GlowPass { flags: flags | BRIGHT, light: color2, call: GlowCall::Color2 };
    let mut out = match level {
        0..=6 => vec![],
        _ if rl == 0 => vec![],
        7 | 8 => vec![chrome],
        9 | 10 if rl >= 2 => vec![chrome, metal],
        11 | 12 if rl >= 3 => vec![color2(CHROME2), metal, chrome],
        13..=15 if rl >= 4 => vec![color2(CHROME4), metal, chrome],
        _ => vec![],
    };
    if rl == 0 {
        return out;
    }
    let t = item.kind();
    let k = |g: u8, i: u16| kind(g, i);
    let excellent_glow = item.excellent & 63 > 0
        && (t < k(WING, 0) || t > k(WING, 6))
        && t != k(HELPER, 30)
        && (t < k(WING, 36) || t > k(WING, 43))
        && (t < k(WING, 130) || t > k(WING, 134));
    if excellent_glow {
        out.push(GlowPass { flags: TEXTURE | BRIGHT, light: GlowLight::Excellent, call: GlowCall::Excellent });
    } else if item.is_ancient() {
        let light = GlowLight::Ancient(part_object_color3(item));
        out.push(GlowPass { flags: CHROME3 | BRIGHT, light, call: GlowCall::Color2 });
    }
    out
}

/// The meshes a glow call draws for this type, as `RenderBody` / `RenderMesh` templates (flags are
/// set by the caller) with the `b->BodyLight` change they make (ZzzObject.cpp:8960-9079, 9102-9169,
/// 10299-10329).
fn glow_calls(item: &Item, call: GlowCall, hide_skin: bool) -> Vec<(Pass, LightOp)> {
    let (g, i) = (item.group, item.index);
    let keep = |p: Pass| vec![(p, LightOp::Keep)];
    let body = |hidden: i32| keep(Pass::body(0).hidden(hidden));
    let mesh = |m: usize| keep(Pass::mesh(m, 0));
    match call {
        GlowCall::Color => match (g, i) {
            (STAFF, 5) | (SWORD, 20) => body(1),
            (SWORD, 21) => body(0),
            // `o->HiddenMesh = 2` around the call (ZzzObject.cpp:8960-8963, 9076-9079).
            (MACE, 7) => body(2),
            (SWORD, 31) | (SPEAR, 10) | (SHIELD, 16) | (MACE, 8..=13) => keep(Pass::body(0)),
            (SWORD, 26) => body(5),
            (SWORD, 27) => mesh(0),
            (SWORD, 28) => mesh(2),
            // `o->BlendMesh = 1` and a white `BodyLight` for the second call (ZzzObject.cpp:8987-8993).
            (STAFF, 30) => vec![
                (Pass::body(0).hidden(-1), LightOp::Keep),
                (Pass::mesh(1, 0).blend_mesh(1), LightOp::Set([1.0; 3])),
            ],
            (STAFF, 32) => mesh(1),
            (SHIELD, 19..=21) | (STAFF, 33 | 34) | (MACE, 18) | (BOW, 24) | (HELM, 49) => mesh(0),
            (HELM, 50) => mesh(1),
            (HELM, 53) => mesh(2),
            (SPEAR, 11) => vec![(Pass::mesh(0, 0), LightOp::Keep), (Pass::mesh(1, 0), LightOp::Keep)],
            _ => body(-1),
        },
        GlowCall::Color2 => match (g, i) {
            (STAFF, 5) | (SWORD, 20) | (HELPER, 50) => body(1),
            (SWORD, 21) | (POTION, 64) => body(0),
            (SWORD, 26) => body(5),
            (SHIELD, 19) | (HELM, 49) => mesh(0),
            (HELM, 50) => mesh(1),
            (HELM, 53) => mesh(2),
            _ => body(-1),
        },
        GlowCall::Excellent => match (g, i) {
            (HELM, 39..=44) if hide_skin => mesh([2, 1, 0, 2, 1, 2][i as usize - 39]),
            _ => body(-1),
        },
    }
}

/// Every draw call of the item, in order: its own drawing ([`body_passes`]) with the level light,
/// then the glow calls ([`glow_passes`]), which see what the drawing left in `o` and `b`.
pub fn item_passes(item: &Item, light: [f32; 3], render_level: u8, ctx: &ItemCtx) -> Vec<ItemPass> {
    let base = level_light(item, light, render_level);
    let white_light = ctx.white_light && base == GlowLight::Fixed(light);
    let body_ctx = BodyCtx { hide_skin: ctx.hide_skin, num_meshes: ctx.num_meshes, white_light };
    let body = body_passes(item, &body_ctx);
    let mut out: Vec<ItemPass> = body
        .passes
        .iter()
        .map(|p| ItemPass { pass: p.pass, light: PassLight { base, op: p.light }, gl_color: p.gl_color })
        .collect();
    for glow in glow_passes(item, light, render_level) {
        // `PartObjectColor2` starts from the `BodyLight` the item's drawing left (ZzzObject.cpp:9100);
        // `glow_passes` assumed the level light (`Light · 0.9`, a fixed light at those levels).
        let glow_light = match (glow.call, glow.light, base) {
            (GlowCall::Color2, GlowLight::Fixed(_), GlowLight::Fixed(l)) if body.light_after != LightOp::Keep => {
                if body.light_after.animated() {
                    // Only types whose `PartObjectColor2` keeps the light end with a varying one
                    // (SWORD+31, MACE+7, SHIELD+15).
                    PassLight { base, op: body.light_after }
                } else {
                    let after = body.light_after.apply(l, 0.0);
                    PassLight { base: GlowLight::Fixed(part_object_color2(item, after)), op: LightOp::Keep }
                }
            }
            _ => PassLight { base: glow.light, op: LightOp::Keep },
        };
        for (template, op) in glow_calls(item, glow.call, ctx.hide_skin) {
            let mut pass = Pass { flags: glow.flags, ..template };
            // What the item's drawing left in `o` and `b` (`o->BlendMesh`... are read by these calls).
            if pass.blend_mesh.is_none() {
                pass.blend_mesh = body.blend_mesh;
            }
            if pass.mesh.is_none() && pass.hidden_mesh.is_none() {
                pass.hidden_mesh = body.hidden_mesh;
            }
            pass.glow = body.glow;
            pass.stream_mesh = body.stream_mesh;
            if let Some(u) = body.tex_u {
                pass.tex_coord = Some([u, Saw::ZERO]);
            }
            let light = if op == LightOp::Keep { glow_light } else { PassLight { base: glow_light.base, op } };
            out.push(ItemPass { pass, light, gl_color: None });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(group: u8, index: u16, level: u8) -> Item {
        Item { level, ..Item::new(group, index) }
    }

    const CTX: ItemCtx = ItemCtx { hide_skin: true, num_meshes: 2, white_light: true };

    #[test]
    fn pasadas_por_nivel() {
        let flags = |it: &Item| item_passes(it, [1.0; 3], 4, &CTX).iter().map(|p| p.pass.flags).collect::<Vec<_>>();
        assert_eq!(flags(&lv(SWORD, 0, 0)), vec![TEXTURE]);
        assert_eq!(flags(&lv(SWORD, 0, 4)), vec![TEXTURE]);
        assert_eq!(flags(&lv(SWORD, 0, 7)), vec![TEXTURE, CHROME | BRIGHT]);
        assert_eq!(flags(&lv(SWORD, 0, 9)), vec![TEXTURE, CHROME | BRIGHT, METAL | BRIGHT]);
        assert_eq!(flags(&lv(SWORD, 0, 11)), vec![TEXTURE, CHROME2 | BRIGHT, METAL | BRIGHT, CHROME | BRIGHT]);
        assert_eq!(flags(&lv(SWORD, 0, 15)), vec![TEXTURE, CHROME4 | BRIGHT, METAL | BRIGHT, CHROME | BRIGHT]);
        // +7 and above: the base at 0.8 or 0.9.
        assert_eq!(level_light(&lv(SWORD, 0, 7), [1.0; 3], 4), GlowLight::Fixed([0.8; 3]));
        assert_eq!(item_passes(&lv(SWORD, 0, 13), [1.0; 3], 4, &CTX)[0].light.at(0.0), [0.9; 3]);
        // Excellent: one more pass adding the item's own texture.
        let exc = Item { excellent: 4, ..lv(SWORD, 0, 9) };
        let last = *item_passes(&exc, [1.0; 3], 4, &CTX).last().unwrap();
        assert_eq!((last.pass.flags, last.light.base), (TEXTURE | BRIGHT, GlowLight::Excellent));
        // Wings do not glow from level or excellent.
        assert_eq!(flags(&Item { excellent: 1, ..lv(WING, 4, 13) }), vec![TEXTURE]);
        // Jewels always as +8.
        assert_eq!(flags(&lv(POTION, 13, 0)), vec![TEXTURE, CHROME | BRIGHT]);
    }

    #[test]
    fn limite_de_efectos() {
        // With `RenderLevel` 1 a +13 stays at +7; with 0, at +5 (the pulsing blue) and no excellent.
        assert_eq!(effect_level(&lv(SWORD, 0, 13), 1), 7);
        let exc = Item { excellent: 1, ..lv(SWORD, 0, 13) };
        let p = item_passes(&exc, [1.0; 3], 0, &CTX);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].light.base, GlowLight::Luminosity([0.5, 0.7, 1.0]));
        assert!(p[0].light.animated() && p[0].light.relative());
    }

    /// The glow goes on top of the item's own drawing, in the original order, and picks the meshes
    /// as `RenderPartObjectBodyColor` does.
    #[test]
    fn glow_after_the_body() {
        // Divine Sword of Archangel +9: three body calls, then chrome and metal over every mesh.
        let p = item_passes(&lv(SWORD, 19, 9), [1.0; 3], 4, &CTX);
        let flags: Vec<u32> = p.iter().map(|p| p.pass.flags).collect();
        assert_eq!(flags, vec![TEXTURE | METAL, LIGHTMAP | TEXTURE, TEXTURE, CHROME | BRIGHT, METAL | BRIGHT]);
        assert_eq!(p[0].light.at(0.0), [0.9; 3]);
        assert_eq!((p[3].pass.mesh, p[3].pass.hidden_mesh), (None, Some(-1)));
        // `MODEL_SWORD+28` +7: the chrome only on mesh 2 (ZzzObject.cpp:8983), with the `o->BlendMesh = 1`
        // the drawing left.
        let p = item_passes(&lv(SWORD, 28, 7), [1.0; 3], 4, &CTX);
        let glow = p.last().unwrap();
        assert_eq!((glow.pass.flags, glow.pass.mesh, glow.pass.blend_mesh), (CHROME | BRIGHT, Some(2), Some(1)));
        // SWORD+22 leaves `BodyLight` at 0.7: the +13 `RENDER_CHROME4` pass starts from it.
        let p = item_passes(&lv(SWORD, 22, 13), [1.0; 3], 4, &CTX);
        let c4 = p.iter().find(|p| p.pass.flags == CHROME4 | BRIGHT).unwrap();
        assert_eq!(c4.light.at(0.0), [0.7; 3]);
    }

    #[test]
    fn colores() {
        // Kris: color 0 (1, 0.5, 0); the dragon armor (index 1): color 1 (1, 0.2, 0).
        assert_eq!(part_object_color(&Item::new(SWORD, 0)), [1.0, 0.5, 0.0]);
        assert_eq!(part_object_color(&Item::new(ARMOR, 1)), [1.0, 0.2, 0.0]);
        assert_eq!(part_object_color2(&Item::new(ARMOR, 4), [0.9; 3]), [0.9, 0.45, 0.0]);
        let ex = GlowLight::Excellent.at(0.0);
        assert_eq!(ex, [0.5, 0.15, 0.5]);
        assert!((GlowLight::Luminosity([1.0; 3]).at(0.0)[0] - 0.6).abs() < 1e-6);
    }
}
