import type { SessionFlowComponent } from './SessionFlowComponent';

export type SessionFlowCursor = {
	version: number;
	session_id: string;
	component: SessionFlowComponent;
	filter: string;
	after_id: string;
	upper_id: string;
};
