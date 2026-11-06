export type SessionFlowLifecycle =
	| 'started'
	| 'observed'
	| 'completed'
	| 'queued'
	| 'claimed'
	| 'attempted'
	| 'delivered'
	| 'consumed'
	| 'expired'
	| 'reverted'
	| 'unknown';
