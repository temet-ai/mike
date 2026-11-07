import { ApiError } from '$lib/api';

export type WakeState =
	| 'pending'
	| 'delivering'
	| 'pushed'
	| 'push_failed'
	| 'unreachable'
	| 'appended'
	| 'reverted'
	| 'backed'
	| 'capped_hour'
	| 'capped_correction'
	| 'expired'
	| 'disabled'
	| 'operator_rejected'
	| 'abandoned';

export interface WakeFields {
	delivery_channel?: 'append' | 'wake' | null;
	deliver_after?: string | null;
	wake_state?: WakeState | string | null;
	delivered_request_id?: number | null;
	reverted_at?: string | null;
	revert_reason?: string | null;
}

export function wakeLabel(f: WakeFields): string | null {
	if (!f.delivery_channel) return null;
	if (f.delivered_request_id) return `carried by request ${f.delivered_request_id}`;
	if (f.reverted_at) return `dropped: ${f.revert_reason ?? 'reverted'}`;
	if (f.delivery_channel === 'append') return 'queued for the next request';
	if (f.wake_state === 'pending' && f.deliver_after) {
		const t = new Date(f.deliver_after);
		const hh = String(t.getUTCHours()).padStart(2, '0');
		const mm = String(t.getUTCMinutes()).padStart(2, '0');
		const ss = String(t.getUTCSeconds()).padStart(2, '0');
		return `pending until ${hh}:${mm}:${ss}Z`;
	}
	return (f.wake_state ?? 'wake').replaceAll('_', ' ');
}

async function send<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(path, {
		method,
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	if (!res.ok) {
		let message = `${method} ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const parsed = (await res.json()) as { error?: string };
			if (parsed.error) message = parsed.error;
		} catch {
			message = `${method} ${path} failed: ${res.status}`;
		}
		throw new ApiError(message, res.status, path);
	}
	return (await res.json()) as T;
}

export interface SteerQuestionStatus {
	question: string;
	policy: { id: number; steerer_id: number; mode: string; holdback_pct: number | null };
	effective_holdback_pct: number;
	bar: { min_n: number; min_precision: number; targets: string[] };
	gate: Record<
		string,
		{
			passes?: boolean;
			reasons?: string[];
			bounds?: { precision: number | null; low: number | null; high: number | null; n: number };
		}
	>;
	live: { arms: { treated: number; holdback: number } };
	effect: { status: string; primary: { rd: number | null; ci: [number, number] | null } };
	promotions: {
		id: number;
		from_mode: string;
		to_mode: string;
		action: string;
		decided_at: string;
	}[];
}

export interface SteerQuestionsResponse {
	steerer_id: number;
	questions: SteerQuestionStatus[];
}

export function getSteerQuestions(): Promise<SteerQuestionsResponse> {
	return send('GET', '/api/steering/questions');
}

export function promoteQuestion(
	steererId: number,
	question: string,
	toMode: string,
	overrideReason?: string
): Promise<unknown> {
	return send('POST', `/api/steering/questions/${steererId}/${question}/promote`, {
		to_mode: toMode,
		override_reason: overrideReason || undefined
	});
}

export function demoteQuestion(
	steererId: number,
	question: string,
	toMode: string,
	reason: string
): Promise<unknown> {
	return send('POST', `/api/steering/questions/${steererId}/${question}/demote`, {
		to_mode: toMode,
		reason
	});
}

export function setHoldback(
	steererId: number,
	question: string,
	pct: number | null
): Promise<unknown> {
	return send('PATCH', `/api/steering/questions/${steererId}/${question}`, { holdback_pct: pct });
}

export function labelEvaluation(
	uuid: string,
	label: 'right' | 'wrong',
	note?: string
): Promise<unknown> {
	return send('POST', `/api/steering/evaluations/${uuid}/label`, { label, note });
}
