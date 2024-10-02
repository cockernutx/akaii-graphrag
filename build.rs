use std::{env, fs::read_dir, path::{Path, PathBuf}};

fn recurse_protos(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let Ok(entries) = read_dir(path) else { return vec![] };
    entries.flatten().flat_map(|entry| {
        let Ok(meta) = entry.metadata() else { return vec![] };
        if meta.is_dir() { return recurse_protos(entry.path()); }
        if meta.is_file() { return vec![entry.path()]; }
        vec![]
    }).collect()
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = recurse_protos("./protos");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    tonic_build::configure().compile_well_known_types(true).build_client(false).file_descriptor_set_path(out_dir.join("descriptor.bin")).type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]").compile_protos(&protos, &["./protos"])?;
    Ok(())
}