// WebSocket hook for the server-managed string store.
//
// Connects to /api/v1/ws/strings and receives full JSON snapshots whenever
// any value changes.  Used by app.tsx to display well-known string IDs at
// designated locations in the layout (e.g. "test" in the sidebar).

import { useEffect, useState } from "react";

import { connectWs } from "./ws";

/// Returns all server-managed strings as a key-value record.
/// Updates are pushed by the server immediately on change.
export function useStrings(): Record<string, string> {
    const [strings, setStrings] = useState<Record<string, string>>({});

    useEffect(() => {
        const abort = new AbortController();

        connectWs({
            path: "/api/v1/ws/strings",
            signal: abort.signal,
            onTextMessage(text) {
                try {
                    setStrings(JSON.parse(text) as Record<string, string>);
                } catch (e) {
                    console.error("Failed to parse strings WS message:", e);
                }
            },
        });

        return () => abort.abort();
    }, []);

    return strings;
}
