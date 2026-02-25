import { useEffect, useState } from 'react';

import { api } from './api';
import { StreamRenderer } from './streamRenderer';

/// Default capture resolution (matches RTX 5090 / existing constants).
const DEFAULT_WIDTH = 1920;
const DEFAULT_HEIGHT = 1200;

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
    /// Capturable windows shown in the picker, or null when picker is closed.
    const [windows, setWindows] = useState<WindowInfo[] | null>(null);
    /// True during the initial "do we already have a stream?" check.
    const [loading, setLoading] = useState(true);

    // On mount, check for an existing running stream (e.g. after page reload).
    useEffect(() => {
        (async () => {
            try {
                const res = await api.index.$get();
                if (res.ok) {
                    const streams = await res.json();
                    const running = streams.find((s) => s.status === "running");
                    if (running) {
                        setStreamId(running.id);
                    }
                }
            } catch (e) {
                console.error("Failed to list streams:", e);
            } finally {
                setLoading(false);
            }
        })();
    }, []);

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

    /// Stop the active stream and return to idle.
    async function stopCapture() {
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
                    ) : windows ? (
                        <WindowPicker
                            windows={windows}
                            onSelect={startCapture}
                            onCancel={() => setWindows(null)}
                        />
                    ) : (
                        <Placeholder>
                            <button type="button" onClick={loadWindows} className={pillButton}>
                                Start Capture
                            </button>
                        </Placeholder>
                    )}
                </div>
                <div className={`${island} flex-1 p-6 flex flex-col gap-3`}>
                    <span className="text-[#bcc0cc]">Hi, I'm Nekomaru OwO</span>
                    {streamId && (
                        <button type="button" onClick={stopCapture} className={pillButton}>
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