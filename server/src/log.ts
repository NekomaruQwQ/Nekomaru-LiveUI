/* ── Colored logging utilities for the LiveUI server ──────────────────────────
   Structured, color-coded log output for every server module.  Two cooperating
   systems:

   1. MARKER SYSTEM (§1)
      Every log line begins with a right-padded marker that identifies the
      source module and, optionally, the stream context.  Two shapes:

        [moduleId]                   — non-stream (server-level) logs
        [@streamId moduleId]         — stream-scoped logs

      Markers are colored with `picocolors`:
        - Brackets and module ID → cyan
        - Stream ID (`@main`, `@youtube-music`) → bold green

   2. ALIGNMENT SYSTEM (§2)
      Markers are right-padded with spaces so that message text aligns
      vertically.  The pad width depends on the stream prefix length so that
      each "level" of nesting aligns independently:

        Non-stream markers      pad to BASE_PAD_WIDTH (14 chars)
        Stream markers          pad to BASE_PAD_WIDTH + len("@" + streamId + " ") + 1

      This keeps the common case compact while still aligning lines within
      each group.

   ─── LEVEL STYLING ───────────────────────────────────────────────────────

   Four levels, applied to the message text (not the marker):

     error → bold red      (hard failures)
     warn  → yellow        (recoverable issues)
     debug → dim           (verbose diagnostics)
     info  → unstyled      (normal operation)

   ─── DEBUG GATING ───────────────────────────────────────────────────────

   Server-side debug logs (`log.debug(...)`) are gated behind the
   `LIVEUI_DEBUG` environment variable.  When unset or empty, `debug()`
   calls are no-ops — zero formatting cost.  Set `LIVEUI_DEBUG=1` to
   enable verbose output.
   ────────────────────────────────────────────────────────────────────────── */

import pc from "picocolors";

/// When truthy, `Logger.debug()` calls produce output.  Otherwise they are
/// silent no-ops (zero formatting cost).  Read once at module load.
const DEBUG_ENABLED = !!process.env.LIVEUI_DEBUG;

// ── Types ────────────────────────────────────────────────────────────────────

export interface Logger {
    info(msg: string): void;
    warn(msg: string): void;
    error(msg: string): void;
    /// Verbose diagnostics — only emitted when `LIVEUI_DEBUG` is set.
    debug(msg: string): void;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   §1  MARKER SYSTEM
   ═══════════════════════════════════════════════════════════════════════════════

   Every log line starts with a marker that identifies the source.  Markers
   carry two pieces of information:

     1. Module ID — a short name identifying the server module (e.g. `video`,
        `kpm`, `selector`).

     2. Stream ID (optional) — the well-known stream name (`main`,
        `youtube-music`).  Present only for logs that relate to a specific
        capture stream.

   Both return `{ text, visibleLen }`.  `text` contains ANSI escape codes
   (from picocolors), so its `.length` is longer than the visible width.
   `visibleLen` is the number of printable characters — used by the
   alignment system (§2) to compute padding.
   ────────────────────────────────────────────────────────────────────────── */

/// Build a non-stream marker: `[moduleId]` with brackets and module in cyan.
function moduleMarker(moduleId: string): { text: string; visibleLen: number } {
    const plain = `[${moduleId}]`;
    return { text: pc.cyan(plain), visibleLen: plain.length };
}

/// Build a stream-scoped marker: `[@streamId moduleId]` with the stream ID
/// in bold green, brackets and module in cyan.
function streamMarker(streamId: string, moduleId: string): { text: string; visibleLen: number } {
    const plain = `[@${streamId} ${moduleId}]`;
    return {
        text: `${pc.cyan("[")}${pc.bold(pc.green(`@${streamId}`))} ${pc.cyan(`${moduleId}]`)}`,
        visibleLen: plain.length,
    };
}


/* ═══════════════════════════════════════════════════════════════════════════════
   §2  ALIGNMENT SYSTEM
   ═══════════════════════════════════════════════════════════════════════════════

   Markers are right-padded with spaces to a target width, so message text
   starts at the same column for all markers in the same "stream level".

   The target width is NOT a single global constant.  Each stream level has
   its own width:

     Non-stream:       BASE_PAD_WIDTH                            = 14
     Stream "main":    BASE_PAD_WIDTH + len("main") + 2          = 20
     Stream "yt-music":BASE_PAD_WIDTH + len("youtube-music") + 2 = 29

   The +2 accounts for the `@` prefix and the space between stream ID and
   module ID.

   If a marker exceeds its target width, NO padding is added — the message
   just starts one space after the marker.

   BASE_PAD_WIDTH = 14 is sized for `[selector]` (10 chars + 2 brackets =
   12), the longest commonly-seen non-stream module, plus 2 chars of
   breathing room.
   ────────────────────────────────────────────────────────────────────────── */

/// Base pad width for the module-ID portion of the marker.  Stream markers
/// add the stream prefix length on top, so each stream level aligns
/// independently.
const BASE_PAD_WIDTH = 14;

/// Right-pad a marker to `targetWidth` visible characters.
/// If the marker already exceeds the target, no padding is added (overflow).
function padMarker(text: string, visibleLen: number, targetWidth: number): string {
    if (visibleLen >= targetWidth) return text;
    return text + " ".repeat(targetWidth - visibleLen);
}

/// Build a Logger from a pre-computed marker string and its target width.
/// The returned closures capture the padded marker, so formatting happens
/// once at logger creation time — not on every log call.
function loggerFromMarker(text: string, visibleLen: number, targetWidth: number): Logger {
    const m = padMarker(text, visibleLen, targetWidth);
    return {
        info(msg)  { console.log(`${m} ${msg}`); },
        warn(msg)  { console.log(`${m} ${styleByLevel("warn", msg)}`); },
        error(msg) { console.log(`${m} ${styleByLevel("error", msg)}`); },
        debug(msg) { if (DEBUG_ENABLED) console.log(`${m} ${styleByLevel("debug", msg)}`); },
    };
}


// ── Level styling ────────────────────────────────────────────────────────────

type Level = "info" | "warn" | "error" | "debug";

/// Apply level-specific styling to a message string.
function styleByLevel(level: Level, msg: string): string {
    switch (level) {
        case "error": return pc.bold(pc.red(msg));
        case "warn":  return pc.yellow(msg);
        case "debug": return pc.dim(msg);
        case "info":  return msg;
    }
}


// ── Public API ───────────────────────────────────────────────────────────────

/// Create a non-stream logger.  Marker: `[moduleId]` in cyan.
/// Pad target: BASE_PAD_WIDTH (14).
export function createLogger(moduleId: string): Logger {
    const { text, visibleLen } = moduleMarker(moduleId);
    return loggerFromMarker(text, visibleLen, BASE_PAD_WIDTH);
}

/// Create a stream-scoped logger.  Marker: `[@streamId moduleId]` with the
/// stream ID in bold green, brackets and module in cyan.
/// Pad target: BASE_PAD_WIDTH + streamId.length + 2  (for `@` and space).
export function createStreamLogger(streamId: string, moduleId: string): Logger {
    const { text, visibleLen } = streamMarker(streamId, moduleId);
    return loggerFromMarker(text, visibleLen, BASE_PAD_WIDTH + streamId.length + 2);
}
