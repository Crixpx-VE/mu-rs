//! Brillo de los objetos: las pasadas de `RenderPartObjectEffect` (ZzzObject.cpp:9327-10385) según el
//! nivel (+3 a +15), las opciones excelentes y los sets antiguos.
//!
//! Cada pasada es una llamada a `RenderPartObjectBody` / `RenderPartObjectBodyColor(2)` con sus
//! `RENDER_*` ([`crate::objects::render::flags`]) y su `b->BodyLight` ([`GlowLight`]); quien dibuja la
//! resuelve malla a malla con [`crate::objects::render::resolve`] como el resto de modelos. Va la rama
//! general: los objetos con dibujo propio al principio de la función (la lanza de Kundun, los
//! anillos de evento...) también la usan.

use super::group::*;
use super::{Item, kind};
use crate::objects::render::flags::*;

/// `b->BodyLight` de una pasada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlowLight {
    Fixed([f32; 3]),
    /// `g_Luminosity · k`, con `g_Luminosity = sin(WorldTime · 0.004) · 0.15 + 0.6` (ZzzScene.cpp:2765).
    Luminosity([f32; 3]),
    /// Excelente: `L = sin(WorldTime · 0.002) · 0.5 + 0.5` → `(L, 0.3 L, 1 − L)` (ZzzObject.cpp:10306-10307).
    Excellent,
    /// Set antiguo: `PartObjectColor3` por `Alpha = sin(WorldTime · 0.001) · 0.5 + 0.4`
    /// (`RENDER_CHROME3` con `Alpha` < 0.99 multiplica `BodyLight` por él, ZzzBMD.cpp:1110-1118).
    Ancient([f32; 3]),
}

impl GlowLight {
    /// El color con `WorldTime` en ms (OpenGL lo limita a [0, 1] al dibujar).
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

    /// Cambia con el tiempo.
    pub fn animated(self) -> bool {
        !matches!(self, GlowLight::Fixed(_))
    }
}

/// Una llamada de dibujo de todas las mallas (`RenderBody`) con sus `RENDER_*` y su luz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlowPass {
    pub flags: u32,
    pub light: GlowLight,
}

/// El nivel con el que se elige el brillo (`switch (Type)` del principio y el límite de efectos:
/// `min(Level, RenderLevel · 2 + 5)` con `GetRenderLevel()` < 4).
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
        // `MODEL_COMPILED_CELE` y `MODEL_COMPILED_SOUL` (_define.h:609-610).
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

/// `PartObjectColor(Type, 1, 1, ...)` (ZzzObject.cpp:6483-6770): el color del cromado.
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

/// `PartObjectColor2(Type, 1, 1, Light, ...)` (ZzzObject.cpp:6771-6854): sobre la luz que había.
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

/// `PartObjectColor3` (ZzzObject.cpp:6855-6892): el del brillo de los sets antiguos.
pub fn part_object_color3(item: &Item) -> [f32; 3] {
    let gold = (HELM..=BOOTS).contains(&item.group) && matches!(item.index, 3 | 9 | 17);
    if gold { [1.0, 0.7, 0.2] } else { [0.1, 0.6, 1.0] }
}

