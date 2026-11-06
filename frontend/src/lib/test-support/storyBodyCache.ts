import type { BodyPayload } from '$lib/types';
import type { BodyKind } from '$lib/stores/cache/cacheConfig';
import { __resetCacheForTests, loadBody } from '$lib/stores/cache/unifiedCache.svelte';

export type { BodyKind };

export async function primeBodyCache(
	id: number,
	kind: BodyKind,
	text: string | null
): Promise<void> {
	__resetCacheForTests();
	if (text === null) return;

	const origFetch = globalThis.fetch;
	const stubFetch: typeof fetch = async () => {
		const payload: BodyPayload = {
			text,
			byte_size: text.length,
			has_body: true
		};
		return new Response(JSON.stringify(payload), {
			status: 200,
			headers: { 'content-type': 'application/json' }
		});
	};
	globalThis.fetch = stubFetch;
	try {
		await loadBody(id, kind);
	} finally {
		globalThis.fetch = origFetch;
	}
}
