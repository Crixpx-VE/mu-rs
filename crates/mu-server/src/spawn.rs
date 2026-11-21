//! Dónde aparece un personaje nuevo: una casilla al azar de la zona segura de su mapa de inicio
//! (la ciudad), sacada del terreno del cliente (`EncTerrainN.att`: zona segura y se puede andar).

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use mu_data::terrain::{self, attr};

pub struct Spawns {
    tiles: HashMap<u8, Vec<(u8, u8)>>,
    seed: AtomicU64,
}

/// Mapas de inicio de las clases (Lorencia, Noria, Elbeland).
const HOME_MAPS: [u8; 3] = [0, 3, 51];

impl Spawns {
    pub fn load(data: &Path) -> Self {
        let mut tiles = HashMap::new();
        for map in HOME_MAPS {
            match terrain::load(data, map as u16 + 1) {
                Ok(t) => {
                    let list: Vec<(u8, u8)> = (0..terrain::SIZE)
                        .flat_map(|y| (0..terrain::SIZE).map(move |x| (x, y)))
                        .filter(|&(x, y)| t.walkable(x, y) && t.attribute(x, y) & attr::SAFE_ZONE != 0)
                        .map(|(x, y)| (x as u8, y as u8))
                        .collect();
                    eprintln!("[spawn] mapa {map}: {} casillas de zona segura", list.len());
                    tiles.insert(map, list);
                }
                Err(e) => eprintln!("[spawn] sin terreno del mapa {map} ({e}): se aparece en (130, 130)"),
            }
        }
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
        Self { tiles, seed: AtomicU64::new(seed) }
    }

    /// Una casilla de la zona segura de `map`.
    pub fn pick(&self, map: u8) -> (u8, u8) {
        let Some(list) = self.tiles.get(&map).filter(|l| !l.is_empty()) else { return (130, 130) };
        // xorshift64.
        let mut x = self.seed.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.store(x, Ordering::Relaxed);
        list[(x % list.len() as u64) as usize]
    }
}
