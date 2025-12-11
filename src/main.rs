mod app;
mod capture;
mod converter;
mod encoder;
mod stream;
mod resample;

fn main() {
    pretty_env_logger::init();
    app::run();
}
