export type GraphOutcome = 'success' | 'partial' | 'failure' | 'unknown' | (string & {}) | null;

export function graphOutcomeBorderVar(outcome: GraphOutcome): string {
	switch (outcome) {
		case 'success':
			return '--status-success-border';
		case 'partial':
			return '--status-warning-border';
		case 'failure':
			return '--status-danger-border';
		default:
			return '--status-muted-border';
	}
}

export function graphOutcomeColorVar(outcome: GraphOutcome): string {
	switch (outcome) {
		case 'success':
			return '--status-success-fg';
		case 'partial':
			return '--status-warning-fg';
		case 'failure':
			return '--status-danger-fg';
		default:
			return '--text-3';
	}
}

export function graphOutcomeLabel(outcome: GraphOutcome): string {
	switch (outcome) {
		case 'success':
			return 'Success';
		case 'partial':
			return 'Partial';
		case 'failure':
			return 'Failure';
		default:
			return 'Unjudged';
	}
}
