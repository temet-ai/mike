import type { SessionFlowActorKind } from './SessionFlowActorKind';
import type { SessionFlowProvenance } from './SessionFlowProvenance';
import type { SessionFlowSource } from './SessionFlowSource';

export type SessionFlowActor = {
	id: string;
	kind: SessionFlowActorKind;
	session_id: string | null;
	label: string;
	producer_id: string | null;
	provenance: SessionFlowProvenance;
	parent_actor_id: string | null;
	source: SessionFlowSource | null;
};
