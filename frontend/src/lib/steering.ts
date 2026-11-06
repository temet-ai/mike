export type Authority = 'supply_context' | 'redirect' | 'command';
export type SteeringMode = 'augment' | 'drive';
export type Verdict =
	| 'fired'
	| 'suppressed_debounce'
	| 'suppressed_arbitration'
	| 'no_drift'
	| 'gate_miss'
	| 'failed_closed';

export const AUTHORITY_LABELS: Record<Authority, string> = {
	supply_context: 'supply context',
	redirect: 'redirect',
	command: 'command'
};

export const STEERING_MODE_LABELS: Record<SteeringMode, string> = {
	augment: 'augment',
	drive: 'drive'
};

export const VERDICT_LABELS: Record<Verdict, string> = {
	fired: 'fired',
	suppressed_debounce: 'suppressed (debounce)',
	suppressed_arbitration: 'suppressed (arbitration)',
	no_drift: 'no drift',
	gate_miss: 'gate miss',
	failed_closed: 'failed closed'
};

export type VerdictKind = 'fired' | 'suppressed' | 'inert' | 'failed';

export function verdictKind(v: Verdict): VerdictKind {
	switch (v) {
		case 'fired':
			return 'fired';
		case 'suppressed_debounce':
		case 'suppressed_arbitration':
			return 'suppressed';
		case 'no_drift':
		case 'gate_miss':
			return 'inert';
		case 'failed_closed':
			return 'failed';
	}
}

export type InterventionStatus = 'fired' | 'pending' | 'processed' | 'expired' | 'reverted';

export function interventionStatus(
	row: {
		sent_at: string | null;
		processed_at: string | null;
		expires_at: string | null;
		reverted_at: string | null;
	},
	nowMs?: number
): InterventionStatus {
	if (row.reverted_at) return 'reverted';
	if (!row.sent_at) return 'processed';
	if (row.processed_at) return 'processed';
	const now = nowMs ?? Date.now();
	if (row.expires_at && new Date(row.expires_at).getTime() < now) return 'expired';
	return row.sent_at ? 'fired' : 'pending';
}

export const STEERING_PREFIX = "<system-reminder>Steering note from the operator's proxy:\n\n";
export const STEERING_SUFFIX =
	'\n\nIf it is already done or does not apply, say so in a line.</system-reminder>';

export function buildSteeringBlock(inner: string): string {
	return `${STEERING_PREFIX}${inner}${STEERING_SUFFIX}`;
}
