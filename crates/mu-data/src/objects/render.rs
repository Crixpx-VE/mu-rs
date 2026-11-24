//! How the original draws each mesh of a model: `BMD::RenderBody` / `BMD::RenderMesh`
//! (`ZzzBMD.cpp` ≈937-1354 and ≈1851-1913), translated into data.
//!
//! Each object's draw code (`RenderObject`, `ZzzObject.cpp`) makes one or more calls to
//! `RenderBody` (all meshes) or `RenderMesh(i)` (one), each with its `RENDER_*`, a texture replacing
//! the mesh's (`MeshTexture`), a `b->BodyLight` and a `b->StreamMesh`. Here each call is a
//! [`Pass`]; [`resolve`] decides, per mesh, which texture, coordinates, color and blending it gets
//! ([`MeshLook`]), like the branches of `RenderMesh`. Passes are drawn in order and, since the original
//! uses `glDepthFunc(GL_LEQUAL)`, a later pass over the same mesh covers the earlier one.

use crate::bmd::TextureScript;

/// `RENDER_*` (`ZzzBMD.h:10-30`).
pub mod flags {
    pub const COLOR: u32 = 0x0000_0001;
    pub const TEXTURE: u32 = 0x0000_0002;
    pub const CHROME: u32 = 0x0000_0004;
    pub const METAL: u32 = 0x0000_0008;
    pub const LIGHTMAP: u32 = 0x0000_0010;
    pub const BRIGHT: u32 = 0x0000_0040;
    pub const DARK: u32 = 0x0000_0080;
    pub const CHROME2: u32 = 0x0000_0200;
    pub const CHROME3: u32 = 0x0000_0800;
    pub const CHROME4: u32 = 0x0000_1000;
    pub const NODEPTH: u32 = 0x0000_2000;
    pub const CHROME5: u32 = 0x0000_4000;
    pub const OIL: u32 = 0x0000_8000;
    pub const CHROME6: u32 = 0x0001_0000;
    pub const CHROME7: u32 = 0x0002_0000;
    /// Any of the flags that take the chrome branch of `RenderMesh` (`ZzzBMD.cpp:1022-1031`).
    pub const ANY_CHROME: u32 = CHROME | CHROME2 | CHROME3 | CHROME4 | CHROME5 | CHROME6 | CHROME7 | METAL | OIL;
}

/// Global textures the draw code uses instead of the mesh's (`BITMAP_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bitmap {
    /// `BITMAP_CHROME`: `Effect\Chrome01.jpg` (`GL_LINEAR`, `GL_REPEAT`; `ZzzOpenData.cpp:5091`).
    Chrome,
    /// `BITMAP_CHROME2`: `Effect\Chrome02.jpg` (`GL_NEAREST`, `GL_CLAMP_TO_EDGE`; `ZzzOpenData.cpp:5122`).
    Chrome2,
    /// `BITMAP_CHROME3`: `Effect\Chrome03.jpg` (`GL_NEAREST`, `GL_CLAMP_TO_EDGE`).
    Chrome3,
    /// `BITMAP_CHROME6`: `Effect\Chrome06.jpg` (`GL_NEAREST`, `GL_CLAMP_TO_EDGE`).
    Chrome6,
    /// `BITMAP_CHROME7`: `Effect\Chrome07.jpg` (`GL_NEAREST`, `GL_CLAMP_TO_EDGE`).
    Chrome7,
    /// `BITMAP_CHROME8`: `Effect\Chrome08.jpg` (`GL_LINEAR`, `GL_CLAMP_TO_EDGE`), loaded by Raklion
    /// and other maps (`MapManager.cpp:753`, `:759`).
    Chrome8,
    /// `BITMAP_SHINY`: `Effect\Shiny01.jpg` (`GL_LINEAR`, `GL_CLAMP_TO_EDGE`; `ZzzOpenData.cpp:5085`).
    Shiny,
    /// `BITMAP_WATER + WaterTextureNumber`: the 32 caustics `Object8\wt00-31.jpg` (`GL_LINEAR`,
    /// `GL_REPEAT`; `MapManager.cpp:1041-1053`), one per tick (`WaterTextureNumber++`, ZzzScene.cpp:2315).
    /// [`Bitmap::path`] is the first one; the rest, [`Bitmap::frame_path`].
    Water,
    /// Another texture loaded by the map (e.g. `BITMAP_CHROME+2` = `Object9\sand01.jpg` in Tarkan,
    /// `MapManager.cpp:150`): path relative to `Data` and whether it is `GL_NEAREST` + `GL_CLAMP_TO_EDGE`.
    File { path: &'static str, nearest_clamp: bool },
}

