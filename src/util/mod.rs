use std::path::{Path, PathBuf};

pub fn get_asset_path<P: AsRef<Path>>(asset_location: P) -> PathBuf {
    std::env::current_dir()
        .unwrap()
        .join("assets")
        .join(asset_location)
}

pub fn get_shader_path<P: AsRef<Path>>(shader_location: P) -> PathBuf {
    // remove the extension
    let shader_location = shader_location.as_ref().with_extension("");

    let mut path = get_asset_path("shaders/target");
    path.push(shader_location);

    path
}
