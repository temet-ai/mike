import { ApiError } from '$lib/api';

/** What `POST /api/knowledge/rerank-pass` answers with on 200. `tuned` and `stopped` are Rust debug names or null. */
export interface RerankPassResult {
	graded: number;
	tuned: string | null;
	thread_candidates: number;
	lesson_pairs: number;
	stopped: string | null;
}

/** Runs the overnight reranker pass now. It can take minutes; the request stays open until it finishes. */
export async function runRerankPass(): Promise<RerankPassResult> {
	const path = '/api/knowledge/rerank-pass';
	const res = await fetch(path, { method: 'POST', headers: { accept: 'application/json' } });
	if (!res.ok) {
		let message = `POST ${path} failed: ${res.status} ${res.statusText}`;
		const body = (await res.json().catch(() => null)) as { error?: string } | null;
		if (typeof body?.error === 'string' && body.error.length > 0) message = body.error;
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as RerankPassResult;
}

/** The tuning step's outcome as a sentence part; an unknown name is shown as it came. */
export function describeTuned(tuned: string | null): string {
	if (tuned === null) return 'not reached';
	if (tuned === 'AlreadyToday') return 'already done today';
	if (tuned === 'NoSettingsService') return 'no settings service';
	const few = /^TooFewGrades\((\d+)\)$/.exec(tuned);
	if (few) return `only ${few[1]} graded searches, too few`;
	const rec = /^Recorded \{ kept: (\d+), tried: (\d+) \}$/.exec(tuned);
	if (rec) return `kept ${rec[1]} of ${rec[2]} tried changes`;
	return tuned;
}

/** Why a pass ended before finishing; an unknown name is shown as it came. */
export function describeStopped(stopped: string): string {
	switch (stopped) {
		case 'MachineBusy':
			return 'the machine got busy';
		case 'RerankerFailed':
			return 'the reranker did not answer';
		case 'DatabaseFailed':
			return 'the database failed';
		case 'MoreToGrade':
			return 'more searches are left to grade';
		default:
			return stopped;
	}
}
