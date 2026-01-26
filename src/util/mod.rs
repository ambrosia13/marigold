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

pub fn get_spirv_source<P: AsRef<Path>>(shader_location: P) -> Vec<u32> {
    std::fs::read(get_shader_path(&shader_location))
        .map(|source| wgpu::util::make_spirv_raw(&source).to_vec())
        .unwrap_or_else(|_| {
            panic!(
                "unable to read spir-v source for {}",
                shader_location.as_ref().to_string_lossy()
            )
        })
}
