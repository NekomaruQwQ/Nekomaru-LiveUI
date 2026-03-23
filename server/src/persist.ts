/**
 * JSON file persistence helpers.
 *
 * Ported from M2's `persist.ts`.  Uses Bun's file API.
 */

const DATA_DIR = "data";

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
