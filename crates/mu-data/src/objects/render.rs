//! Cómo dibuja el original cada malla de un modelo: `BMD::RenderBody` / `BMD::RenderMesh`
//! (`ZzzBMD.cpp` ≈937-1354 y ≈1851-1913), traducido a datos.
//!
//! El código de dibujo de cada objeto (`RenderObject`, `ZzzObject.cpp`) hace una o varias llamadas a
//! `RenderBody` (todas las mallas) o `RenderMesh(i)` (una), cada una con sus `RENDER_*`, una textura en
//! lugar de la de la malla (`MeshTexture`), un `b->BodyLight` y un `b->StreamMesh`. Aquí cada llamada es
//! un [`Pass`]; [`resolve`] decide, para cada malla, con qué textura, coordenadas, color y mezcla queda
//! ([`MeshLook`]), como las ramas de `RenderMesh`. Las pasadas se dibujan en orden y, como el original
//! usa `glDepthFunc(GL_LEQUAL)`, una pasada posterior sobre la misma malla tapa a la anterior.

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
    /// Cualquiera de las que entran en la rama de cromado de `RenderMesh` (`ZzzBMD.cpp:1022-1031`).
    pub const ANY_CHROME: u32 = CHROME | CHROME2 | CHROME3 | CHROME4 | CHROME5 | CHROME6 | CHROME7 | METAL | OIL;
}

/// Texturas globales que el código de dibujo pone en lugar de la de la malla (`BITMAP_*`).
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
    /// `BITMAP_SHINY`: `Effect\Shiny01.jpg` (`GL_LINEAR`, `GL_CLAMP_TO_EDGE`; `ZzzOpenData.cpp:5085`).
    Shiny,
    /// Otra textura que carga el mapa (p. ej. `BITMAP_CHROME+2` = `Object9\sand01.jpg` en Tarkan,
    /// `MapManager.cpp:150`): ruta relativa a `Data` y si es `GL_NEAREST` + `GL_CLAMP_TO_EDGE`.
    File { path: &'static str, nearest_clamp: bool },
}

impl Bitmap {
    /// Ruta relativa a `Data` (con la extensión del original: `.jpg` = `.OZJ`, `.tga` = `.OZT`).
    pub fn path(self) -> &'static str {
        match self {
            Bitmap::Chrome => "Effect/Chrome01.jpg",
            Bitmap::Chrome2 => "Effect/Chrome02.jpg",
            Bitmap::Chrome3 => "Effect/Chrome03.jpg",
            Bitmap::Chrome6 => "Effect/Chrome06.jpg",
            Bitmap::Chrome7 => "Effect/Chrome07.jpg",
            Bitmap::Shiny => "Effect/Shiny01.jpg",
            Bitmap::File { path, .. } => path,
        }
    }

    /// `GL_NEAREST` y `GL_CLAMP_TO_EDGE` (si no, `GL_LINEAR`; `GL_REPEAT` salvo `Shiny`).
    pub fn nearest(self) -> bool {
        match self {
            Bitmap::Chrome2 | Bitmap::Chrome3 | Bitmap::Chrome6 | Bitmap::Chrome7 => true,
            Bitmap::File { nearest_clamp, .. } => nearest_clamp,
            Bitmap::Chrome | Bitmap::Shiny => false,
        }
    }

    pub fn clamp(self) -> bool {
        self.nearest() || self == Bitmap::Shiny
    }
}

/// Fórmula de las coordenadas de cromado (`g_chrome`) de `RenderMesh` (`ZzzBMD.cpp:1054-1108`).
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
    /// `RENDER_METAL` (y `RENDER_OIL`, que no se traduce): la rama `else`.
    Metal = 8,
}

/// Qué fórmula usa una combinación de `RENDER_*`, en el orden de los `if` del original.
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

/// `g_chrome[j]` de una normal ya girada (`NormalTransform`, ejes de MU) con `WorldTime` en ms.
///
/// No depende de la cámara: `BMD::Chrome` (el que usaba `g_vright`) no lo llama nadie. Todas son
/// lineales en la normal, así que interpolarlas por píxel da lo mismo que por vértice. La versión del
/// sombreador (`model_light.wesl`) es copia de esta.
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

