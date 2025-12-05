mod app;
mod capture;

fn main() {
    pretty_env_logger::init();
    app::run();
}