impl Bitmap {
    /// Path relative to `Data` (with the original extension: `.jpg` = `.OZJ`, `.tga` = `.OZT`).
    pub fn path(self) -> &'static str {
        match self {
            Bitmap::Chrome => "Effect/Chrome01.jpg",
            Bitmap::Chrome2 => "Effect/Chrome02.jpg",
            Bitmap::Chrome3 => "Effect/Chrome03.jpg",
            Bitmap::Chrome6 => "Effect/Chrome06.jpg",
            Bitmap::Chrome7 => "Effect/Chrome07.jpg",
            Bitmap::Chrome8 => "Effect/Chrome08.jpg",
            Bitmap::Shiny => "Effect/Shiny01.jpg",
            Bitmap::Water => "Object8/wt00.jpg",
            Bitmap::File { path, .. } => path,
        }
    }

    /// Frames of an animated texture (`BITMAP_WATER`: 32); 1 for the rest.
    pub fn frames(self) -> usize {
        if self == Bitmap::Water { WATER_FRAMES } else { 1 }
    }

    /// Path of frame `i` (`Object8\wt{i:02}.jpg` for water; the usual one for the rest).
    pub fn frame_path(self, i: usize) -> String {
        match self {
            Bitmap::Water => format!("Object8/wt{:02}.jpg", i % WATER_FRAMES),
            _ => self.path().to_string(),
        }
    }

    /// `GL_NEAREST` and `GL_CLAMP_TO_EDGE` (otherwise `GL_LINEAR`; `GL_REPEAT` except `Shiny`).
    pub fn nearest(self) -> bool {
        match self {
            Bitmap::Chrome2 | Bitmap::Chrome3 | Bitmap::Chrome6 | Bitmap::Chrome7 => true,
            Bitmap::File { nearest_clamp, .. } => nearest_clamp,
            Bitmap::Chrome | Bitmap::Chrome8 | Bitmap::Shiny | Bitmap::Water => false,
        }
    }

    pub fn clamp(self) -> bool {
        self.nearest() || matches!(self, Bitmap::Shiny | Bitmap::Chrome8)
    }
}

/// `BITMAP_WATER` .. `BITMAP_WATER + 31`.
pub const WATER_FRAMES: usize = 32;

/// `b->BodyLight` that changes over time during a pass: `v = sin(Timer + WorldTime · freq_ms) ·
/// amp + base`, and `BodyLight = (v, v, v)` or, with `relative`, `BodyLight · v` (the ground light).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyWave {
    pub freq_ms: f32,
    pub amp: f32,
    pub base: f32,
    pub relative: bool,
    /// Adds the object's `o->Timer` (each object its own phase; otherwise `Timer = 0`).
    pub timer: bool,
}

impl BodyWave {
    /// The factor `v` with `WorldTime` in ms and the object's `o->Timer`.
    pub fn at(self, world_time: f32, timer: f32) -> f32 {
        let t = if self.timer { timer } else { 0.0 };
        (t + world_time * self.freq_ms).sin() * self.amp + self.base
    }

    /// `b->BodyLight` with the light of object `object`.
    pub fn light(self, object: [f32; 3], world_time: f32, timer: f32) -> [f32; 3] {
        let v = self.at(world_time, timer);
        if self.relative { object.map(|c| c * v) } else { [v; 3] }
    }
}

