/* ── Colored logging utilities for the LiveServer ─────────────────────────────
   This module provides structured, color-coded log output for every server
   module and forwarded Rust child-process stderr.  Three cooperating systems:

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
      vertically.  The pad width is NOT global — it depends on the stream
      prefix length so that each "level" of nesting aligns independently:

        Non-stream markers      pad to BASE_PAD_WIDTH (18 chars)
        Stream markers          pad to BASE_PAD_WIDTH + len("@" + streamId + " ") + 1

      This means `[server::selector]` (18 visible chars) aligns with
      `[server::process]` (17 visible chars, 1 space of padding), while
      `[@main server::process]` (24 visible chars) aligns with
      `[@main server::selector]` (25 visible chars, no padding — overflow is
      fine).  The two groups have different left margins, but within each
      group the message columns line up.

      Why not a single global width?  The longest module ID is
      `server::youtube_music` (23 chars + brackets = 25).  Aligning
      everything to 25 would waste 7 characters on the common non-stream
      markers.  Per-level widths keep things compact.

   3. RUST STDERR FORWARDING (§3)
      The Rust child process (`live-capture.exe`) writes env_logger-formatted
      lines to stderr:  `[LEVEL target] message`.  These arrive in the server
      via `pipeStderr()` in process.ts, which groups them into logical entries
      (one head line + zero or more continuation lines) using a 10 ms
      timer-based flush.

      `writeCaptureGroup()` then renders each group:

        SINGLE-LINE group → inline with marker:
          [@main live_capture::encoder] H.264 encoder ready

        MULTILINE group → marker on its own line, body indented +4:
          [@main live_capture::encoder]
              thread 'encoding' (78980) panicked at 'index out of bounds'
              note: run with `RUST_BACKTRACE=1`
              stack backtrace:
                 0: std::panicking::begin_panic

      The env_logger target (`live_capture::encoder`) becomes the module ID
      in the marker.  The `winrt_capture` crate is remapped to `live_capture`
      for consistency (it's an implementation detail of the capture pipeline).

   ─── LEVEL STYLING ───────────────────────────────────────────────────────

   Four levels, applied to the message text (not the marker):

     error → bold red      (hard failures, IPC errors)
     warn  → yellow        (recoverable issues)
     debug → dim           (verbose diagnostics, Rust DEBUG/TRACE)
     info  → unstyled      (normal operation)

   ─── DEBUG GATING ───────────────────────────────────────────────────────

   Server-side debug logs (`log.debug(...)`) are gated behind the
   `LIVEUI_DEBUG` environment variable.  When unset or empty, `debug()`
   calls are no-ops — zero formatting cost.  Set `LIVEUI_DEBUG=1` to
   enable verbose output (per-frame timing, poll results, buffer stats).

   This mirrors Rust's `RUST_LOG` approach: silent by default, opt-in
   verbosity.  The flag is read once at module load (`DEBUG_ENABLED`).

   For forwarded Rust output, the level is extracted from the env_logger
   header (`[INFO ...]`, `[ERROR ...]`, etc.).  A special case: if ANY line
   in a group matches the Rust panic pattern (`thread '...' panicked`), the
   ENTIRE group is upgraded to error level — because a panic always means
   the child process is about to crash, and coloring only the panic line
   while leaving the backtrace unstyled would be misleading.

   ─── GROUPING (caller's responsibility) ──────────────────────────────────

   `writeCaptureGroup` receives a pre-grouped array of lines.  The grouping
   logic lives in `pipeStderr()` (process.ts), NOT here.  The split:

     process.ts  — owns line buffering, newline splitting, and time-based
                   grouping (10 ms flush delay).  Calls `isCaptureLogHead()`
                   to detect group boundaries.
     log.ts      — owns rendering: marker construction, padding, coloring,
                   single-vs-multiline layout.  Stateless — pure function of
                   the input lines.

   This separation keeps log.ts free of async/timer concerns and makes both
   halves independently testable.
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

     1. Module ID — the Rust-style `crate::module` path of the source file.
        Server modules use `server::*` (e.g. `server::selector`); forwarded
        Rust lines keep their original target (e.g. `live_capture::encoder`).

     2. Stream ID (optional) — the well-known stream name (`main`,
        `youtube-music`, or a random UUID prefix for ad-hoc streams).
        Present only for logs that relate to a specific capture stream.

   Two marker shapes exist:

     moduleMarker(moduleId)             → `[moduleId]`
       - Entire text in cyan.
       - Used by `createLogger()` for server-level logs.

     streamMarker(streamId, moduleId)   → `[@streamId moduleId]`
       - `[` and `moduleId]` in cyan, `@streamId` in bold green.
       - The bold green makes the stream ID pop visually, so you can scan
         a busy log and instantly see which stream each line belongs to.
       - Used by `createStreamLogger()` and `writeCaptureGroup()`.

   Both return `{ text, visibleLen }`.  `text` contains ANSI escape codes
   (from picocolors), so its `.length` is longer than the visible width.
   `visibleLen` is the number of printable characters — used by the
   alignment system (§2) to compute how much padding to add.
   ────────────────────────────────────────────────────────────────────────── */

