// API client for the LiveServer string store.
//
// Plain fetch — no Hono RPC dependency.

/// Fetch all string key-value pairs (user + computed).
export async function fetchStrings(): Promise<Record<string, string>> {
	const res = await fetch("/api/strings");
	if (!res.ok) return {};
	return res.json();
}
