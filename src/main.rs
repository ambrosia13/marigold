#![feature(iter_array_chunks)]
#![allow(clippy::too_many_arguments)]

use env_logger::Env;

mod app;
mod egui;
mod util;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn"))
        .filter_module("naga", log::LevelFilter::Warn) // force naga to only show warnings since it produces insane log spam
        .filter_module("marigold", log::LevelFilter::Info)
        .init();

    app::run();
}