/// Chrome coordinate formula (`g_chrome`) of `RenderMesh` (`ZzzBMD.cpp:1054-1108`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ChromeMode {
    Chrome = 1,
    Chrome2 = 2,
    Chrome3 = 3,
    Chrome4 = 4,
    Chrome5 = 5,
    Chrome6 = 6,
    Chrome7 = 7,
    /// `RENDER_METAL` (and `RENDER_OIL`, not translated): the `else` branch.
    Metal = 8,
}

/// Which formula a `RENDER_*` combination uses, in the order of the original `if`s.
pub fn chrome_mode(f: u32) -> Option<ChromeMode> {
    use flags::*;
    if f & ANY_CHROME == 0 {
        return None;
    }
    Some(if f & CHROME2 != 0 {
        ChromeMode::Chrome2
    } else if f & CHROME3 != 0 {
        ChromeMode::Chrome3
    } else if f & CHROME4 != 0 {
        ChromeMode::Chrome4
    } else if f & CHROME5 != 0 {
        ChromeMode::Chrome5
    } else if f & CHROME6 != 0 {
        ChromeMode::Chrome6
    } else if f & CHROME7 != 0 {
        ChromeMode::Chrome7
    } else if f & CHROME != 0 {
        ChromeMode::Chrome
    } else {
        ChromeMode::Metal
    })
}

/// `LightVector` (`ZzzBMD.cpp:49`).
pub const LIGHT_VECTOR: [f32; 3] = [0.0, -0.1, -0.8];

/// `g_chrome[j]` from an already rotated normal (`NormalTransform`, MU axes) with `WorldTime` in ms.
///
/// Camera-independent: `BMD::Chrome` (the one using `g_vright`) is never called. All formulas are
/// linear in the normal, so per-pixel interpolation equals per-vertex. The shader version
/// (`model_light.wesl`) is a copy of this one.
pub fn chrome_uv(mode: ChromeMode, n: [f32; 3], world_time: f32) -> [f32; 2] {
    let wt = world_time.floor();
    let wave = wt.rem_euclid(10000.0) * 0.0001;
    let wave2 = wt.rem_euclid(5000.0) * 0.00024 - 0.4;
    let l = [(world_time * 0.001).cos(), (world_time * 0.002).sin(), 1.0];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    match mode {
        ChromeMode::Chrome2 => [(n[2] + n[0]) * 0.8 + wave2 * 2.0, (n[1] + n[0]) * 1.0 + wave2 * 3.0],
        ChromeMode::Chrome3 => [dot(n, LIGHT_VECTOR), 1.0 - dot(n, LIGHT_VECTOR)],
        ChromeMode::Chrome4 => [
            dot(n, l) + n[1] * 0.5 + l[1] * 3.0,
            1.0 - dot(n, l) - (n[2] * 0.5 + wave * 3.0),
        ],
        ChromeMode::Chrome5 => [
            dot(n, l) + n[1] * 3.0 + l[1] * 5.0,
            1.0 - dot(n, l) - (n[2] * 2.5 + wave * 1.0),
        ],
        ChromeMode::Chrome6 => {
            let s = (n[2] + n[0]) * 0.8 + wave2 * 2.0;
            [s, s]
        }
        ChromeMode::Chrome7 => {
            let s = (n[2] + n[0]) * 0.8 + world_time * 0.00006;
            [s, s]
        }
        ChromeMode::Chrome => [n[2] * 0.5 + wave, n[1] * 0.5 + wave * 2.0],
        ChromeMode::Metal => [n[2] * 0.5 + 0.2, n[1] * 0.5 + 0.5],
    }
}

