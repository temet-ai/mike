import type { SessionFlowComponent } from './SessionFlowComponent';
import type { SessionFlowLifecycle } from './SessionFlowLifecycle';
import type { SessionFlowMetadata } from './SessionFlowMetadata';
import type { SessionFlowOutcome } from './SessionFlowOutcome';
import type { SessionFlowSource } from './SessionFlowSource';
import type { SessionFlowTimeSemantics } from './SessionFlowTimeSemantics';

export type SessionFlowActivity = {
	id: string;
	phase: string;
	actor_id: string;
	source: SessionFlowSource;
	kind: SessionFlowComponent;
	occurred_at: string | null;
	recorded_at: string;
	ended_at: string | null;
	time_semantics: SessionFlowTimeSemantics;
	lifecycle: SessionFlowLifecycle;
	outcome: SessionFlowOutcome | null;
	source_revision: string | null;
	metadata: SessionFlowMetadata;
};
