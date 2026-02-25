import { useEffect, useState } from 'react';

import { api } from './api';
import { StreamRenderer } from './streamRenderer';

/// Default capture resolution (matches RTX 5090 / existing constants).
const DEFAULT_WIDTH = 1920;
const DEFAULT_HEIGHT = 1200;

/// How often to poll the auto-selector for stream ID changes (ms).
const AUTO_POLL_INTERVAL_MS = 1000;

/// Expected shape of window info from the enumerate-windows crate.
/// Adjust field names if the Rust crate uses a different schema.
interface WindowInfo {
    hwnd: number;
    title: string;
}

// ── Styles (JetBrains Islands) ──────────────────────────────────────────

/// Shared island panel style — dark surface, subtle border, soft shadow.
const island = "border border-[#393b40] rounded-2xl shadow-[0_1px_3px_rgba(0,0,0,0.3),0_4px_12px_rgba(0,0,0,0.15)] bg-[#2b2d30]";

/// Shared button style — dark pill with hover highlight.
const pillButton = "px-4 py-1.5 border border-[#4e5157] rounded-md bg-[#3c3f44] text-[#bcc0cc] text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#4a4d52]";

// ── App ─────────────────────────────────────────────────────────────────

export function App() {
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

    // ── Manual mode handlers ─────────────────────────────────────────────

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

    // ── Render ───────────────────────────────────────────────────────────

    return (
        <div className="flex flex-col flex-1 gap-2 p-2">
            <div className="flex flex-row flex-5 gap-2">
                <div className={`${island} flex-3 min-w-0 overflow-hidden`}>
                    {loading ? (
                        <Placeholder>Connecting...</Placeholder>
                    ) : streamId ? (
                        <StreamRenderer streamId={streamId} />
                    ) : autoActive ? (
                        <Placeholder>Auto-selecting...</Placeholder>
                    ) : windows ? (
                        <WindowPicker
                            windows={windows}
                            onSelect={startCapture}
                            onCancel={() => setWindows(null)}
                        />
                    ) : (
                        <Placeholder>
                            <div className="flex gap-2">
                                <button type="button" onClick={startAuto} className={pillButton}>
                                    Auto
                                </button>
                                <button type="button" onClick={loadWindows} className={pillButton}>
                                    Pick Window
                                </button>
                            </div>
                        </Placeholder>
                    )}
                </div>
                <div className={`${island} flex-1 p-6 flex flex-col gap-3`}>
                    <span className="text-[#bcc0cc]">Hi, I'm Nekomaru OwO</span>
                    {autoActive ? (
                        <button type="button" onClick={stopAuto} className={pillButton}>
                            Stop Auto
                        </button>
                    ) : streamId && (
                        <button type="button" onClick={stopManualCapture} className={pillButton}>
                            Stop Capture
                        </button>
                    )}
                </div>
            </div>
            <div className={`${island} flex-1 p-2`}>
            </div>
        </div>
    );
}

// ── Sub-components ──────────────────────────────────────────────────────

/// Centered placeholder shown when no stream is active.
function Placeholder({ children }: { children: React.ReactNode }) {
    return (
        <div className="flex items-center justify-center min-h-50 text-[#6f737a] text-sm">
            {children}
        </div>
    );
}

/// List of capturable windows — user clicks one to start capturing it.
function WindowPicker({ windows, onSelect, onCancel }: {
    windows: WindowInfo[];
    onSelect: (w: WindowInfo) => void;
    onCancel: () => void;
}) {
    return (
        <div className="flex flex-col gap-0.5 max-h-100 overflow-y-auto">
            <div className="flex items-center justify-between px-3 py-2">
                <span className="font-semibold text-sm text-[#bcc0cc]">
                    Select a window to capture
                </span>
                <button type="button" onClick={onCancel} className={pillButton}>
                    Cancel
                </button>
            </div>
            {windows.map((w) => (
                <button
                    type="button"
                    key={w.hwnd}
                    onClick={() => onSelect(w)}
                    className="block w-full px-3 py-2.5 border-none rounded-md bg-transparent text-left text-[13px] cursor-pointer text-[#bcc0cc] hover:bg-white/6"
                >
                    {w.title || `Window ${w.hwnd}`}
                </button>
            ))}
        </div>
    );
}
