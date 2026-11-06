export const TURN_ORIGINS = [
	'primary',
	'primary_first',
	'title_sidecar',
	'subagent',
	'advisor_sidecar',
	'quota_probe',
	'monitor_sidecar',
	'count_tokens_sidecar',
	'web_fetch_sidecar',
	'workflow_subagent',
	'next_input_suggestion_sidecar'
] as const;

export type TurnOrigin = (typeof TURN_ORIGINS)[number];

export const SIDECAR_ORIGINS = [
	'title_sidecar',
	'advisor_sidecar',
	'quota_probe',
	'monitor_sidecar',
	'count_tokens_sidecar',
	'web_fetch_sidecar',
	'next_input_suggestion_sidecar'
] as const satisfies readonly TurnOrigin[];

export type SidecarOrigin = (typeof SIDECAR_ORIGINS)[number];

const SIDECAR_SET: ReadonlySet<string> = new Set(SIDECAR_ORIGINS);

export function isSidecarOrigin(origin: string | undefined): origin is SidecarOrigin {
	return origin !== undefined && SIDECAR_SET.has(origin);
}

export const HOUSEKEEPING_ORIGINS = [
	'title_sidecar',
	'quota_probe',
	'monitor_sidecar',
	'count_tokens_sidecar',
	'web_fetch_sidecar',
	'next_input_suggestion_sidecar'
] as const satisfies readonly SidecarOrigin[];

const HOUSEKEEPING_SET: ReadonlySet<string> = new Set(HOUSEKEEPING_ORIGINS);

export function isHousekeepingOrigin(origin: string | undefined): boolean {
	return origin !== undefined && HOUSEKEEPING_SET.has(origin);
}

export const SIDECAR_BAND_LABELS: Record<SidecarOrigin, string> = {
	quota_probe: 'quota probe',
	title_sidecar: 'title sidecar',
	advisor_sidecar: 'advisor',
	monitor_sidecar: 'monitor',
	count_tokens_sidecar: 'count tokens',
	web_fetch_sidecar: 'web fetch',
	next_input_suggestion_sidecar: 'suggestion probe'
};

export const ORIGIN_CHIP_LABELS: Record<TurnOrigin, string> = {
	primary: '',
	primary_first: 'session start',
	title_sidecar: 'title-gen',
	subagent: 'subagent',
	advisor_sidecar: 'advisor',
	quota_probe: 'quota-probe',
	monitor_sidecar: 'monitor',
	count_tokens_sidecar: 'count-tokens',
	web_fetch_sidecar: 'web-fetch',
	workflow_subagent: 'workflow',
	next_input_suggestion_sidecar: 'suggestion-probe'
};
