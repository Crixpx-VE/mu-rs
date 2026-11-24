//! Converts MU textures (.OZJ/.OZT) to PNG for inspection (debugging).
//!
//!   cargo run -p mu-data --example tex2png -- <output_dir> <file.OZJ|OZT>...

use std::path::Path;

use mu_data::texture;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(out), files) = (args.first(), &args[1.min(args.len())..]) else {
        eprintln!("uso: tex2png <carpeta_salida> <archivos...>");
        std::process::exit(2);
    };
    for f in files {
        let p = Path::new(f);
        let t = match texture::load(p.parent().unwrap(), p.file_name().unwrap().to_str().unwrap()) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{f}: {e}");
                continue;
            }
        };
        let alpha: Vec<u8> = t.rgba.chunks(4).map(|c| c[3]).collect();
        let (min, max) = (alpha.iter().min().unwrap(), alpha.iter().max().unwrap());
        let dest = Path::new(out).join(p.file_stem().unwrap()).with_extension("png");
        image::save_buffer(&dest, &t.rgba, t.width, t.height, image::ExtendedColorType::Rgba8).unwrap();
        println!("{f}: {}x{} alfa {min}..{max} {:?} -> {}", t.width, t.height, t.alpha_kind(), dest.display());
    }
}