/// `k · ((int)WorldTime % period)`, with `WorldTime` in ms: the `BlendMeshTexCoordU/V` of `MoveObject`
/// (e.g. `-(int)WorldTime%1000 * 0.001f` = `Saw { period: 1000, k: -0.001 }`). `k = 0`: none.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Saw {
    pub period: f32,
    pub k: f32,
}

impl Saw {
    pub const ZERO: Saw = Saw { period: 1.0, k: 0.0 };

    pub fn at(self, world_time: f32) -> f32 {
        if self.k == 0.0 {
            return 0.0;
        }
        self.k * world_time.floor().rem_euclid(self.period)
    }
}

/// A `RenderBody` (`mesh: None`) or `RenderMesh(mesh)` call from an object's draw code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pass {
    /// `None` = `RenderBody`: all meshes, with texture scripts (`_H` hides, `_R` adds) and
    /// without `HiddenMesh`. `Some(i)` = `RenderMesh(i, ...)`: the `_H`/`_R` scripts do not apply.
    pub mesh: Option<usize>,
    /// `RENDER_*` ([`flags`]).
    pub flags: u32,
    /// `MeshTexture` / `Texture`: replaces the mesh's texture.
    pub texture: Option<Bitmap>,
    /// `b->BodyLight` during the call (`None` = the object's: the terrain light).
    pub light: Option<[f32; 3]>,
    /// `b->StreamMesh` during the call (`None` = the model's, [`super::ModelRules::stream_mesh`]).
    pub stream_mesh: Option<i32>,
    /// The `BlendMesh` parameter (`None` = `o->BlendMesh`, [`super::ModelRules::additive_mesh`]).
    /// Compared with the mesh's texture index; `<= -2` = all are additive.
    pub blend_mesh: Option<i32>,
    /// The `BlendMeshTexCoordU/V` parameter (`None` = those of `o`, [`super::ModelRules::tex_coord`]).
    pub tex_coord: Option<[Saw; 2]>,
    /// The `BlendMeshLight` parameter of this call (`None` = that of `o`, [`super::ModelRules::glow`]),
    /// e.g. `RenderBody(RENDER_TEXTURE, o->Alpha, 0, fLumi, ...)`.
    pub glow: Option<super::Glow>,
    /// The call's `Alpha` parameter (`None` = `o->Alpha`, 1). With `RENDER_TEXTURE` and below
    /// 0.99, `EnableAlphaTest` and `glColor4f(..., Alpha)` (`ZzzBMD.cpp:1201-1204`, 1272-1280).
    pub alpha: Option<f32>,
    /// Time-varying `b->BodyLight` (instead of [`Pass::light`]).
    pub body_wave: Option<BodyWave>,
}

impl Pass {
    /// `RenderBody(flags, ...)`.
    pub const fn body(flags: u32) -> Self {
        Self {
            mesh: None,
            flags,
            texture: None,
            light: None,
            stream_mesh: None,
            blend_mesh: None,
            tex_coord: None,
            glow: None,
            alpha: None,
            body_wave: None,
        }
    }
    /// The call's `Alpha`.
    pub const fn alpha(mut self, a: f32) -> Self {
        self.alpha = Some(a);
        self
    }
    /// `BodyLight = (v, v, v)`, `v = sin(WorldTime · freq_ms) · amp + base`.
    pub const fn body_pulse(mut self, freq_ms: f32, amp: f32, base: f32) -> Self {
        self.body_wave = Some(BodyWave { freq_ms, amp, base, relative: false, timer: false });
        self
    }
    /// `BodyLight *= v`, `v = sin(Timer? + WorldTime · freq_ms) · amp + base`.
    pub const fn body_scale_pulse(mut self, freq_ms: f32, amp: f32, base: f32, timer: bool) -> Self {
        self.body_wave = Some(BodyWave { freq_ms, amp, base, relative: true, timer });
        self
    }
    /// `RenderMesh(mesh, flags, ...)`.
    pub const fn mesh(mesh: usize, flags: u32) -> Self {
        Self { mesh: Some(mesh), ..Self::body(flags) }
    }
    pub const fn texture(mut self, b: Bitmap) -> Self {
        self.texture = Some(b);
        self
    }
    pub const fn light(mut self, l: [f32; 3]) -> Self {
        self.light = Some(l);
        self
    }
    pub const fn stream(mut self, mesh: i32) -> Self {
        self.stream_mesh = Some(mesh);
        self
    }
    pub const fn blend_mesh(mut self, mesh: i32) -> Self {
        self.blend_mesh = Some(mesh);
        self
    }
    pub const fn tex_coord(mut self, u: Saw, v: Saw) -> Self {
        self.tex_coord = Some([u, v]);
        self
    }
    /// The call's `BlendMeshLight`.
    pub const fn glow(mut self, g: super::Glow) -> Self {
        self.glow = Some(g);
        self
    }
    /// `BlendMeshLight = sin(WorldTime · freq_ms) · amp + base` (original frequency, per ms).
    pub const fn pulse(self, freq_ms: f32, amp: f32, base: f32) -> Self {
        self.glow(super::Glow::Pulse { freq: freq_ms * 1000.0, amp, base })
    }
}

