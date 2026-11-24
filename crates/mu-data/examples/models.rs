//! Lists a map's models with their animations (for debugging).
//!
//!   cargo run -p mu-data --example models -- [world=1] [data=../referencias/MuMain/src/bin/Data]

use std::collections::BTreeMap;
use std::path::PathBuf;

use mu_data::{bmd, fs, objects};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world: u16 =args.first().and_then(|w| w.parse().ok()).unwrap_or(1);
    let data = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("../referencias/MuMain/src/bin/Data"));

    let list = objects::load(&data, world).expect("no se pudo leer el .obj");
    let mut count: BTreeMap<i16, usize> = BTreeMap::new();
    for o in &list {
        *count.entry(o.kind).or_default() += 1;
    }
    for (kind, n) in count {
        let Some(rel) = objects::model_path(world, kind) else { continue };
        let dir = data.join(rel.parent().unwrap());
        let name = rel.file_name().unwrap().to_str().unwrap();
        match fs::read(&dir, name).and_then(|b| bmd::read(&b)) {
            Ok(m) => {
                let frames: Vec<usize> = m.actions.iter().map(|a| a.frames).collect();
                let meshes: Vec<&str> = m.meshes.iter().map(|x| x.texture.as_str()).collect();
                println!("{kind:4} x{n:<4} {:24} bones {:3} actions {frames:?} meshes {meshes:?}", m.name, m.bones.len());
            }
            Err(e) => println!("{kind:4} x{n:<4} {rel:?}: {e}"),
        }
    }
}
