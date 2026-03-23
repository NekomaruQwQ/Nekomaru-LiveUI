/**
 * JSON file persistence helpers.
 *
 * Ported from M2's `persist.ts`.  Uses Bun's file API.
 * Paths are resolved relative to the repo root (one level up from server/).
 */

import { resolve } from "node:path";

/** Repo root — one directory above server/. */
const REPO_ROOT = resolve(import.meta.dirname, "../..");

/** Data directory at the repo root. */
export const DATA_DIR = resolve(REPO_ROOT, "data");

/** Ensure the data directory exists. */
export async function ensureDataDir(): Promise<void> {
    const { mkdir } = await import("node:fs/promises");
    await mkdir(DATA_DIR, { recursive: true });
}

/** Load a JSON file, returning `fallback` if missing. */
export async function loadJson<T>(filePath: string, fallback: T): Promise<T> {
    const file = Bun.file(filePath);
    if (!await file.exists()) return fallback;
    return await file.json() as T;
}

/** Save a value as JSON. */
export async function saveJson(filePath: string, data: unknown): Promise<void> {
    await Bun.write(filePath, JSON.stringify(data, null, "\t"));
}
