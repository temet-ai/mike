import { ApiError } from '$lib/api';

export type SettingBasis = 'session_frozen' | 'change_log_at_session_start' | 'current_settings';
export type ToggleScope = 'session' | 'global';
export type ToggleApplies = 'new_sessions' | 'on_restart' | 'immediate' | 'live_disable';
export type TopicState = 'recalled' | 'delivered_in_full' | 'available';
export type SkillState = 'loaded' | 'load_failed';

export interface DeliveryProject {
	id: number;
	path: string;
	name: string;
	resolution: string;
}

export interface DeliveryModel {
	model: string | null;
	request_count: number;
	requests_with_manifest: number;
}

export interface DeliveryAgent {
	persona: string | null;
	agent_name: string | null;
	first_request_id: number | null;
	started_at: string | null;
}

export interface DeliveryToggle {
	key: string;
	scope: ToggleScope;
	applies: ToggleApplies;
	enabled: boolean | null;
	basis: SettingBasis;
	enabled_now: boolean;
	changed_since_session_start: boolean;
}

export interface DeliveryLimits {
	inject_lessons: boolean;
	max_lessons_to_inject: number;
	max_lessons_per_topic: number;
	basis: SettingBasis;
}

export interface DeliveryToggleChange {
	key: string;
	changed_at: string;
	enabled_before: boolean | null;
	enabled_after: boolean | null;
	via: string;
	during_session: boolean;
}

export interface DeliveryProfile {
	project: DeliveryProject;
	first_seen: string;
	last_seen: string;
	models: DeliveryModel[];
	agents: DeliveryAgent[];
	toggles: DeliveryToggle[];
	limits: DeliveryLimits;
	toggle_changes: DeliveryToggleChange[];
}

export interface DeliveredLesson {
	lesson_id: number;
	text: string;
	kind: string;
	topic: string | null;
	stage: string;
	archived_at: string | null;
	archived_reason: string | null;
	injection_count: number;
	first_injected_at: string;
	last_injected_at: string;
	first_request_id: number;
	last_request_id: number;
	lanes: string[];
	personas: string[];
}

export interface DeliveryTopic {
	topic: string;
	state: TopicState;
	recall_request_ids: number[];
	delivered_lesson_ids: number[];
	available_project_lessons: number;
	available_shared_lessons: number;
}

export interface RecallInjection {
	request_id: number;
	recalled_at: string;
	topics: string[];
	lesson_count: number | null;
}

export interface LoadedSkill {
	name: string | null;
	state: SkillState;
	load_count: number;
	error_count: number;
	first_request_id: number;
	last_request_id: number;
	first_loaded_at: string;
	last_loaded_at: string;
}

export interface RecallToolCall {
	tool_name: string;
	call_count: number;
	error_count: number;
	first_request_id: number;
	last_request_id: number;
	first_called_at: string;
	last_called_at: string;
}

export interface InjectionStage {
	stage: string;
	outcome: string;
	reason_code: string;
	request_count: number;
	first_request_id: number;
	last_request_id: number;
}

export interface EvidenceGap {
	code: string;
	detail: string;
}

export interface SessionDelivery {
	session_uuid: string;
	generated_at: string;
	profile: DeliveryProfile;
	lessons: DeliveredLesson[];
	topics: DeliveryTopic[];
	recall_injections: RecallInjection[];
	skills: LoadedSkill[];
	recall_tool_calls: RecallToolCall[];
	injection_stages: InjectionStage[];
	evidence_gaps: EvidenceGap[];
}

export async function getSessionDelivery(sessionId: string): Promise<SessionDelivery> {
	const path = `/api/sessions/${encodeURIComponent(sessionId)}/delivery`;
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as SessionDelivery;
}
