import * as path from "node:path";
import * as vite from "vite";
import react from "@vitejs/plugin-react-swc";
import tailwindcss from "@tailwindcss/vite";

const vitePort = Number(process.env.LIVE_VITE_PORT);
if (!vitePort) throw new Error("LIVE_VITE_PORT environment variable is not set");

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

        // The browser loads the page from the core server (LIVE_PORT), not Vite.
        // The core server proxies non-API requests to Vite for dev assets.
        // HMR client must connect directly to Vite — no proxy needed for dev-only traffic.
        hmr: { clientPort: vitePort },
    },
});
