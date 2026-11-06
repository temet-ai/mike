import { cachedGet } from '$lib/stores/cache/apiCache.svelte';

export interface MemoryEntry {
	id: number;
	session_id: string;
	status: string;
	created_at: string;
	request_body_bytes: number;
	response_body_bytes: number;
	filtered_request_body_bytes: number;
	total_bytes: number;
}

export interface MemoryBreakdown {
	entry_count: number;
	request_body_bytes: number;
	response_body_bytes: number;
	filtered_request_body_bytes: number;
	tool_calls_bytes: number;
	tool_results_bytes: number;
	tool_results_in_body_bytes: number;
	system_prompt_blocks_bytes: number;
	entries_total_bytes: number;
	session_index_keys: number;
	composable_session_index_keys: number;
	session_meta_keys: number;
	in_flight_count: number;
	process_rss_bytes: number | null;
	max_entries: number;
	max_store_bytes: number;
	over_byte_budget: boolean;
	top_entries: MemoryEntry[];
}

export class MemoryApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly url: string
	) {
		super(message);
		this.name = 'MemoryApiError';
	}
}

export async function getMemoryBreakdown(top?: number): Promise<MemoryBreakdown> {
	const path = top !== undefined ? `/api/diagnostics/memory?top=${top}` : '/api/diagnostics/memory';
	return cachedGet(path, async () => {
		const res = await fetch(path, {
			headers: { accept: 'application/json' }
		});
		if (!res.ok) {
			throw new MemoryApiError(
				`GET ${path} failed: ${res.status} ${res.statusText}`,
				res.status,
				path
			);
		}
		return (await res.json()) as MemoryBreakdown;
	});
}