/// `k · ((int)WorldTime % periodo)`, con `WorldTime` en ms: los `BlendMeshTexCoordU/V` de `MoveObject`
/// (p. ej. `-(int)WorldTime%1000 * 0.001f` = `Saw { period: 1000, k: -0.001 }`). `k = 0`: nada.
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

/// Una llamada a `RenderBody` (`mesh: None`) o `RenderMesh(mesh)` del código de dibujo de un objeto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pass {
    /// `None` = `RenderBody`: todas las mallas, con los guiones de textura (`_H` oculta, `_R` suma) y
    /// sin `HiddenMesh`. `Some(i)` = `RenderMesh(i, ...)`: los guiones `_H`/`_R` no cuentan.
    pub mesh: Option<usize>,
    /// `RENDER_*` ([`flags`]).
    pub flags: u32,
    /// `MeshTexture` / `Texture`: en lugar de la textura de la malla.
    pub texture: Option<Bitmap>,
    /// `b->BodyLight` durante la llamada (`None` = el del objeto: la luz del terreno).
    pub light: Option<[f32; 3]>,
    /// `b->StreamMesh` durante la llamada (`None` = el del modelo, [`super::ModelRules::stream_mesh`]).
    pub stream_mesh: Option<i32>,
    /// El parámetro `BlendMesh` (`None` = `o->BlendMesh`, [`super::ModelRules::additive_mesh`]).
    /// Se compara con el índice de textura de la malla; `<= -2` = todas suman.
    pub blend_mesh: Option<i32>,
    /// El parámetro `BlendMeshTexCoordU/V` (`None` = los de `o`, [`super::ModelRules::tex_coord`]).
    pub tex_coord: Option<[Saw; 2]>,
}

impl Pass {
    /// `RenderBody(flags, ...)`.
    pub const fn body(flags: u32) -> Self {
        Self { mesh: None, flags, texture: None, light: None, stream_mesh: None, blend_mesh: None, tex_coord: None }
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
}

/// La pasada por defecto: `RenderBody(RENDER_TEXTURE, o->Alpha, o->BlendMesh, ...)`.
pub const DEFAULT_PASSES: &[Pass] = &[Pass::body(flags::TEXTURE)];

/// Mezcla con la que queda una malla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookBlend {
    /// `DisableAlphaBlend`.
    Opaque,
    /// `EnableAlphaTest`: mezcla + prueba de alfa (> 0.25), escribe profundidad.
    AlphaTest,
    /// `EnableAlphaBlend`: ONE, ONE (suma), sin escribir profundidad.
    Add,
    /// `EnableAlphaBlendMinus`: ZERO, ONE_MINUS_SRC_COLOR (oscurece), sin escribir profundidad.
    Subtract,
}

/// Color con el que se multiplica la textura.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookColor {
    /// `LightTransform`: `BodyLight` por la intensidad según la normal.
    Lit,
    /// `glColor3fv(BodyLight)`: la `StreamMesh` y el cromado.
    Flat,
    /// `glColor3f(BodyLight · BlendMeshLight)`: la `BlendMesh`.
    BlendLight,
}

/// Cómo queda una malla en una pasada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshLook {
    /// `None` = la textura de la malla.
    pub texture: Option<Bitmap>,
    /// `false` = `DisableTexture` (solo color).
    pub textured: bool,
    /// Coordenadas de cromado en vez de las del modelo.
    pub chrome: Option<ChromeMode>,
    /// Se suman `BlendMeshTexCoordU/V` a las coordenadas (`EnableWave`, o `RENDER_CHROME4`).
    pub wave: Option<[Saw; 2]>,
    pub color: LookColor,
    /// `BodyLight` de la pasada (`None` = el del objeto).
    pub light: Option<[f32; 3]>,
    pub blend: LookBlend,
}

