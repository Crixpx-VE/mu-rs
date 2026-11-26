//! How long the file side of a map load takes (terrain, object list, object models and their
//! textures), without the game: `cargo run --release -p mu-data --example loadtime -- <Data> <world>...`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use mu_data::{bmd, fs, objects, terrain, texture};

fn main() {
    let mut args = std::env::args().skip(1);
    let data = PathBuf::from(args.next().expect("Data folder"));
    // What every character that appears reads (character.rs `load_character_dressed`).
    let t = Instant::now();
    let player = fs::read(&data.join("Player"), "player.bmd").and_then(|b| bmd::read(&b));
    let read_ms = t.elapsed().as_secs_f64() * 1000.0;
    if let Ok(p) = &player {
        let t = Instant::now();
        let _ = p.bone_matrices(0, 0);
        println!("player.bmd: read {read_ms:.1} ms, {} actions, one pose {:.2} ms", p.actions.len(), t.elapsed().as_secs_f64() * 1000.0);
    }
    for world in args.map(|w| w.parse::<u16>().expect("world number")) {
        measure(&data, world);
    }
}

fn measure(data: &Path, world: u16) {
    let t = Instant::now();
    let terrain = terrain::load(data, world);
    let terrain_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let list = objects::load(data, world).unwrap_or_default();
    let list_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut models: HashMap<PathBuf, Option<bmd::Bmd>> = HashMap::new();
    for o in &list {
        let Some(rel) = objects::model_path(world, o.kind) else { continue };
        models.entry(rel.clone()).or_insert_with(|| {
            let dir = data.join(rel.parent().unwrap_or(Path::new("")));
            let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
            fs::read(&dir, name).and_then(|b| bmd::read(&b)).ok()
        });
    }
    let models_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut textures = HashSet::new();
    let mut pixels = 0usize;
    for (rel, m) in &models {
        let Some(m) = m else { continue };
        let dir = data.join(rel.parent().unwrap_or(Path::new("")));
        for mesh in &m.meshes {
            let file = bmd::texture_file_name(&mesh.texture);
            if textures.insert((dir.clone(), file.clone()))
                && let Ok(tex) = texture::load(&dir, &file)
            {
                pixels += (tex.width * tex.height) as usize;
            }
        }
    }
    let textures_ms = t.elapsed().as_secs_f64() * 1000.0;

    println!(
        "World{world}: terrain {terrain_ms:.0} ms ({}), {} objects {list_ms:.0} ms, {} models {models_ms:.0} ms, {} textures ({:.1} Mpx) {textures_ms:.0} ms",
        if terrain.is_ok() { "ok" } else { "error" },
        list.len(),
        models.len(),
        textures.len(),
        pixels as f64 / 1e6,
    );
}
