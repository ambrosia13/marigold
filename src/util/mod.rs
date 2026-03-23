use std::path::{Path, PathBuf};

use glam::UVec3;

pub mod buffer;

pub fn get_asset_root() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        // running via cargo, binary is in manifest_dir/target/debug or manifest_dir/target/release, but assets is in manifest_dir/assets
        PathBuf::from(manifest_dir)
    } else {
        // running the binary directly, assets is expected to be in the same directory
        let mut path = std::env::current_exe().unwrap();
        assert!(path.pop());

        path
    }
}

pub fn get_asset_path<P: AsRef<Path>>(asset_location: P) -> PathBuf {
    get_asset_root().join("assets").join(&asset_location)
}

pub fn get_shader_path<P: AsRef<Path>>(shader_location: P) -> PathBuf {
    // remove the extension
    let shader_location = shader_location.as_ref().with_extension("");

    let mut path = get_asset_path("shaders/target");
    path.push(&shader_location);

    log::info!(
        "Shader path '{}' resolves to '{}'",
        shader_location.to_string_lossy(),
        path.to_string_lossy()
    );

    path
}

pub fn get_spirv_source<P: AsRef<Path>>(shader_location: P) -> Vec<u32> {
    std::fs::read(get_shader_path(&shader_location))
        .map(|source| wgpu::util::make_spirv_raw(&source).to_vec())
        .unwrap_or_else(|err| {
            panic!(
                "unable to read spir-v source for {}; error: {}",
                shader_location.as_ref().to_string_lossy(),
                err
            )
        })
}

pub fn get_workgroup_count_from_size(workgroup_size: UVec3, dimensions: UVec3) -> UVec3 {
    (dimensions + workgroup_size - UVec3::ONE) / workgroup_size
}

// returns new number + unit
pub fn display_byte_size(bytes: usize) -> (f64, &'static str) {
    if bytes < 1024 {
        (bytes as f64, "B")
    } else if bytes < 1024 * 1024 {
        (bytes as f64 / 1024.0, "KiB")
    } else if bytes < 1024 * 1024 * 1024 {
        (bytes as f64 / (1024.0 * 1024.0), "MiB")
    } else {
        (bytes as f64 / (1024.0 * 1024.0 * 1024.0), "GiB")
    }
}
