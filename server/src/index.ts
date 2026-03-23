/**
 * Nekomaru LiveUI — M4 server entry point.
 *
 * Thin HTTP/WS relay server.  No Win32, no GPU, no binary protocol parsing.
 * All capture intelligence lives in the Rust workers.
 *
 * Usage:
 *   LIVE_PORT=3000 LIVE_VITE_PORT=5173 bun run src/index.ts
 */

import { Hono } from "hono";
import { websocket } from "hono/bun";

import videoRoutes from "./video";
import kpmRoutes from "./kpm";
import selectorRoutes from "./selector";
import stringsRoutes from "./strings";
import coreRoutes from "./core";

import { loadStrings } from "./strings";
import { loadSelectorConfig } from "./selector";
import { createLogger } from "./log";

const log = createLogger("server");

// ── Config ──────────────────────────────────────────────────────────────────

const PORT = Number(process.env.LIVE_PORT);
const VITE_PORT = Number(process.env.LIVE_VITE_PORT);

if (!PORT || !VITE_PORT) {
    log.error("Both LIVE_PORT and LIVE_VITE_PORT are required");
    process.exit(1);
}

// ── App ─────────────────────────────────────────────────────────────────────

const app = new Hono();

// Mount API routes.
app.route("/api/v1/streams", videoRoutes);
app.route("/api/v1/streams/auto", selectorRoutes);
app.route("/api/v1/strings", stringsRoutes);
app.route("/api/v1/kpm", kpmRoutes);
app.route("/api/core", coreRoutes);

// POST /api/v1/refresh — reload selector config + strings from disk.
app.post("/api/v1/refresh", async (c) => {
    await loadSelectorConfig();
    await loadStrings();
    return c.json({ ok: true });
});

// GET /api/v1/strings — also accessible via WS polling from frontend.
// (Strings routes already handle this.)

// ── Vite Proxy Fallback ─────────────────────────────────────────────────────

// Non-API requests are reverse-proxied to the Vite dev server.
app.all("/*", async (c) => {
    const url = new URL(c.req.url);
    const viteUrl = `http://localhost:${VITE_PORT}${url.pathname}${url.search}`;

    try {
        const resp = await fetch(viteUrl, {
            method: c.req.method,
            headers: c.req.raw.headers,
            body: c.req.method !== "GET" && c.req.method !== "HEAD"
                ? c.req.raw.body
                : undefined,
        });

        return new Response(resp.body, {
            status: resp.status,
            headers: resp.headers,
        });
    } catch {
        return c.text("Vite dev server not available", 502);
    }
});

// ── Start ───────────────────────────────────────────────────────────────────

// Load persisted state before starting.
await loadStrings();
await loadSelectorConfig();

// Spawn Vite dev server as a child process.
const frontendDir = new URL("../../frontend", import.meta.url).pathname
    // Bun on Windows returns /C:/path — strip the leading slash.
    .replace(/^\/([A-Z]:)/, "$1");

const viteProcess = Bun.spawn(
    ["bunx", "--bun", "vite", "--port", String(VITE_PORT)],
    {
        cwd: frontendDir,
        env: { ...process.env, LIVE_VITE_PORT: String(VITE_PORT) },
        stdout: "inherit",
        stderr: "inherit",
    });

log.info(`spawned vite on port ${VITE_PORT} (pid ${viteProcess.pid})`);

// Start the Bun HTTP + WS server.
const server = Bun.serve({
    port: PORT,
    fetch: app.fetch,
    websocket,
});

log.info(`listening on http://localhost:${PORT}`);

// ── Graceful Shutdown ───────────────────────────────────────────────────────

process.on("SIGINT", () => {
    log.info("shutting down...");
    viteProcess?.kill();
    server.stop();
    process.exit(0);
});
