import type { SessionFlowComponent } from './SessionFlowComponent';
import type { SessionFlowRetention } from './SessionFlowRetention';

export type SessionFlowCoverage = {
	component: SessionFlowComponent;
	lower_id: string | null;
	upper_id: string | null;
	returned_through_id: string | null;
	exhausted: boolean;
	retention: SessionFlowRetention;
	capture_complete: boolean | null;
	latest_only: boolean;
	best_effort: boolean;
	omitted_components: Array<SessionFlowComponent>;
	server_observed_at: string;
	stale_reason: string | null;
};
