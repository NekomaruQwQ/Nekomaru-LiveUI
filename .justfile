set shell := ["nu", "-c"]

list:
    just --list
app:
    cargo run -p live-app
server:
    cd server; bun --hot index.ts;

install: install-frontend install-server

install-frontend:
    cd frontend; bun i;
install-server:
    cd server; bun i;
