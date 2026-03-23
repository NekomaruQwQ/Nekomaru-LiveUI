/**
 * Server-managed key-value string store.
 *
 * Two persistence layers (same as M2/M3):
 * - `data/strings.json` — single-line values
 * - `data/strings/<key>.md` — multiline values
 *
 * Keys prefixed with `$` are computed strings — readonly values derived
 * from live server state (e.g. capture info, mode).  They appear in GET
 * responses but cannot be written or deleted via the API (returns 403).
 */

import { Hono } from "hono";
import { loadJson, saveJson, ensureDataDir, DATA_DIR } from "./persist";

import * as fs from "node:fs/promises";
import { existsSync } from "node:fs";
import * as path from "node:path";

const STRINGS_FILE  = path.join(DATA_DIR, "strings.json");
const STRINGS_DIR   = path.join(DATA_DIR, "strings");

// ── State ───────────────────────────────────────────────────────────────────

/** File-backed single-line strings. */
let store: Record<string, string> = {};

/** Computed strings ($-prefixed, set by worker events). */
const computed = new Map<string, string>();

// ── Public API ──────────────────────────────────────────────────────────────

/** Get all strings (merged: file-backed + computed). */
export function getAllStrings(): Record<string, string> {
    const result = { ...store };
    for (const [k, v] of computed) result[k] = v;
    return result;
}

/** Set a computed string.  Key must start with `$`. */
export function setComputed(key: string, value: string): void {
    computed.set(key, value);
}

/** Clear a computed string. */
export function clearComputed(key: string): void {
    computed.delete(key);
}

/** Load string store from disk. */
export async function loadStrings(): Promise<void> {
    await ensureDataDir();
    store = await loadJson(STRINGS_FILE, {});

    // Load multiline .md files.
    await fs.mkdir(STRINGS_DIR, { recursive: true });
    try {
        const files = await fs.readdir(STRINGS_DIR);
        for (const file of files) {
            if (!file.endsWith(".md")) continue;
            const key = file.slice(0, -3);
            const content = await fs.readFile(path.join(STRINGS_DIR, file), "utf-8");
            store[key] = content;
        }
    } catch {
        // Directory might not exist yet.
    }

    console.log(`[strings] loaded ${Object.keys(store).length} strings`);
}

/** Save single-line strings to disk. */
async function persistSingleLine(): Promise<void> {
    // Only persist non-multiline values to the JSON file.
    const singleLine: Record<string, string> = {};
    for (const [k, v] of Object.entries(store)) {
        if (!v.includes("\n")) singleLine[k] = v;
    }
    await saveJson(STRINGS_FILE, singleLine);
}

// ── Routes ──────────────────────────────────────────────────────────────────

const app = new Hono();

// GET /api/v1/strings — all strings (file-backed + computed).
app.get("/", (c) => c.json(getAllStrings()));

// PUT /api/v1/strings/:key — set a string value.
app.put("/:key", async (c) => {
    const key = c.req.param("key");
    if (key.startsWith("$")) return c.json({ error: "cannot write computed string" }, 403);

    const body = await c.req.json<{ value: string }>();
    const value = body.value;

    store[key] = value;

    // Persist: multiline → .md file, single-line → JSON.
    if (value.includes("\n")) {
        await fs.mkdir(STRINGS_DIR, { recursive: true });
        await fs.writeFile(path.join(STRINGS_DIR, `${key}.md`), value);
        // Remove from JSON if it was there.
        const json = await loadJson<Record<string, string>>(STRINGS_FILE, {});
        delete json[key];
        await saveJson(STRINGS_FILE, json);
    } else {
        await persistSingleLine();
        // Remove .md file if it was multiline before.
        const mdPath = path.join(STRINGS_DIR, `${key}.md`);
        if (existsSync(mdPath)) await fs.unlink(mdPath);
    }

    return c.json({ ok: true });
});

// DELETE /api/v1/strings/:key — delete a string.
app.delete("/:key", async (c) => {
    const key = c.req.param("key");
    if (key.startsWith("$")) return c.json({ error: "cannot delete computed string" }, 403);

    delete store[key];
    await persistSingleLine();

    // Remove .md file if it exists.
    const mdPath = path.join(STRINGS_DIR, `${key}.md`);
    if (existsSync(mdPath)) await fs.unlink(mdPath);

    return c.json({ ok: true });
});

export default app;