/// Build a non-stream marker: `[moduleId]` with brackets and module in cyan.
/// Returns `{ text, visibleLen }` so the caller can pad.
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

   ─── PER-LEVEL TARGET WIDTHS ────────────────────────────────────────────

   The target width is NOT a single global constant.  Instead, each stream
   level has its own width:

     Non-stream:       BASE_PAD_WIDTH                          = 18
     Stream "main":    BASE_PAD_WIDTH + len("main") + 2        = 24
     Stream "yt-music":BASE_PAD_WIDTH + len("youtube-music") + 2 = 33

   The +2 accounts for the `@` prefix and the space between stream ID and
   module ID.  This means:

     [server::selector]  ···· msg     ← 18-char column (2 spaces padding)
     [server::process]   ····· msg    ← 18-char column (3 spaces padding)
     [@main server::selector] · msg   ← 24-char column (0 spaces — overflow)
     [@main server::process]  ·· msg  ← 24-char column (1 space padding)

   Within each group, messages align.  Between groups, the left margin
   differs — but since stream-scoped logs are visually distinct (bold green
   stream ID), the different indentation actually helps readability.

   ─── OVERFLOW BEHAVIOR ──────────────────────────────────────────────────

   If a marker exceeds its target width (e.g. `[server::youtube_music]` is
   25 chars vs the 18-char target), NO padding is added — the message just
   starts one space after the marker.  This prevents long module names from
   pushing common markers far to the right.

   ─── WHY NOT A GLOBAL WIDTH? ────────────────────────────────────────────

   A global width of 33 (to fit `[@youtube-music server::youtube_music]`)
   would waste 15 characters on every `[server::process]` line — nearly a
   full tab stop of dead space.  Per-level widths keep the common case
   compact while still aligning lines within each group.

   BASE_PAD_WIDTH = 18 is sized for `[server::selector]` (16 chars + 2
   brackets = 18), the longest commonly-seen non-stream module.  The
   `server::youtube_music` module overflows by 7 characters, but it logs
   infrequently enough that this is acceptable.
   ────────────────────────────────────────────────────────────────────────── */

/// Base pad width for the module-ID portion of the marker.  Stream markers
/// add the stream prefix length on top, so each stream level aligns
/// independently.  Sized for typical modules (`server::selector` = 16
/// chars + 2 brackets = 18); longer markers overflow without extra padding.
const BASE_PAD_WIDTH = 18;

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
/// Pad target: BASE_PAD_WIDTH (18).
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


/* ═══════════════════════════════════════════════════════════════════════════════
   §3  RUST STDERR FORWARDING
   ═══════════════════════════════════════════════════════════════════════════════

   live-capture.exe writes structured logs to stderr using Rust's `env_logger`
   crate.  Each log line has the format:

     [LEVEL target] message

   For example:
     [INFO  live_capture::encoder] creating H.264 encoder...
     [ERROR live_capture::capture] DirectX device lost

   These lines arrive in the server via `pipeStderr()` in process.ts, which
   splits on newlines and groups them into logical entries.  This module then
   renders each group with a stream-scoped marker.

   ─── LINE GROUPING ──────────────────────────────────────────────────────

   A "group" is one env_logger head line plus zero or more continuation lines
   (backtrace frames, multi-line error messages, etc.).  Grouping is done by
   `pipeStderr()` in process.ts using two signals:

     1. HEAD DETECTION: `isCaptureLogHead(line)` tests whether a line matches
        `ENV_LOG_RE`.  A match starts a new group (flushing the previous one).

     2. TIME-BASED FLUSH: a 10 ms timer fires after the last line in the
        group.  This catches continuation lines that arrive in the same or
        next event-loop tick, while keeping single-line logs fast (10 ms
        latency is imperceptible).

   ─── SINGLE vs MULTILINE RENDERING ──────────────────────────────────────

   writeCaptureGroup() receives the pre-grouped lines and renders them:

     SINGLE LINE (lines.length === 1):
       The marker and message share a line, separated by a space:
         [@main live_capture::encoder] H.264 encoder ready

     MULTIPLE LINES (lines.length > 1):
       The marker stands alone on its line.  Every content line (including
       the parsed message from the head) is indented +4 spaces below:
         [@main live_capture::encoder]
             thread 'encoding' (78980) panicked at 'index out of bounds'
             note: run with `RUST_BACKTRACE=1`
             stack backtrace:
                0: std::panicking::begin_panic

       Why +4?  It visually nests the body under the marker without wasting
       too much horizontal space.  The indent is absolute (4 spaces from the
       left edge), not relative to the marker width — so multiline bodies
       always start in the same column regardless of stream ID length.

   ─── PANIC DETECTION ────────────────────────────────────────────────────

   Rust panics produce output like:
     thread 'encoding' (78980) panicked at 'index out of bounds', src/encoder.rs:42:5
     note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
     stack backtrace:
        0: std::panicking::begin_panic
        1: live_capture::encoder::encode_frame

   These may or may not be preceded by an env_logger head line.  PANIC_RE
   matches the `thread '...' panicked` pattern.

   For multiline groups: if ANY line matches PANIC_RE, the ENTIRE group is
   styled as error (bold red).  Coloring only the panic line while leaving
   the backtrace unstyled would be misleading — the whole block is one
   fatal event.

   For single-line groups: only the message is tested against PANIC_RE.

   ─── TARGET NORMALIZATION ───────────────────────────────────────────────

   The `winrt_capture` crate is an external dependency used by
   `live_capture` for Windows Runtime capture APIs.  Its env_logger target
   (`winrt_capture::...`) is remapped to `live_capture::...` so the log
   reads as a unified pipeline rather than leaking implementation details.
   ────────────────────────────────────────────────────────────────────────── */

