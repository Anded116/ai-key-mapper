//! aikm-import <preset.txt> [-o profile.toml] [--game NAME] [--exe GAME.exe]
//!
//! Converts a JoyShockMapper preset into an aikm profile. Without -o the profile goes to stdout;
//! the summary always goes to stderr.

use std::path::PathBuf;

use jsm_import::{import, Options};

fn usage() -> ! {
    eprintln!("usage: aikm-import <preset.txt> [-o profile.toml] [--game NAME] [--exe GAME.exe]");
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut input, mut output) = (None, None);
    let mut opts = Options::default();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--out" => output = Some(PathBuf::from(args.next().unwrap_or_else(|| usage()))),
            "--game" => opts.game = Some(args.next().unwrap_or_else(|| usage())),
            "--exe" => opts.exe = Some(args.next().unwrap_or_else(|| usage())),
            _ if a.starts_with('-') => usage(),
            _ if input.is_none() => input = Some(PathBuf::from(a)),
            _ => usage(),
        }
    }
    let Some(input) = input else { usage() };
    let imported = import(&input, &opts).unwrap_or_else(|e| {
        eprintln!("aikm-import: {e}");
        std::process::exit(1);
    });
    match &output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &imported.toml) {
                eprintln!("aikm-import: {}: {e}", path.display());
                std::process::exit(1);
            }
        }
        None => print!("{}", imported.toml),
    }
    let p = &imported.profile;
    let layered: usize = p.layers.iter().map(|l| l.bindings.len()).sum();
    eprintln!(
        "{}: кнопок {}, в слоях {}, не перенесено строк {}{}",
        p.game,
        p.buttons.len(),
        layered,
        imported.skipped.len(),
        output.map_or(String::new(), |o| format!(" → {}", o.display()))
    );
}
