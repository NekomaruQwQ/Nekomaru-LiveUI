set shell := ["nu", "-c"]

list:
    just --list

install: install-frontend install-server

[working-directory: "frontend"]
install-frontend:
    bun i
[working-directory: "server"]
install-server:
    bun i

# Run the webview host (live-app.exe)
app:
    cargo run -p live-app

# Run the dev server (LiveServer + Vite frontend proxy)
[working-directory: "server"]
server:
    bun --hot index.ts
