export type SessionFlowActorKind =
	| 'session'
	| 'worker'
	| 'unresolved_dispatch'
	| 'sidecar'
	| 'peer'
	| 'upstream'
	| 'tool_service'
	| 'mike';
