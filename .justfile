set shell := ["nu", "-c"]

base_url := "http://localhost:($env.LIVE_PORT)"

alias i := install

list:
    just --list
push bookmark revision="@-":
    jj bookmark move {{bookmark}} --to={{revision}}
    jj git push --all
pull:
    jj git fetch
    jj git new -r main

install:
    cargo build --release
    cd frontend; bun i
refresh:
    http post $"{{base_url}}/api/v1/refresh" ""
capture name:
    http put $"{{base_url}}/api/v1/streams/auto/config/preset" "{{name}}"

server *args:
    cargo build --release
    cargo run --release -p live-server -- {{args}}
app *args:
    use .mod.nu run-app; \
    run-app app $"{{base_url}}" \
        -x 1280 -y 720 \
        -t "Nekomaru LiveUI" \
        {{args}}
youtube-music *args:
    use .mod.nu run-app; \
    run-app youtube-music "https://music.youtube.com/" \
        -x 1280 -y 720 -s 2 \
        -t "YouTube Music - Nekomaru LiveUI" \
        {{args}}
