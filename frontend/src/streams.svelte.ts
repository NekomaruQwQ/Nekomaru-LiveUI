// Stream-availability singleton.
//
// Polls GET /streams every 2 seconds and exposes boolean flags for whether
// each well-known stream exists.  The poll loop starts on first import —
// there is exactly one viewer shell, so a global singleton is the right
// shape (vs. a per-mount hook in the React version).

import { fetchStreams } from "./api";

const POLL_INTERVAL_MS = 2000;

/// Reactive holder for stream-availability flags.  Exposed as a singleton
/// instance so that consumers can read `streamStatus.hasMain` directly in
/// Svelte templates and get fine-grained reactivity.
class StreamStatus {
    hasMain = $state(false);
    hasYouTubeMusic = $state(false);
}

export const streamStatus = new StreamStatus();

async function poll() {
    try {
        const streams = await fetchStreams();
        streamStatus.hasMain = streams.some(s => s.id === "main");
        streamStatus.hasYouTubeMusic = streams.some(s => s.id === "youtube-music");
    } catch (e) {
        console.error("Failed to poll stream status:", e);
    }
}

void poll();
setInterval(poll, POLL_INTERVAL_MS);
