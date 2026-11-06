import type { SessionFlowRelationshipKind } from './SessionFlowRelationshipKind';
import type { SessionFlowSource } from './SessionFlowSource';

export type SessionFlowRelationship = {
	id: string;
	from_id: string | null;
	to_id: string | null;
	kind: SessionFlowRelationshipKind;
	source: SessionFlowSource;
	phase: string;
	occurred_at: string | null;
	recorded_at: string;
};
