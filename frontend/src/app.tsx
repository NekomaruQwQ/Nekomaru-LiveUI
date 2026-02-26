import { useCaptureControl, type WindowInfo } from './capture';
import { StreamRenderer } from './stream';

// ── Styles (JetBrains Islands) ──────────────────────────────────────────

/// Shared island panel style — dark surface, subtle border, soft shadow.
const island = "border border-[#393b40] rounded-2xl shadow-[0_1px_3px_rgba(0,0,0,0.3),0_4px_12px_rgba(0,0,0,0.15)] bg-[#2b2d30]";

/// Shared button style — dark pill with hover highlight.
const pillButton = "px-4 py-1.5 border border-[#4e5157] rounded-md bg-[#3c3f44] text-[#bcc0cc] text-[13px] cursor-pointer transition-colors duration-150 hover:bg-[#4a4d52]";

// ── App ─────────────────────────────────────────────────────────────────

export function App() {
    const capture = useCaptureControl();

    return (
        <div className="flex flex-col flex-1 gap-2 p-2">
            <div className="flex flex-row flex-5 gap-2">
                <div className={`${island} flex-3 min-w-0 overflow-hidden`}>
                    {capture.loading ? (
                        <Placeholder>Connecting...</Placeholder>
                    ) : capture.streamId ? (
                        <StreamRenderer streamId={capture.streamId} />
                    ) : capture.autoActive ? (
                        <Placeholder>Auto-selecting...</Placeholder>
                    ) : capture.windows ? (
                        <WindowPicker
                            windows={capture.windows}
                            onSelect={capture.startCapture}
                            onCancel={capture.dismissWindows}
                        />
                    ) : (
                        <Placeholder>
                            <div className="flex gap-2">
                                <button type="button" onClick={capture.startAuto} className={pillButton}>
                                    Auto
                                </button>
                                <button type="button" onClick={capture.loadWindows} className={pillButton}>
                                    Pick Window
                                </button>
                            </div>
                        </Placeholder>
                    )}
                </div>
                <div className={`${island} flex-1 p-6 flex flex-col gap-3`}>
                    <span className="text-[#bcc0cc]">Hi, I'm Nekomaru OwO</span>
                    {capture.autoActive ? (
                        <button type="button" onClick={capture.stopAuto} className={pillButton}>
                            Stop Auto
                        </button>
                    ) : capture.streamId && (
                        <button type="button" onClick={capture.stopManualCapture} className={pillButton}>
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