/// Regex matching the env_logger default format: `[LEVEL target] message`
///
/// Capture groups:
///   1: level string (INFO, ERROR, WARN, DEBUG, TRACE)
///   2: target path  (e.g. `live_capture::encoder`)
///   3: message text (everything after the closing `]`)
const ENV_LOG_RE = /^\[(\w+)\s+([^\]]+)]\s*(.*)$/;

/// Matches a Rust panic line, e.g. `thread 'encoding' (78980) panicked at ...`
/// Used to upgrade the entire group's level to error when a panic is detected.
const PANIC_RE = /^thread\s+'[^']*'.*panicked/;

/// Map env_logger level strings to our level type.
function parseRustLevel(raw: string): Level {
    switch (raw) {
        case "ERROR": return "error";
        case "WARN":  return "warn";
        case "DEBUG": case "TRACE": return "debug";
        default:      return "info";
    }
}

/// Normalize a Rust crate target name.  The `winrt_capture` external crate
/// is conceptually part of `live_capture`, so we remap it for consistency.
function normalizeRustTarget(target: string): string {
    if (target.startsWith("winrt_capture")) {
        return target.replace("winrt_capture", "live_capture");
    }
    return target;
}

/// Test whether a line looks like the start of a Rust env_logger entry
/// (`[LEVEL target] message`).  Used by `pipeStderr` in process.ts to
/// detect group boundaries.
export function isCaptureLogHead(line: string): boolean {
    return ENV_LOG_RE.test(line);
}

/// Write a group of lines originating from a single Rust log entry.
///
/// - **Single line**: printed inline with its marker.
/// - **Multiple lines**: marker on its own line, each content line indented +4.
///
/// The first line is parsed with `ENV_LOG_RE` to extract the level and target
/// for the marker.  If the line doesn't match the env_logger format (e.g.
/// `pretty_env_logger` uses `LEVEL target > msg`), `defaultTarget` is used
/// as the module ID in the marker.
///
/// If any line matches the panic pattern, the entire group is styled as error.
export function writeCaptureGroup(streamId: string, lines: string[], defaultTarget: string): void {
    if (lines.length === 0) return;

    const targetWidth = BASE_PAD_WIDTH + streamId.length + 2;
    const head = lines[0] ?? "";
    const m = ENV_LOG_RE.exec(head);

    // Build the marker from the first line (or fall back to caller-provided default).
    const level = m ? parseRustLevel(m[1] ?? "") : "info";
    const target = m ? normalizeRustTarget(m[2] ?? defaultTarget) : defaultTarget;
    const message = m ? (m[3] ?? "") : head;
    const marker = streamMarker(streamId, target);
    const padded = padMarker(marker.text, marker.visibleLen, targetWidth);

    if (lines.length === 1) {
        // Single-line — inline.  Upgrade to error if the message is a panic.
        const effective = PANIC_RE.test(message) ? "error" as Level : level;
        process.stderr.write(`${padded} ${styleByLevel(effective, message)}\n`);
    } else {
        // Multiline — marker alone, then every line indented +4.
        // Upgrade the entire block to error if any line contains a panic.
        const effective = lines.some((l) => PANIC_RE.test(l)) ? "error" as Level : level;
        process.stderr.write(`${padded}\n`);
        process.stderr.write(`    ${styleByLevel(effective, message)}\n`);
        for (let i = 1; i < lines.length; i++) {
            process.stderr.write(`    ${styleByLevel(effective, lines[i] ?? "")}\n`);
        }
    }
}
