import type { SessionFlowActivity } from './SessionFlowActivity';
import type { SessionFlowActor } from './SessionFlowActor';
import type { SessionFlowAvailability } from './SessionFlowAvailability';
import type { SessionFlowCoverage } from './SessionFlowCoverage';
import type { SessionFlowRelationship } from './SessionFlowRelationship';

export type SessionFlowPage = {
	version: number;
	session_id: string;
	availability: SessionFlowAvailability;
	actors: Array<SessionFlowActor>;
	activities: Array<SessionFlowActivity>;
	relationships: Array<SessionFlowRelationship>;
	coverage: SessionFlowCoverage;
	next_cursor: string | null;
};
