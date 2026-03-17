set shell := ["nu", "-c"]

base_url := "http://localhost:($env.LIVE_CORE_PORT)"

alias i := install

list:
    just --list
refresh:
    http post $"{{base_url}}/api/v1/refresh" ""
capture name:
    http put $"{{base_url}}/api/v1/streams/auto/config/preset" "{{name}}"

server:
    cargo run -p live-server

app *args:
    cargo run -p live-app -- -x 1280 -y 720 {{args}}
youtube-music *args:
    cargo run -p live-app -- \
        "https://music.youtube.com/" \
        -t "YouTube Music" \
        -x 1280 -y 720 -s 2 \
        {{args}}

install:
    cd frontend; bun i;
