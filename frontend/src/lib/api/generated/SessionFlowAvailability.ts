export type SessionFlowAvailability =
	| { eligible: { capture_version: number; admitted_at: string; producer: string } }
	| { unsupported_version: { capture_version: number } }
	| 'no_admitted_start';
