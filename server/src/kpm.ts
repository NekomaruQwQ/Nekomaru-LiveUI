/**
 * KPM WebSocket relay.
 *
 * - Input:    `WS /api/v1/ws/kpm/input` — binary messages from live-kpm via live-ws.
 * - Frontend: `WS /api/v1/ws/kpm`       — pushes `{"kpm": N}` JSON text.
 *
 * The server reads the KpmUpdate payload (i64 LE), converts to JSON, and
 * pushes to all connected frontend clients.
 */

import { Hono } from "hono";
import { upgradeWebSocket } from "hono/bun";
import { MessageType, HEADER_SIZE } from "./protocol";
import { createLogger } from "./log";

const log = createLogger("kpm");

// ── State ───────────────────────────────────────────────────────────────────

type KpmClient = { send: (data: string) => void };

const frontendClients = new Set<KpmClient>();
let lastKpmValue: number | null = null;

// ── Routes ──────────────────────────────────────────────────────────────────

const app = new Hono();

// WS /api/v1/ws/kpm/input — KPM updates from live-kpm via live-ws.
app.get(
    "/ws/input",
    upgradeWebSocket(() => ({
        onOpen() {
            log.info("input connected");
        },

        onMessage(event) {
            const raw = event.data;
            if (!(raw instanceof ArrayBuffer) && !ArrayBuffer.isView(raw)) return;
            const bytes = new Uint8Array(
                raw instanceof ArrayBuffer ? raw : raw.buffer);

            if (bytes.length < HEADER_SIZE + 8) return;
            if (bytes[0] !== MessageType.KpmUpdate) return;

            // Payload: i64 LE at offset HEADER_SIZE.
            const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
            const kpm = Number(view.getBigInt64(HEADER_SIZE, true));

            lastKpmValue = kpm;
            const json = JSON.stringify({ kpm });

            for (const client of frontendClients) {
                try { client.send(json); }
                catch { frontendClients.delete(client); }
            }
        },

        onClose() {
            log.info("input disconnected");
            lastKpmValue = null;
            // Push null to frontends.
            const json = JSON.stringify({ kpm: null });
            for (const client of frontendClients) {
                try { client.send(json); }
                catch { frontendClients.delete(client); }
            }
        },
    }))
);

// WS /api/v1/ws/kpm — frontend KPM display.
app.get(
    "/ws",
    upgradeWebSocket(() => ({
        onOpen(_event, ws) {
            const client: KpmClient = { send: (data) => ws.send(data) };
            frontendClients.add(client);

            // Send current value immediately.
            ws.send(JSON.stringify({ kpm: lastKpmValue }));

            (ws as any).__client = client;
            log.info(`frontend connected (${frontendClients.size} clients)`);
        },

        onClose(_event, ws) {
            const client = (ws as any).__client as KpmClient | undefined;
            if (client) frontendClients.delete(client);
            log.info(`frontend disconnected (${frontendClients.size} clients)`);
        },
    }))
);

export default app;