/// The default pass: `RenderBody(RENDER_TEXTURE, o->Alpha, o->BlendMesh, ...)`.
pub const DEFAULT_PASSES: &[Pass] = &[Pass::body(flags::TEXTURE)];

/// Blending a mesh ends up with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookBlend {
    /// `DisableAlphaBlend`.
    Opaque,
    /// `EnableAlphaTest`: blending + alpha test (> 0.25), writes depth.
    AlphaTest,
    /// `EnableAlphaBlend`: ONE, ONE (additive), no depth write.
    Add,
    /// `EnableAlphaBlendMinus`: ZERO, ONE_MINUS_SRC_COLOR (darkens), no depth write.
    Subtract,
}

/// Color the texture is multiplied by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookColor {
    /// `LightTransform`: `BodyLight` times the intensity from the normal.
    Lit,
    /// `glColor3fv(BodyLight)`: the `StreamMesh` and chrome.
    Flat,
    /// `glColor3f(BodyLight · BlendMeshLight)`: the `BlendMesh`.
    BlendLight,
}

/// How a mesh looks in a pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshLook {
    /// `None` = the mesh's texture.
    pub texture: Option<Bitmap>,
    /// `false` = `DisableTexture` (color only).
    pub textured: bool,
    /// Chrome coordinates instead of the model's.
    pub chrome: Option<ChromeMode>,
    /// `BlendMeshTexCoordU/V` are added to the coordinates (`EnableWave`, or `RENDER_CHROME4`).
    pub wave: Option<[Saw; 2]>,
    pub color: LookColor,
    /// The pass's `BodyLight` (`None` = the object's).
    pub light: Option<[f32; 3]>,
    pub blend: LookBlend,
    /// Color alpha (`glColor4f`); 1 = opaque.
    pub alpha: f32,
    /// Time-varying `BodyLight` (overrides [`MeshLook::light`]).
    pub body_wave: Option<BodyWave>,
}

/// What is needed about a mesh to decide how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshInfo {
    pub index: usize,
    /// `m->Texture`.
    pub texture_index: i32,
    pub script: Option<TextureScript>,
    /// `pBitmap->Components == 4` of the mesh's texture (TGA with alpha).
    pub has_alpha: bool,
}

/// Object data shared by all its passes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectDraw {
    /// `o->BlendMesh` (`-1` = none).
    pub blend_mesh: i32,
    /// `Models[tipo].StreamMesh` (`-1` = none).
    pub stream_mesh: i32,
    /// `o->BlendMeshTexCoordU/V`.
    pub tex_coord: [Saw; 2],
    /// `o->HiddenMesh` (`-1` = none).
    pub hidden_mesh: i32,
}

