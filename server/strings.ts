// Server-managed string store with well-known IDs.
//
// Provides a simple key-value store that the control panel can write to and
// the frontend can poll.  Follows the same well-known ID pattern as streams
// ("main", "youtube-music") — each string key maps to a specific display
// location in the frontend.
//
// Two persistence layers, loaded in order (higher layer wins on conflict):
//   1. data/strings.json        — single JSON file for short, single-line values
//   2. data/strings/<key>.md    — individual Markdown files for multiline content
//
// On PUT, values are routed automatically:
//   - Single-line → strings.json (removes any .md file for the same key)
//   - Multiline   → data/strings/<key>.md (removes the key from strings.json)
//
// Mounted at /strings in index.ts.  All routes are relative to that base:
//   GET    /       → all key-value pairs as a JSON object
//   PUT    /:key   → set a string value
//   DELETE /:key   → delete a string
//
// Routes are method-chained so TypeScript infers the full route schema into
// `typeof api`.  The frontend imports StringsApiType to create a typed Hono
// RPC client.

import * as path from "node:path";
import { existsSync, mkdirSync } from "node:fs";
import { readdir, unlink } from "node:fs/promises";
import { zValidator } from "@hono/zod-validator";
import { Hono } from "hono";
import { z } from "zod";
import { dataDir } from "./common";
import { loadJson, saveJson } from "./persist";

// ── Constants ────────────────────────────────────────────────────────────────

const stringsPath = path.join(dataDir, "strings.json");

/// Directory for file-based string sources (<key>.md).
const stringsDirPath = path.join(dataDir, "strings");

/// Accepted filename pattern: alphanumeric, hyphens, underscores + .md extension.
const validFilenamePattern = /^([a-zA-Z0-9_-]+)\.md$/;

/// Key validation: same charset as the filename stem.
const validKeyPattern = /^[a-zA-Z0-9_-]+$/;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A value is multiline if, after trimming trailing whitespace, it still
/// contains at least one newline character.
function isMultiline(value: string): boolean {
	return value.trimEnd().includes("\n");
}

/// Validate that a string key is safe for use as a filename stem.
function isValidKey(key: string): boolean {
	return validKeyPattern.test(key);
}

/// Scan data/strings/ for .md files and return a map of key → file content.
/// Individual file-read errors are caught and logged (non-fatal).
async function loadFileStrings(): Promise<Map<string, string>> {
	const result = new Map<string, string>();

	let entries: string[];
	try {
		entries = await readdir(stringsDirPath);
	} catch {
		return result;
	}

	for (const entry of entries) {
		const match = validFilenamePattern.exec(entry);
		if (!match) continue;

		const key = match[1];
		try {
			const content = await Bun.file(path.join(stringsDirPath, entry)).text();
			result.set(key, content);
		} catch (err) {
			console.warn(`[strings] failed to read ${entry}:`, err);
		}
	}

	console.log(`[strings] scanned ${result.size} file-based entries from data/strings/`);
	return result;
}

/// Remove data/strings/<key>.md if it exists.  Silent on missing file.
async function removeFileString(key: string): Promise<void> {
	try { await unlink(path.join(stringsDirPath, `${key}.md`)); } catch { /* missing is fine */ }
}

/// Remove a key from strings.json on disk (load → delete → save).
/// No-op if the file is missing or the key is absent.
async function removeFromJson(key: string): Promise<void> {
	const json = await loadJson<Record<string, string>>(stringsPath, {});
	delete json[key];
	await saveJson(stringsPath, json);
}

// ── Store ────────────────────────────────────────────────────────────────────

// Ensure data/strings/ exists before any reads or writes.
if (!existsSync(stringsDirPath)) mkdirSync(stringsDirPath, { recursive: true });

/// Backing store: string key → string value.
/// Hydrated from both disk sources on module load.
const store = new Map<string, string>();
{
	// Layer 1: strings.json (lower priority).
	const json = await loadJson<Record<string, string>>(stringsPath, {});
	for (const [k, v] of Object.entries(json)) store.set(k, v);

	// Layer 2: file-based strings (higher priority, overwrites JSON on conflict).
	const fileStrings = await loadFileStrings();
	for (const [k, v] of fileStrings) store.set(k, v);

	console.log(`[strings] loaded ${store.size} entries on startup`);
}

// ── Routes ───────────────────────────────────────────────────────────────────

const api = new Hono()

	/// Return all key-value pairs as a flat JSON object.
	.get("/", (c) => {
		return c.json(Object.fromEntries(store));
	})

	/// Set a string value by key (idempotent).
	/// Single-line values persist to strings.json; multiline values persist to
	/// data/strings/<key>.md.  The other source is cleaned up on each write.
	.put("/:key",
		zValidator("json", z.object({ value: z.string() })),
		async (c) => {
			const key = c.req.param("key");
			if (!isValidKey(key)) return c.json({ error: "invalid key" }, 400);

			const { value } = c.req.valid("json");
			store.set(key, value);

			if (isMultiline(value)) {
				// Multiline → write .md file, remove from JSON.
				await Bun.write(path.join(stringsDirPath, `${key}.md`), value);
				await removeFromJson(key);
			} else {
				// Single-line → update JSON, remove .md file.
				await removeFileString(key);
				const json = await loadJson<Record<string, string>>(stringsPath, {});
				json[key] = value;
				await saveJson(stringsPath, json);
			}

			return c.json({ ok: true });
		})

	/// Delete a string by key.  Cleans up both JSON and file-based sources.
	.delete("/:key", async (c) => {
		const key = c.req.param("key");
		if (!isValidKey(key)) return c.json({ error: "invalid key" }, 400);

		store.delete(key);
		await removeFromJson(key);
		await removeFileString(key);
		return c.json({ ok: true });
	});

/// Route type for Hono RPC — the frontend imports this to create a typed
/// client via `hc<StringsApiType>("/strings")`.
export type StringsApiType = typeof api;

/// Re-read all string sources from disk, replacing all in-memory entries.
/// Load order: strings.json first, then data/strings/*.md on top.
export async function reloadStore(): Promise<void> {
	store.clear();

	// Layer 1: strings.json (lower priority).
	const saved = await loadJson<Record<string, string>>(stringsPath, {});
	for (const [k, v] of Object.entries(saved)) store.set(k, v);

	// Layer 2: file-based strings (higher priority).
	const fileStrings = await loadFileStrings();
	for (const [k, v] of fileStrings) store.set(k, v);

	console.log(`[strings] reloaded ${store.size} total entries`);
}

export default api;
