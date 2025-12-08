mod app;
mod capture;
mod converter;
mod encoder;
mod stream;
mod encoding_thread;

fn main() {
    pretty_env_logger::init();
    app::run();
}