/// How mesh `m` looks in pass `p` (`None` = that call does not draw it).
pub fn resolve(p: &Pass, o: &ObjectDraw, m: &MeshInfo) -> Option<MeshLook> {
    use flags::*;
    let i = m.index as i32;
    let mut blend_mesh = p.blend_mesh.unwrap_or(o.blend_mesh);
    match p.mesh {
        // `RenderBody`: `_H` and `HiddenMesh` are not drawn; `_R` makes the mesh its own `BlendMesh`.
        None => {
            if i == o.hidden_mesh || m.script.is_some_and(|s| s.hidden) {
                return None;
            }
            if m.script.is_some_and(|s| s.bright) {
                blend_mesh = i;
            }
        }
        Some(mesh) if mesh != m.index => return None,
        Some(_) => {}
    }
    let tex_coord = p.tex_coord.unwrap_or(o.tex_coord);
    let has_offset = tex_coord.iter().any(|s| s.k != 0.0);
    let model_stream = p.stream_mesh.unwrap_or(o.stream_mesh);
    // The `_S` script only enables scrolling; flat light comes from the model's `StreamMesh`.
    let stream = if m.script.is_some_and(|s| s.stream) { i } else { model_stream };
    let wave = ((i == blend_mesh || i == stream) && has_offset).then_some(tex_coord);
    let lit = if i == model_stream { LookColor::Flat } else { LookColor::Lit };
    // `pBitmap`: the texture finally used.
    let has_alpha = if p.texture.is_some() { p.texture.is_some_and(|b| b.path().ends_with(".tga")) } else { m.has_alpha };
    let f = p.flags;
    let look = |texture, blend, color, wave| MeshLook {
        texture,
        textured: true,
        chrome: None,
        wave,
        color,
        light: p.light,
        blend,
        alpha: p.alpha.unwrap_or(1.0),
        body_wave: p.body_wave,
    };
    let translucent = p.alpha.is_some_and(|a| a < 0.99);

    if f & COLOR != 0 {
        let blend = if f & BRIGHT != 0 {
            LookBlend::Add
        } else if f & DARK != 0 {
            LookBlend::Subtract
        } else {
            LookBlend::Opaque
        };
        return Some(MeshLook { textured: false, ..look(None, blend, LookColor::Flat, None) });
    }
    if let Some(mode) = chrome_mode(f) {
        if m.script.is_some_and(|s| s.none_blend) {
            return None;
        }
        let blend = if f & (CHROME3 | CHROME4 | CHROME5 | CHROME7) != 0 || f & BRIGHT != 0 {
            LookBlend::Add
        } else if f & DARK != 0 {
            LookBlend::Subtract
        } else {
            LookBlend::Opaque
        };
        // `BindTexture`: with `MeshTexture == -1` each formula's own; otherwise the requested one.
        let texture = match p.texture {
            Some(t) => Some(t),
            None if f & (CHROME2 | CHROME3 | CHROME4) != 0 => Some(Bitmap::Chrome2),
            None if f & CHROME6 != 0 => Some(Bitmap::Chrome6),
            None if f & CHROME != 0 => Some(Bitmap::Chrome),
            None if f & METAL != 0 => Some(Bitmap::Shiny),
            None => None,
        };
        // Only `RENDER_CHROME4` adds `BlendMeshTexCoordU/V` (`ZzzBMD.cpp:1301`).
        let wave = (mode == ChromeMode::Chrome4 && has_offset).then_some(tex_coord);
        return Some(MeshLook { chrome: Some(mode), ..look(texture, blend, LookColor::Flat, wave) });
    }
    if blend_mesh <= -2 || m.texture_index == blend_mesh {
        let blend = if f & DARK != 0 { LookBlend::Subtract } else { LookBlend::Add };
        return Some(look(p.texture, blend, LookColor::BlendLight, wave));
    }
    if f & TEXTURE != 0 {
        let blend = if f & BRIGHT != 0 {
            LookBlend::Add
        } else if f & DARK != 0 {
            LookBlend::Subtract
        } else if translucent || has_alpha {
            LookBlend::AlphaTest
        } else {
            LookBlend::Opaque
        };
        return Some(look(p.texture, blend, lit, wave));
    }
    if f & BRIGHT != 0 {
        if has_alpha {
            return None;
        }
        return Some(MeshLook { textured: false, ..look(None, LookBlend::Add, lit, None) });
    }
    // No `RENDER_TEXTURE` or anything else: the original draws with whatever texture and blending were set.
    Some(look(p.texture, if has_alpha { LookBlend::AlphaTest } else { LookBlend::Opaque }, lit, wave))
}

