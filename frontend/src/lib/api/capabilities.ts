import { ApiError } from '$lib/api';
import { cachedGet } from '$lib/stores/cache/apiCache.svelte';

export type CapabilityState = 'active' | 'degraded' | 'inert' | 'off';

export interface CapabilityRow {
	id: string;
	label: string;
	state: CapabilityState;
	detail: string;
	fix: string;
}

export interface CapabilitiesResponse {
	capabilities: CapabilityRow[];
}

export async function getCapabilities(): Promise<CapabilityRow[]> {
	const path = '/api/capabilities';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		const data = (await res.json()) as CapabilitiesResponse;
		return data.capabilities ?? [];
	});
}