/// Lo que hace falta saber de una malla para decidir cómo se dibuja.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshInfo {
    pub index: usize,
    /// `m->Texture`.
    pub texture_index: i32,
    pub script: Option<TextureScript>,
    /// `pBitmap->Components == 4` de la textura de la malla (TGA con alfa).
    pub has_alpha: bool,
}

/// Lo del objeto que usan todas sus pasadas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectDraw {
    /// `o->BlendMesh` (`-1` = ninguna).
    pub blend_mesh: i32,
    /// `Models[tipo].StreamMesh` (`-1` = ninguna).
    pub stream_mesh: i32,
    /// `o->BlendMeshTexCoordU/V`.
    pub tex_coord: [Saw; 2],
    /// `o->HiddenMesh` (`-1` = ninguna).
    pub hidden_mesh: i32,
}

/// Cómo queda la malla `m` en la pasada `p` (`None` = esa llamada no la dibuja).
pub fn resolve(p: &Pass, o: &ObjectDraw, m: &MeshInfo) -> Option<MeshLook> {
    use flags::*;
    let i = m.index as i32;
    let mut blend_mesh = p.blend_mesh.unwrap_or(o.blend_mesh);
    match p.mesh {
        // `RenderBody`: `_H` y `HiddenMesh` no se dibujan; `_R` hace a la malla su `BlendMesh`.
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
    // El guion `_S` solo activa el desplazamiento; la luz plana es de `StreamMesh` del modelo.
    let stream = if m.script.is_some_and(|s| s.stream) { i } else { model_stream };
    let wave = ((i == blend_mesh || i == stream) && has_offset).then_some(tex_coord);
    let lit = if i == model_stream { LookColor::Flat } else { LookColor::Lit };
    // `pBitmap`: la textura que se usa al final.
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
    };

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
        // `BindTexture`: con `MeshTexture == -1` la de cada fórmula; si no, la pedida.
        let texture = match p.texture {
            Some(t) => Some(t),
            None if f & (CHROME2 | CHROME3 | CHROME4) != 0 => Some(Bitmap::Chrome2),
            None if f & CHROME6 != 0 => Some(Bitmap::Chrome6),
            None if f & CHROME != 0 => Some(Bitmap::Chrome),
            None if f & METAL != 0 => Some(Bitmap::Shiny),
            None => None,
        };
        // Solo `RENDER_CHROME4` suma `BlendMeshTexCoordU/V` (`ZzzBMD.cpp:1301`).
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
        } else if has_alpha {
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
    // Sin `RENDER_TEXTURE` ni nada: el original dibuja con la textura y la mezcla que hubiera.
    Some(look(p.texture, if has_alpha { LookBlend::AlphaTest } else { LookBlend::Opaque }, lit, wave))
}

