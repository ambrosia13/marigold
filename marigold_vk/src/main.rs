mod app;
mod util;
mod vulkan;
mod window;

use env_logger::Env;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn"))
        .filter_module("naga", log::LevelFilter::Warn) // force naga to only show warnings since it produces insane log spam
        // every package in our workspace should report full info
        .filter_module("bvh", log::LevelFilter::Info)
        .filter_module("mesh_interface", log::LevelFilter::Info)
        .filter_module("gltf_loading", log::LevelFilter::Info)
        .filter_module("marigold", log::LevelFilter::Info)
        .filter_module("marigold_vk", log::LevelFilter::Info)
        .init();

    window::run();
}
