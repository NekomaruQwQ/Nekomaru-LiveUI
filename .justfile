set shell := ["nu", "-c"]

# Run the webview host (live-app.exe)
app:
    cargo run -p live-app

# Run the dev server (LiveServer + Vite frontend proxy)
[working-directory: "server"]
server:
    bun --hot index.ts
