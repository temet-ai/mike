import type { EvalRunDto } from '$lib/api/evalRuns';
import { currentRunFor, newestFirst } from '$lib/components/judges/judges';

export function latestReviewableRun(runs: EvalRunDto[], liveId: number): EvalRunDto | null {
	if (liveId > 0) {
		const current = currentRunFor(runs, liveId);
		if (current) return current;
	}
	const newest = newestFirst(runs);
	return newest.find((r) => r.status === 'complete') ?? newest[0] ?? null;
}
