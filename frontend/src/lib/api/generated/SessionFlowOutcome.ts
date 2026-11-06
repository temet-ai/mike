export type SessionFlowOutcome =
	| 'success'
	| 'error'
	| 'applied'
	| 'unchanged'
	| 'skipped'
	| 'failed_closed'
	| 'expired'
	| 'reverted'
	| 'resolved'
	| 'unknown';