/// Las pasadas de cada malla, en orden, quitando las que una pasada posterior opaca sobre la misma
/// malla tapa del todo (con `GL_LEQUAL` lo último dibujado gana): `out[k]` = (índice de pasada, aspecto).
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
        // RENDER_CHROME en WorldTime = 2500 ms: Wave = 0.25.
        let uv = chrome_uv(ChromeMode::Chrome, [0.0, 0.6, 0.8], 2500.0);
        assert!((uv[0] - (0.4 + 0.25)).abs() < 1e-6 && (uv[1] - (0.3 + 0.5)).abs() < 1e-6, "{uv:?}");
        // Wave = (int)WorldTime % 10000 · 0.0001: a los 10 s vuelve a empezar.
        assert_eq!(chrome_uv(ChromeMode::Chrome, [0.0, 0.0, 1.0], 10_000.0), [0.5, 0.0]);
        // CHROME3: LightVector (0, -0.1, -0.8).
        let uv = chrome_uv(ChromeMode::Chrome3, [0.0, 0.0, -1.0], 0.0);
        assert!((uv[0] - 0.8).abs() < 1e-6 && (uv[1] - 0.2).abs() < 1e-6);
        // METAL: sin tiempo.
        assert_eq!(chrome_uv(ChromeMode::Metal, [0.0, 0.0, 0.0], 1234.0), [0.2, 0.5]);
        // Orden de los `if`: CHROME2 gana a CHROME; BRIGHT solo no es cromado.
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
        // La BlendMesh suma con BodyLight·BlendMeshLight.
        let l = resolve(&body, &o, &mesh(4, None, false)).unwrap();
        assert_eq!((l.blend, l.color), (LookBlend::Add, LookColor::BlendLight));
        // TGA: prueba de alfa; JPG: opaca.
        assert_eq!(resolve(&body, &o, &mesh(1, None, true)).unwrap().blend, LookBlend::AlphaTest);
        assert_eq!(resolve(&body, &o, &mesh(1, None, false)).unwrap().blend, LookBlend::Opaque);
        // Con textura de cromado (JPG) en lugar de la TGA: opaca.
        let p = Pass::body(TEXTURE).texture(Bitmap::Chrome);
        assert_eq!(resolve(&p, &o, &mesh(1, None, true)).unwrap().blend, LookBlend::Opaque);
        // Guiones: `_H` no se dibuja en RenderBody, sí con RenderMesh; `_R` suma solo en RenderBody.
        let h = Some(TextureScript { hidden: true, ..Default::default() });
        assert!(resolve(&body, &obj(), &mesh(0, h, false)).is_none());
        assert!(resolve(&Pass::mesh(0, TEXTURE), &obj(), &mesh(0, h, false)).is_some());
        let r = Some(TextureScript { bright: true, ..Default::default() });
        assert_eq!(resolve(&body, &obj(), &mesh(2, r, false)).unwrap().blend, LookBlend::Add);
        assert_eq!(resolve(&Pass::mesh(2, TEXTURE), &obj(), &mesh(2, r, false)).unwrap().blend, LookBlend::Opaque);
        // `_N`: sin cromado.
        let n = Some(TextureScript { none_blend: true, ..Default::default() });
        assert!(resolve(&Pass::body(CHROME | BRIGHT), &obj(), &mesh(0, n, false)).is_none());
        // Cromado + BRIGHT: suma con BITMAP_CHROME y color plano.
        let l = resolve(&Pass::mesh(0, CHROME | BRIGHT), &obj(), &mesh(0, None, false)).unwrap();
        assert_eq!((l.blend, l.color, l.texture, l.chrome), (LookBlend::Add, LookColor::Flat, Some(Bitmap::Chrome), Some(ChromeMode::Chrome)));
        // DARK: resta.
        assert_eq!(resolve(&Pass::body(TEXTURE | DARK), &obj(), &mesh(0, None, false)).unwrap().blend, LookBlend::Subtract);
    }

    #[test]
    fn stream_mesh() {
        let saw = [Saw { period: 1000.0, k: -0.001 }, Saw::ZERO];
        let o = ObjectDraw { tex_coord: saw, ..obj() };
        // StreamMesh del modelo: luz plana y se desplaza.
        let p = Pass::body(TEXTURE).stream(2);
        let l = resolve(&p, &o, &mesh(2, None, false)).unwrap();
        assert_eq!((l.color, l.wave), (LookColor::Flat, Some(saw)));
        let l = resolve(&p, &o, &mesh(1, None, false)).unwrap();
        assert_eq!((l.color, l.wave), (LookColor::Lit, None));
        // Guion `_S`: se desplaza pero sigue iluminada.
        let s = Some(TextureScript { stream: true, ..Default::default() });
        let l = resolve(&Pass::body(TEXTURE), &o, &mesh(1, s, false)).unwrap();
        assert_eq!((l.color, l.wave), (LookColor::Lit, Some(saw)));
    }

    #[test]
    fn la_pasada_opaca_tapa_las_anteriores() {
        let passes = [Pass::body(TEXTURE).texture(Bitmap::Chrome).stream(2), Pass::body(TEXTURE)];
        // Malla JPG: la segunda la tapa del todo.
        assert_eq!(resolve_all(&passes, &obj(), &mesh(0, None, false)).len(), 1);
        // Malla TGA: el cromado se ve por los huecos de la segunda.
        let both = resolve_all(&passes, &obj(), &mesh(2, None, true));
        assert_eq!(both.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![0, 1]);
    }
}