/// Las pasadas del objeto con la luz `light` del sitio (la del inventario es (1, 1, 1)) y el límite de
/// efectos de las opciones (`g_pOption->GetRenderLevel()`, 0 a 4).
pub fn item_passes(item: &Item, light: [f32; 3], render_level: u8) -> Vec<GlowPass> {
    let level = effect_level(item, render_level);
    let rl = render_level;
    let scaled = |k: f32| GlowLight::Fixed(light.map(|v| v * k));
    let body = |l: GlowLight| GlowPass { flags: TEXTURE, light: l };
    let color = GlowLight::Fixed(part_object_color(item));
    let chrome = GlowPass { flags: CHROME | BRIGHT, light: color };
    let metal = GlowPass { flags: METAL | BRIGHT, light: color };
    let color2 = GlowLight::Fixed(part_object_color2(item, light.map(|v| v * 0.9)));
    let mut out = match level {
        0..=2 => vec![body(GlowLight::Fixed(light))],
        3 | 4 => vec![body(GlowLight::Luminosity([light[0], light[1] * 0.6, light[2] * 0.6]))],
        5 | 6 => vec![body(GlowLight::Luminosity([light[0] * 0.5, light[1] * 0.7, light[2]]))],
        _ if rl == 0 => vec![body(GlowLight::Fixed(light))],
        7 | 8 if rl >= 1 => vec![body(scaled(0.8)), chrome],
        9 | 10 if rl >= 2 => vec![body(scaled(0.9)), chrome, metal],
        11 | 12 if rl >= 3 => vec![body(scaled(0.9)), GlowPass { flags: CHROME2 | BRIGHT, light: color2 }, metal, chrome],
        13..=15 if rl >= 4 => vec![body(scaled(0.9)), GlowPass { flags: CHROME4 | BRIGHT, light: color2 }, metal, chrome],
        _ => vec![body(GlowLight::Fixed(light))],
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
        out.push(GlowPass { flags: TEXTURE | BRIGHT, light: GlowLight::Excellent });
    } else if item.is_ancient() {
        out.push(GlowPass { flags: CHROME3 | BRIGHT, light: GlowLight::Ancient(part_object_color3(item)) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(group: u8, index: u16, level: u8) -> Item {
        Item { level, ..Item::new(group, index) }
    }

    #[test]
    fn pasadas_por_nivel() {
        let flags = |it: &Item| item_passes(it, [1.0; 3], 4).iter().map(|p| p.flags).collect::<Vec<_>>();
        assert_eq!(flags(&lv(SWORD, 0, 0)), vec![TEXTURE]);
        assert_eq!(flags(&lv(SWORD, 0, 4)), vec![TEXTURE]);
        assert_eq!(flags(&lv(SWORD, 0, 7)), vec![TEXTURE, CHROME | BRIGHT]);
        assert_eq!(flags(&lv(SWORD, 0, 9)), vec![TEXTURE, CHROME | BRIGHT, METAL | BRIGHT]);
        assert_eq!(flags(&lv(SWORD, 0, 11)), vec![TEXTURE, CHROME2 | BRIGHT, METAL | BRIGHT, CHROME | BRIGHT]);
        assert_eq!(flags(&lv(SWORD, 0, 15)), vec![TEXTURE, CHROME4 | BRIGHT, METAL | BRIGHT, CHROME | BRIGHT]);
        // +7 y más: la base a 0.8 o 0.9.
        assert_eq!(item_passes(&lv(SWORD, 0, 7), [1.0; 3], 4)[0].light, GlowLight::Fixed([0.8; 3]));
        assert_eq!(item_passes(&lv(SWORD, 0, 13), [1.0; 3], 4)[0].light, GlowLight::Fixed([0.9; 3]));
        // Excelente: una pasada más que suma la propia textura.
        let exc = Item { excellent: 4, ..lv(SWORD, 0, 9) };
        assert_eq!(item_passes(&exc, [1.0; 3], 4).last().unwrap(), &GlowPass { flags: TEXTURE | BRIGHT, light: GlowLight::Excellent });
        // Las alas no brillan por nivel ni por excelente.
        assert_eq!(flags(&Item { excellent: 1, ..lv(WING, 4, 13) }), vec![TEXTURE]);
        // Las joyas siempre como +8.
        assert_eq!(flags(&lv(POTION, 13, 0)), vec![TEXTURE, CHROME | BRIGHT]);
    }

    #[test]
    fn limite_de_efectos() {
        // Con `RenderLevel` 1 un +13 se queda en +7; con 0, en +5 (el azul que late) y sin excelente.
        assert_eq!(effect_level(&lv(SWORD, 0, 13), 1), 7);
        let exc = Item { excellent: 1, ..lv(SWORD, 0, 13) };
        assert_eq!(item_passes(&exc, [1.0; 3], 0), vec![GlowPass { flags: TEXTURE, light: GlowLight::Luminosity([0.5, 0.7, 1.0]) }]);
    }

    #[test]
    fn colores() {
        // Kris: color 0 (1, 0.5, 0); la armadura de dragón (índice 1): color 1 (1, 0.2, 0).
        assert_eq!(part_object_color(&Item::new(SWORD, 0)), [1.0, 0.5, 0.0]);
        assert_eq!(part_object_color(&Item::new(ARMOR, 1)), [1.0, 0.2, 0.0]);
        assert_eq!(part_object_color2(&Item::new(ARMOR, 4), [0.9; 3]), [0.9, 0.45, 0.0]);
        let ex = GlowLight::Excellent.at(0.0);
        assert_eq!(ex, [0.5, 0.15, 0.5]);
        assert!((GlowLight::Luminosity([1.0; 3]).at(0.0)[0] - 0.6).abs() < 1e-6);
    }
}
