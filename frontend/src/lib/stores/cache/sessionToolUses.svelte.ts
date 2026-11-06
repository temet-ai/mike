import { listSessionToolUses } from '$lib/api';
import type { ToolUseDto } from '$lib/types';

const PAGE_LIMIT = 1000;

const cache = new Map<string, ToolUseDto[]>();

const inFlight = new Map<string, Promise<ToolUseDto[]>>();

export function loadSessionToolUses(sessionId: string): Promise<ToolUseDto[]> {
	const done = cache.get(sessionId);
	if (done !== undefined) return Promise.resolve(done);

	const existing = inFlight.get(sessionId);
	if (existing) return existing;

	const promise = (async (): Promise<ToolUseDto[]> => {
		try {
			const rows: ToolUseDto[] = [];
			let offset = 0;
			for (;;) {
				const page = await listSessionToolUses(sessionId, {
					...(offset > 0 ? { offset } : {}),
					limit: PAGE_LIMIT,
					order: 'asc'
				});
				rows.push(...page.rows);
				offset += page.rows.length;
				if (page.rows.length === 0 || offset >= page.total_count) break;
			}
			cache.set(sessionId, rows);
			return rows;
		} finally {
			inFlight.delete(sessionId);
		}
	})();

	inFlight.set(sessionId, promise);
	return promise;
}

export function invalidateSessionToolUses(sessionId: string): void {
	cache.delete(sessionId);
}

export function resetSessionToolUsesCache(): void {
	cache.clear();
	inFlight.clear();
}
