import type { OperatorTurnDto } from '$lib/types';

export type VerdictSource =
	| 'judge'
	| 'operator'
	| 'frustration close'
	| 'no verdict'
	| 'legacy'
	| 'pending';

type ProjectionFields = Pick<
	OperatorTurnDto,
	'outcome' | 'outcome_source' | 'reaction' | 'delivery' | 'legacy' | 'pending'
>;

export function verdictSource(turn: ProjectionFields): VerdictSource {
	switch (turn.outcome_source) {
		case 'judge':
			return 'judge';
		case 'operator':
			return 'operator';
		case 'frustration_close':
			return 'frustration close';
		case 'no_verdict':
			return 'no verdict';
	}
	if (turn.legacy ?? (turn.outcome !== 'unknown' && turn.outcome !== '')) return 'legacy';
	return 'pending';
}

const SOURCE_TIP: Record<VerdictSource, string> = {
	judge: 'Label written from the live turn judge verdict of the promoted bundle.',
	operator: 'Label written from your gold label; it always wins over the judge.',
	'frustration close': 'The session closed in frustration on this, its final genuine turn.',
	'no verdict': 'Not judged: the turn is not a real operator turn, or judging failed permanently.',
	legacy: 'A label written before the judge was promoted; it has not been re-judged.',
	pending: 'Not judged yet: waiting for the next genuine message or the next sweep.'
};

export function sourceTip(source: VerdictSource): string {
	return SOURCE_TIP[source];
}

const REACTION_TIP: Record<string, string> = {
	corrected: 'The next genuine message says this turn got something wrong.',
	redirected: 'The operator changed direction for reasons that are not the turn’s fault.',
	accepted: 'The operator built on the turn, approved it, or gave the next step.',
	no_response: 'No genuine message followed.',
	unclear: 'A next message exists but does not read as any of the above.'
};

const DELIVERY_TIP: Record<string, string> = {
	shipped: 'Delivered, with a commit, push, merge, deploy or release in the turn.',
	delivered: 'Reports the asked-for work done, with no irreversible action.',
	partial: 'Some of the ask is done; the rest is left, blocked or still running.',
	none: 'No real work in the turn.',
	question: 'The main output is a question put to the operator.',
	not_applicable: 'The prompt asked for no work.'
};

export function reactionTip(reaction: string): string {
	return REACTION_TIP[reaction] ?? reaction;
}

export function deliveryTip(delivery: string): string {
	return DELIVERY_TIP[delivery] ?? delivery;
}

export function verdictHistoryHref(turnId: number): string {
	return `/api/operator-turns/${turnId}/verdicts`;
}
