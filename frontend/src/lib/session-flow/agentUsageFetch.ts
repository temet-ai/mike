import { getRequest } from '$lib/api';
import type { RequestDetail } from '$lib/types';

const detailCache = new Map<number, RequestDetail>();
const CONCURRENCY = 6;

export async function fetchRequestDetails(
	ids: readonly number[],
	isStale: () => boolean
): Promise<RequestDetail[]> {
	const results: RequestDetail[] = [];
	const queue = [...ids];
	async function worker(): Promise<void> {
		for (;;) {
			if (isStale()) return;
			const id = queue.shift();
			if (id === undefined) return;
			let detail = detailCache.get(id);
			if (!detail) {
				detail = await getRequest(id);
				detailCache.set(id, detail);
			}
			if (isStale()) return;
			results.push(detail);
		}
	}
	const workerCount = Math.min(CONCURRENCY, ids.length);
	await Promise.all(Array.from({ length: workerCount }, worker));
	return results;
}

export function clearRequestDetailCache(): void {
	detailCache.clear();
}
