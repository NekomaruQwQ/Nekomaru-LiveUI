import * as http from "node:http";
import * as path from "node:path";
import * as vite from "vite";
import react from "@vitejs/plugin-react-swc";
import tailwindcss from "@tailwindcss/vite";

const corePort = Number(process.env.LIVE_CORE_PORT);
const vitePort = Number(process.env.LIVE_PORT);
if (!corePort) throw new Error("LIVE_CORE_PORT environment variable is not set");
if (!vitePort) throw new Error("LIVE_PORT environment variable is not set");

export default vite.defineConfig({
    root: __dirname,
    plugins: [
        react(),
        tailwindcss(),
    ],
    resolve: {
        alias: {
            "@": path.resolve(__dirname, "src"),
        },
    },
    server: {
        port: vitePort,

        // Allow any host to connect to the dev server.  This is necessary when running
        // the frontend on another pc.
        host: true,
        allowedHosts: true,

        // Proxy /api/* to the Rust server (live-server) during development.
        // In production, live-server serves the built frontend directly.
        // Shared keep-alive agent avoids a new TCP connection per request — without this,
        // each proxied fetch pays ~150ms for TCP+HTTP handshake, disastrous for 60fps polling.
        proxy: {
            "/api": {
                target: `http://localhost:${corePort}`,
                agent: new http.Agent({ keepAlive: true }),
            },
        },
    },
});
