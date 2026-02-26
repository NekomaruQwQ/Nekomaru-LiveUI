import { useEffect, useState } from 'react';

import { api } from './api';

/// Default capture resolution (matches RTX 5090 / existing constants).
const DEFAULT_WIDTH = 1920;
const DEFAULT_HEIGHT = 1200;

/// How often to poll the auto-selector for stream ID changes (ms).
const AUTO_POLL_INTERVAL_MS = 1000;

/// Expected shape of window info from the enumerate-windows crate.
export interface WindowInfo {
    hwnd: number;
    title: string;
}

// ── Capture control hook ────────────────────────────────────────────────

/// Encapsulates all capture lifecycle management: auto-selector polling,
/// window enumeration, stream creation/destruction.  Returns state + actions
/// that the UI layer consumes without knowing about the API.
export function useCaptureControl() {
    /// Active stream ID, or null when no capture is running.
    const [streamId, setStreamId] = useState<string | null>(null);
    /// Whether the auto-selector is managing the stream.
    const [autoActive, setAutoActive] = useState(true);
    /// Capturable windows shown in the picker, or null when picker is closed.
    const [windows, setWindows] = useState<WindowInfo[] | null>(null);
    /// True during the initial startup check.
    const [loading, setLoading] = useState(true);

    // Auto-selector: activate on mount, poll for stream ID changes.
    // When autoActive flips to false the effect cleans up (clears interval).
    useEffect(() => {
        if (!autoActive) return;

        let cancelled = false;
        let intervalId: ReturnType<typeof setInterval> | null = null;

        /// Poll the server for the auto-selector's current stream ID.
        async function pollAutoStatus() {
            if (cancelled) return;
            try {
                const res = await api.auto.$get();
                if (!res.ok || cancelled) return;
                const status = await res.json();
                setStreamId(status.currentStreamId);
            } catch (e) {
                console.error("Failed to poll auto status:", e);
            }
        }

        (async () => {
            try {
                // Activate the auto-selector on the server (idempotent).
                await api.auto.$post();
                // Immediate first poll so we don't wait a full interval.
                await pollAutoStatus();
                intervalId = setInterval(pollAutoStatus, AUTO_POLL_INTERVAL_MS);
            } catch (e) {
                console.error("Failed to start auto-selector:", e);
            } finally {
                if (!cancelled) setLoading(false);
            }
        })();

        return () => {
            cancelled = true;
            if (intervalId) clearInterval(intervalId);
        };
    }, [autoActive]);

    /// Stop the auto-selector and fall back to manual mode.
    async function stopAuto() {
        try {
            await api.auto.$delete();
        } catch (e) {
            console.error("Failed to stop auto-selector:", e);
        }
        setAutoActive(false);
        setStreamId(null);
    }

    /// Re-enable the auto-selector.
    function startAuto() {
        setAutoActive(true);
        setLoading(true);
    }

    /// Fetch the list of capturable windows and show the picker.
    async function loadWindows() {
        try {
            const res = await api.windows.$get();
            if (!res.ok) return;
            const data = await res.json() as unknown as WindowInfo[];
            setWindows(data);
        } catch (e) {
            console.error("Failed to enumerate windows:", e);
        }
    }

    /// Create a capture stream for the selected window.
    async function startCapture(win: WindowInfo) {
        try {
            const res = await api.index.$post({
                json: {
                    hwnd: String(win.hwnd),
                    width: DEFAULT_WIDTH,
                    height: DEFAULT_HEIGHT,
                },
            });
            if (!res.ok) return;
            const { id } = await res.json();
            setStreamId(id);
            setWindows(null);
        } catch (e) {
            console.error("Failed to create stream:", e);
        }
    }

    /// Stop a manually-created stream and return to idle.
    async function stopManualCapture() {
        if (!streamId) return;
        try {
            await api[":id"].$delete({ param: { id: streamId } });
        } catch (e) {
            console.error("Failed to destroy stream:", e);
        }
        setStreamId(null);
    }

    /// Close the window picker without selecting.
    function dismissWindows() {
        setWindows(null);
    }

    return {
        streamId,
        autoActive,
        windows,
        loading,
        stopAuto,
        startAuto,
        loadWindows,
        startCapture,
        stopManualCapture,
        dismissWindows,
    };
}
