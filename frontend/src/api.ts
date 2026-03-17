// API client for the LiveServer stream endpoints.
//
// Plain fetch — no Hono RPC dependency.  The response shapes match the Rust
// server's JSON output.

export interface StreamInfo {
	id: string;
	hwnd: string;
	status: "starting" | "running" | "stopped";
	generation: number;
}

/// Fetch the list of active streams.
export async function fetchStreams(): Promise<StreamInfo[]> {
	const res = await fetch("/api/v1/streams");
	if (!res.ok) return [];
	return res.json();
}
