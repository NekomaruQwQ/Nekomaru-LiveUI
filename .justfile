set shell := ["nu", "-c"]

list:
    just --list
app:
    cargo run -p live-app
control:
    cargo run -p live-control
server:
    cd server; bun --hot index.ts;

install: install-frontend install-server

install-frontend:
    cd frontend; bun i;
install-server:
    cd server; bun i;
