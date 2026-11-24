use image::imageops::FilterType;
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = root.join("rules/desktop-jungle-v2.json");
    println!("cargo:rerun-if-changed={}", profile.display());
    let hash = format!("{:x}", Sha256::digest(fs::read(profile).unwrap()));
    assert_eq!(
        hash,
        "a457bbc009e975a9eafe5ff1f8fa40ca2548f2a689b121c8c4ed99dd0f3fdc57"
    );
    let art = root.join("src/assets/game");
    println!("cargo:rerun-if-changed={}", art.display());
    let mut names: Vec<String> = [
        "land_tile",
        "river_tile",
        "blue_trap_tile",
        "red_trap_tile",
        "blue_den_tile",
        "red_den_tile",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    for side in ["blue", "red"] {
        for animal in [
            "rat", "cat", "dog", "wolf", "leopard", "tiger", "lion", "elephant",
        ] {
            names.push(format!("{side}_{animal}"));
        }
    }
    let mut atlas = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let png = image::open(art.join(format!("{name}.png")))
            .unwrap()
            .to_rgba8();
        let (w, h) = if i < 6 { (64, 64) } else { (50, 50) };
        atlas.extend(image::imageops::resize(&png, w, h, FilterType::Lanczos3).into_raw());
    }
    fs::write(out.join("atlas.rgba"), atlas).unwrap();
}
