export def --wrapped run [app: string, copy: string, ...args]: nothing -> nothing {
    cargo build --bin $app;
    rm -rf $"target/debug/($app).($copy).exe";
    cp -rf $"target/debug/($app).exe" $"target/debug/($app).($copy).exe";
    ^$"target/debug/($app).($copy).exe" ...$args
}
