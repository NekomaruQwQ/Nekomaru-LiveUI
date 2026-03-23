/**
 * Selector config storage and routes.
 *
 * The server no longer runs the selector — `live-capture --mode auto` does.
 * The server just stores and serves the config.  Auto mode polls GET /config.
 */

import { Hono } from "hono";
import { loadJson, saveJson, ensureDataDir } from "./persist";

const CONFIG_PATH = "data/selector-config.json";

// ── Types ───────────────────────────────────────────────────────────────────

interface PresetConfig {
    preset: string;
    presets: Record<string, string[]>;
}

const DEFAULT_CONFIG: PresetConfig = {
    preset: "main",
    presets: {
        main: [
            "@code devenv.exe",
            "@code C:/Program Files/Microsoft Visual Studio Code/Code.exe",
            "@code C:/Program Files/JetBrains/",
            "@game D:/7-Games/",
            "@game D:/7-Games.Steam/steamapps/common/",
            "@game E:/Nekomaru-Games/",
            "@game E:/SteamLibrary/steamapps/common/",
            "@exclude gogh.exe",
            "@exclude vtube studio.exe",
        ],
    },
};

// ── State ───────────────────────────────────────────────────────────────────

let config: PresetConfig = { ...DEFAULT_CONFIG };

/** Load config from disk. */
export async function loadSelectorConfig(): Promise<void> {
    await ensureDataDir();
    config = await loadJson(CONFIG_PATH, DEFAULT_CONFIG);
    console.log(`[selector] loaded config: preset="${config.preset}", ${Object.keys(config.presets).length} preset(s)`);
}

// ── Routes ──────────────────────────────────────────────────────────────────

const app = new Hono();

// GET /api/v1/streams/auto/config — full preset config (polled by live-capture).
app.get("/config", (c) => c.json(config));

// PUT /api/v1/streams/auto/config — replace full config.
app.put("/config", async (c) => {
    config = await c.req.json<PresetConfig>();
    await saveJson(CONFIG_PATH, config);
    return c.json({ ok: true });
});

// PUT /api/v1/streams/auto/config/preset — switch active preset by name.
app.put("/config/preset", async (c) => {
    const name = await c.req.text();
    if (!name) return c.json({ error: "preset name required" }, 400);

    // Reload from disk first (may have been edited externally).
    config = await loadJson(CONFIG_PATH, DEFAULT_CONFIG);

    if (!config.presets[name]) {
        return c.json({ error: `preset "${name}" not found` }, 400);
    }

    config.preset = name;
    await saveJson(CONFIG_PATH, config);
    return c.json({ ok: true });
});

export default app;
