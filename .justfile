set shell := ["nu", "-c"]

alias i := install

list:
    just --list

server:
    use .mod.nu run; \
    run live-capture app --help
    cd server; bun --hot index.ts;

app *args:
    use .mod.nu run; \
    run live-app app {{args}}
control *args:
    use .mod.nu run; \
    run live-control app {{args}}
youtube-music *args:
    use .mod.nu run; \
    run live-app youtube-music \
        "https://music.youtube.com/" \
        -m "YouTube Music" \
        -s "2" \
        {{args}}

install: install-frontend install-server
install-frontend:
    cd frontend; bun i;
install-server:
    cd server; bun i;
