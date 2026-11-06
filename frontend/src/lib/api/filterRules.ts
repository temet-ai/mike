import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';

export type FilterRuleKind = 'text_substring' | 'lazy_section' | 'xml_tag';

export interface SessionFilterRuleDto {
	rule_id: number;
	kind: FilterRuleKind;
	label: string;
	tag: string | null;
	enabled: boolean;
	fired_turns: number;
}

export async function getSessionFilterRules(sessionId: string): Promise<SessionFilterRuleDto[]> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/filter-rules`;
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SessionFilterRuleDto[];
	});
}

export async function patchFilterRuleEnabled(
	sessionId: string,
	ruleId: number,
	enabled: boolean
): Promise<void> {
	const path = `/api/filter/rules/${ruleId}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ enabled })
	});
	if (!res.ok) {
		throw new ApiError(`PATCH ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	void invalidateApiCache('/api/filter/rules');
	void invalidateApiCache(`/api/sessions/${encodeURIComponent(sessionId)}/filter-rules`);
}