/// Each mesh's passes, in order, dropping those fully covered by a later opaque pass on the same
/// mesh (with `GL_LEQUAL` the last drawn wins): `out[k]` = (pass index, look).
pub fn resolve_all(passes: &[Pass], o: &ObjectDraw, m: &MeshInfo) -> Vec<(usize, MeshLook)> {
    let looks: Vec<(usize, MeshLook)> =
        passes.iter().enumerate().filter_map(|(k, p)| resolve(p, o, m).map(|l| (k, l))).collect();
    let last_opaque = looks.iter().rposition(|(_, l)| l.blend == LookBlend::Opaque && l.textured);
    match last_opaque {
        Some(cut) => looks[cut..].to_vec(),
        None => looks,
    }
}

#[cfg(test)]
mod tests {
    use super::flags::*;
    use super::*;

    fn obj() -> ObjectDraw {
        ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 }
    }
    fn mesh(i: usize, script: Option<TextureScript>, alpha: bool) -> MeshInfo {
        MeshInfo { index: i, texture_index: i as i32, script, has_alpha: alpha }
    }

    #[test]
    fn cromado_sin_camara() {
        // RENDER_CHROME at WorldTime = 2500 ms: Wave = 0.25.
        let uv = chrome_uv(ChromeMode::Chrome, [0.0, 0.6, 0.8], 2500.0);
        assert!((uv[0] - (0.4 + 0.25)).abs() < 1e-6 && (uv[1] - (0.3 + 0.5)).abs() < 1e-6, "{uv:?}");
        // Wave = (int)WorldTime % 10000 · 0.0001: restarts every 10 s.
        assert_eq!(chrome_uv(ChromeMode::Chrome, [0.0, 0.0, 1.0], 10_000.0), [0.5, 0.0]);
        // CHROME3: LightVector (0, -0.1, -0.8).
        let uv = chrome_uv(ChromeMode::Chrome3, [0.0, 0.0, -1.0], 0.0);
        assert!((uv[0] - 0.8).abs() < 1e-6 && (uv[1] - 0.2).abs() < 1e-6);
        // METAL: time-independent.
        assert_eq!(chrome_uv(ChromeMode::Metal, [0.0, 0.0, 0.0], 1234.0), [0.2, 0.5]);
        // Order of the `if`s: CHROME2 beats CHROME; BRIGHT alone is not chrome.
        assert_eq!(chrome_mode(CHROME | CHROME2 | BRIGHT), Some(ChromeMode::Chrome2));
        assert_eq!(chrome_mode(CHROME | BRIGHT), Some(ChromeMode::Chrome));
        assert_eq!(chrome_mode(BRIGHT | TEXTURE), None);
    }

    #[test]
    fn dientes_de_sierra() {
        let s = Saw { period: 1000.0, k: -0.001 };
        assert!((s.at(1250.0) + 0.25).abs() < 1e-6);
        assert_eq!(s.at(2000.0), 0.0);
        let s = Saw { period: 50000.0, k: -0.00005 };
        assert!((s.at(49_999.0) + 2.49995).abs() < 1e-4);
    }

    #[test]
    fn ramas_de_render_mesh() {
        let o = ObjectDraw { blend_mesh: 4, ..obj() };
        let body = Pass::body(TEXTURE);
        // The BlendMesh adds with BodyLight·BlendMeshLight.
        let l = resolve(&body, &o, &mesh(4, None, false)).unwrap();
        assert_eq!((l.blend, l.color), (LookBlend::Add, LookColor::BlendLight));
        // TGA: alpha test; JPG: opaque.
        assert_eq!(resolve(&body, &o, &mesh(1, None, true)).unwrap().blend, LookBlend::AlphaTest);
        assert_eq!(resolve(&body, &o, &mesh(1, None, false)).unwrap().blend, LookBlend::Opaque);
        // With a chrome texture (JPG) instead of the TGA: opaque.
        let p = Pass::body(TEXTURE).texture(Bitmap::Chrome);
        assert_eq!(resolve(&p, &o, &mesh(1, None, true)).unwrap().blend, LookBlend::Opaque);
        // Scripts: `_H` is not drawn in RenderBody but is with RenderMesh; `_R` adds only in RenderBody.
        let h = Some(TextureScript { hidden: true, ..Default::default() });
        assert!(resolve(&body, &obj(), &mesh(0, h, false)).is_none());
        assert!(resolve(&Pass::mesh(0, TEXTURE), &obj(), &mesh(0, h, false)).is_some());
        let r = Some(TextureScript { bright: true, ..Default::default() });
        assert_eq!(resolve(&body, &obj(), &mesh(2, r, false)).unwrap().blend, LookBlend::Add);
        assert_eq!(resolve(&Pass::mesh(2, TEXTURE), &obj(), &mesh(2, r, false)).unwrap().blend, LookBlend::Opaque);
        // `_N`: no chrome.
        let n = Some(TextureScript { none_blend: true, ..Default::default() });
        assert!(resolve(&Pass::body(CHROME | BRIGHT), &obj(), &mesh(0, n, false)).is_none());
        // Chrome + BRIGHT: additive with BITMAP_CHROME and flat color.
        let l = resolve(&Pass::mesh(0, CHROME | BRIGHT), &obj(), &mesh(0, None, false)).unwrap();
        assert_eq!((l.blend, l.color, l.texture, l.chrome), (LookBlend::Add, LookColor::Flat, Some(Bitmap::Chrome), Some(ChromeMode::Chrome)));
        // DARK: subtract.
        assert_eq!(resolve(&Pass::body(TEXTURE | DARK), &obj(), &mesh(0, None, false)).unwrap().blend, LookBlend::Subtract);
    }

    #[test]
    fn stream_mesh() {
        let saw = [Saw { period: 1000.0, k: -0.001 }, Saw::ZERO];
        let o = ObjectDraw { tex_coord: saw, ..obj() };
        // Model StreamMesh: flat light and scrolling.
        let p = Pass::body(TEXTURE).stream(2);
        let l = resolve(&p, &o, &mesh(2, None, false)).unwrap();
        assert_eq!((l.color, l.wave), (LookColor::Flat, Some(saw)));
        let l = resolve(&p, &o, &mesh(1, None, false)).unwrap();
        assert_eq!((l.color, l.wave), (LookColor::Lit, None));
        // `_S` script: scrolls but stays lit.
        let s = Some(TextureScript { stream: true, ..Default::default() });
        let l = resolve(&Pass::body(TEXTURE), &o, &mesh(1, s, false)).unwrap();
        assert_eq!((l.color, l.wave), (LookColor::Lit, Some(saw)));
    }

    #[test]
    fn la_pasada_opaca_tapa_las_anteriores() {
        let passes = [Pass::body(TEXTURE).texture(Bitmap::Chrome).stream(2), Pass::body(TEXTURE)];
        // JPG mesh: the second pass covers it completely.
        assert_eq!(resolve_all(&passes, &obj(), &mesh(0, None, false)).len(), 1);
        // TGA mesh: the chrome shows through the holes of the second pass.
        let both = resolve_all(&passes, &obj(), &mesh(2, None, true));
        assert_eq!(both.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![0, 1]);
    }
}
