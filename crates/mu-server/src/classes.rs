//! Clases: estadísticas al crear, mapa de inicio y fórmulas de vida, maná, AG y escudo.
//!
//! Todo sale de OpenMU (`Persistence/Initialization/CharacterClasses/Class*.cs`, licencia MIT):
//! `BaseAttributeValues` (constantes) más `AttributeCombinations` (relaciones lineales con nivel y
//! estadísticas). El escudo además suma `nivel² / 30` y la defensa (0 sin equipo).

/// Índice de clase del cliente (`CLASS_*`): 0 Dark Wizard ... 6 Rage Fighter.
pub type ClassIndex = u8;

pub const MAX_CLASS: ClassIndex = 7;

/// Fuerza, agilidad, vitalidad, energía y mando (solo el Dark Lord).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct Stats {
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub energy: u16,
    pub leadership: u16,
}

/// Lo de cada clase al crear un personaje.
pub struct ClassInfo {
    pub stats: Stats,
    /// `CurrentHealth` / `CurrentMana` al crear.
    pub health: u16,
    pub mana: u16,
    /// `HomeMap`: 0 Lorencia, 3 Noria, 51 Elbeland.
    pub home_map: u8,
    /// Puntos por nivel (para cuando se suba de nivel).
    #[allow(dead_code)]
    pub points_per_level: u16,
}

pub fn info(class: ClassIndex) -> ClassInfo {
    let s = |strength, agility, vitality, energy, leadership| Stats { strength, agility, vitality, energy, leadership };
    match class {
        0 => ClassInfo { stats: s(18, 18, 15, 30, 0), health: 60, mana: 60, home_map: 0, points_per_level: 5 },
        1 => ClassInfo { stats: s(28, 20, 25, 10, 0), health: 110, mana: 20, home_map: 0, points_per_level: 5 },
        2 => ClassInfo { stats: s(22, 25, 20, 15, 0), health: 80, mana: 30, home_map: 3, points_per_level: 5 },
        3 => ClassInfo { stats: s(26, 26, 26, 26, 0), health: 110, mana: 60, home_map: 0, points_per_level: 7 },
        4 => ClassInfo { stats: s(26, 20, 20, 15, 25), health: 90, mana: 40, home_map: 0, points_per_level: 7 },
        5 => ClassInfo { stats: s(21, 21, 18, 23, 0), health: 70, mana: 40, home_map: 51, points_per_level: 5 },
        _ => ClassInfo { stats: s(32, 27, 25, 20, 0), health: 100, mana: 40, home_map: 0, points_per_level: 7 },
    }
}

/// Máximos de vida, maná, escudo y AG para un nivel y unas estadísticas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Maximums {
    pub health: u16,
    pub mana: u16,
    pub shield: u16,
    pub ability: u16,
}

pub fn maximums(class: ClassIndex, level: u16, st: &Stats) -> Maximums {
    let (l, s, a, v, e, c) =
        (level as f32, st.strength as f32, st.agility as f32, st.vitality as f32, st.energy as f32, st.leadership as f32);
    // (constante de vida, × nivel, × vitalidad), (constante de maná, × energía, × nivel), AG (× energía, × vitalidad, × agilidad, × fuerza, × mando).
    let (hp, mp, ag) = match class {
        0 => ((30.0, 1.0, 2.0), (0.0, 2.0, 2.0), (0.2, 0.3, 0.4, 0.2, 0.0)),
        1 => ((35.0, 2.0, 3.0), (10.0, 1.0, 0.5), (1.0, 0.3, 0.2, 0.15, 0.0)),
        2 => ((39.0, 1.0, 2.0), (6.0, 1.5, 1.5), (0.2, 0.3, 0.2, 0.3, 0.0)),
        3 => ((57.0, 1.0, 2.0), (7.0, 2.0, 1.0), (0.15, 0.3, 0.25, 0.2, 0.0)),
        4 => ((48.5, 1.5, 2.0), (38.0, 0.0, 1.0), (0.15, 0.1, 0.2, 0.3, 0.3)),
        5 => ((39.0, 1.0, 2.0), (6.0, 1.7, 1.5), (0.15, 0.3, 0.25, 0.2, 0.0)),
        _ => ((57.0, 1.3, 2.0), (7.0, 1.3, 1.0), (1.0, 0.3, 0.2, 0.15, 0.0)),
    };
    let health = hp.0 + hp.1 * l + hp.2 * v;
    let mana = mp.0 + mp.1 * e + mp.2 * l;
    let ability = ag.0 * e + ag.1 * v + ag.2 * a + ag.3 * s + ag.4 * c;
    let shield = 1.2 * (s + a + v + e + if class == 4 { c } else { 0.0 }) + l * l / 30.0;
    let u = |x: f32| x.clamp(0.0, u16::MAX as f32) as u16;
    Maximums { health: u(health), mana: u(mana), shield: u(shield), ability: u(ability) }
}

/// Experiencia para pasar de `level` al siguiente (`GetExperienceForLevel` de OpenMU, S6:
/// `(nivel + 9) · nivel² · 10`, y desde el 256 se añade la parte extra).
pub fn experience_for_level(level: u16) -> u64 {
    let l = level as u64;
    let mut exp = (l + 9) * l * l * 10;
    if l > 255 {
        let k = l - 255;
        exp += (k + 9) * k * k * 1000;
    }
    exp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_knight_nivel_1() {
        let i = info(1);
        let m = maximums(1, 1, &i.stats);
        // 35 + 2 + 75 = 112 de vida; 10 + 10 + 0.5 = 20 de maná.
        assert_eq!((m.health, m.mana), (112, 20));
    }

    #[test]
    fn experiencia() {
        assert_eq!(experience_for_level(1), 100);
        assert_eq!(experience_for_level(2), 440);
    }
}
