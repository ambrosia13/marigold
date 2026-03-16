use env_logger::Env;

mod app;
mod egui;
mod util;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn"))
        .filter_module("marigold", log::LevelFilter::Info)
        .init();

    app::run();
}
