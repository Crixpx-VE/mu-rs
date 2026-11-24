//! Effect model files (`.bmd`): where each `MODEL_*` lives and a cache of the parsed files for the
//! simulation (an effect model's own bones, mesh vertices and action lengths, which the original
//! reads from `Models[o->Type]`). The drawing side (`models.rs`) uses the same [`locate`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::fs;

use super::core::*;
use super::tables::models::{MAP_MODELS, MODELS};

/// Folder, file and texture folder of model `id` on map `world_active` (the original's
/// numbering, Lorencia = 0): the per-map loads of `CMapManager::Load` first (the same id can be
/// another file on another map), then the global table, then the cases the generator does not see.
pub fn locate(id: i32, world_active: i32) -> Option<(String, String, String)> {
    if let Some(&(_, _, d, f, t)) = MAP_MODELS.iter().find(|r| r.0 == world_active && r.1 == id) {
        return Some((d.to_string(), f.to_string(), t.to_string()));
    }
    if let Some(&(_, d, f, t)) = MODELS.iter().find(|r| r.0 == id) {
        return Some((d.to_string(), f.to_string(), t.to_string()));
    }
    // Map object models used by effects (`OpenWorldModels` / `MapManager`).
    let fixed = match id {
        MODEL_WARP..=MODEL_WARP3 => Some(("Npc", format!("warp{:02}.bmd", id - MODEL_WARP + 1))),
        // Raklion (`MapManager.cpp:769-774`): the same models under other numbers.
        MODEL_WARP4..=MODEL_WARP6 => Some(("Npc", format!("warp{:02}.bmd", id - MODEL_WARP4 + 1))),
        MODEL_DUNGEON_STONE01 => Some(("Object2", "DungeonStone01.bmd".into())),
        MODEL_CLOUD => Some(("Object11", "cloud.bmd".into())),
        // Kalima `CreateEffect(9)`: map model 9 (`Object25`, the same in all 7 Kalimas).
        MODEL_KALIMA_FALLING_STONE => Some(("Object25", "Object10.bmd".into())),
        // `MapManager.cpp:293-294` (and other maps): `Object1\Butterfly01`.
        MODEL_BUTTERFLY01 => Some(("Object1", "Butterfly01.bmd".into())),
        _ => None,
    };
    if let Some((dir, file)) = fixed {
        return Some((dir.into(), file, dir.into()));
    }
    if (MODEL_STONE1..=MODEL_STONE2).contains(&id) {
        return Some(("Skill".into(), format!("Stone{:02}.bmd", id - MODEL_STONE1 + 1), "Skill".into()));
    }
    if (MODEL_ITEM..MODEL_ITEM + 16 * 512).contains(&id) {
        const GROUPS: [&str; 7] = ["Sword", "Axe", "Mace", "Spear", "Bow", "Staff", "Shield"];
        let rel = id - MODEL_ITEM;
        let name = GROUPS.get((rel / 512) as usize)?;
        return Some(("Item".into(), format!("{name}{:02}.bmd", rel % 512 + 1), "Item".into()));
    }
    None
}

/// Parsed effect models for the simulation, loaded the first time a kind asks for them.
#[derive(Resource, Default)]
pub struct ModelData {
    /// The client's `Data` folder (`None` in tests: nothing is loaded).
    data: Option<PathBuf>,
    world: i32,
    cache: HashMap<i32, Option<Arc<Bmd>>>,
}

impl ModelData {
    pub fn new(data: PathBuf) -> Self {
        Self { data: Some(data), world: 0, cache: HashMap::new() }
    }

    /// On another map the per-map models change: forget what was loaded.
    pub fn set_world(&mut self, world_active: i32) {
        if self.world != world_active {
            self.world = world_active;
            self.cache.clear();
        }
    }

    /// `Models[id]`, if its file exists.
    pub fn get(&mut self, id: i32) -> Option<Arc<Bmd>> {
        if let Some(m) = self.cache.get(&id) {
            return m.clone();
        }
        let world = self.world;
        let m = self.data.as_ref().and_then(|data| {
            let (dir, file, _) = locate(id, world)?;
            match fs::read(&data.join(&dir), &file).and_then(|b| bmd::read(&b)) {
                Ok(b) => Some(Arc::new(b)),
                Err(e) => {
                    warn!("efecto: no se pudo cargar el modelo {id} ({dir}/{file}): {e}");
                    None
                }
            }
        });
        self.cache.insert(id, m.clone());
        m
    }

    /// Only what is already loaded (for draw functions, which cannot load).
    pub fn loaded(&self, id: i32) -> Option<Arc<Bmd>> {
        self.cache.get(&id).cloned().flatten()
    }
}

/// `BMD::PlayAnimation` (ZzzBMD.cpp:406-478) for an effect whose model has `frames` keys in the
/// current action. Returns `false` when a non-looping action wraps (or a `Loop` one reaches its
/// end), like the original. `locked` = `Actions[].LockPositions`, `hold` = `Actions[].Loop` (the
/// original's name for "stay on the last key").
pub fn play_animation(frame: &mut f32, speed: f32, frames: usize, locked: bool, hold: bool) -> bool {
    if frames <= 1 {
        return true;
    }
    *frame += speed;
    if *frame <= 0.0 {
        *frame += frames as f32 - 1.0;
    }
    if hold {
        if *frame >= frames as f32 {
            *frame = frames as f32 - 0.01;
            return false;
        }
    } else {
        let key = if locked { frames - 1 } else { frames }.max(1);
        // TODO(original): `SceneFlag == 4` (+2) and Kanturu 3rd `MONSTER01_APEAR` (+1) offsets
        // only apply to characters, not to effect models.
        if *frame >= key as f32 {
            let f = frame.floor();
            *frame = (f as usize % key) as f32 + (*frame - f);
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_looping_action_wraps_and_reports_the_end() {
        let mut f = 3.5;
        assert!(!play_animation(&mut f, 1.0, 4, false, false));
        assert!((f - 0.5).abs() < 1e-5);
        let mut f = 1.0;
        assert!(play_animation(&mut f, 1.0, 4, false, false));
        assert_eq!(f, 2.0);
    }

    #[test]
    fn loop_flag_holds_the_last_key() {
        let mut f = 3.5;
        assert!(!play_animation(&mut f, 1.0, 4, false, true));
        assert!((f - 3.99).abs() < 1e-5);
    }

    #[test]
    fn per_map_models_come_first() {
        // MODEL_ARROW_TANKER is only loaded by Crywolf (`MapManager.cpp:192`).
        assert!(locate(MODEL_ARROW_TANKER, 34).is_some());
        assert!(locate(MODEL_STORM, 0).is_some());
    }
}
