import type { RequestSummary } from './types';
import { ORIGIN_CHIP_LABELS } from './turn-origins';

export function isBackgroundRequest(row: Pick<RequestSummary, 'path' | 'max_tokens'>): boolean {
	if (row.path === '/v1/messages/count_tokens') return true;
	if (row.path === '/v1/messages' && row.max_tokens === 1) return true;
	return false;
}

export function turnOriginLabel(row: Pick<RequestSummary, 'turn_origin'>): string {
	return ORIGIN_CHIP_LABELS[row.turn_origin ?? 'primary'] ?? '';
}

export function subagentBadgeLabel(row: {
	turn_origin?: string | null;
	subagent_persona?: string | null;
}): string | null {
	if (row.turn_origin !== 'subagent') return null;
	const persona = row.subagent_persona;
	if (typeof persona === 'string' && persona.length > 0) {
		return `subagent · ${persona}`;
	}
	return 'subagent';
}
