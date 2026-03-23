// API client for the LiveServer stream endpoints.
//
// Plain fetch — no Hono RPC dependency.  The response shapes match the
// M4 TS server's JSON output.

export interface StreamInfo {
	id: string;
}

/// Fetch the list of active streams.
export async function fetchStreams(): Promise<StreamInfo[]> {
	const res = await fetch("/api/v1/streams");
	if (!res.ok) return [];
	return res.json();
}
