import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';

export interface SettingMeta {
	key: string;
	group: string;
	kind: 'bool' | 'int' | 'float' | 'string' | 'text' | 'string_list' | 'enum';
	scope: 'session' | 'global';
	/**
	 * `next_conversation` covers the prompt assistant's `read_*` sources (G10): the service
	 * snapshots them into a conversation at creation, so a change to `read_*` while a
	 * conversation is open doesn't affect it — only a NEW conversation sees it, unlike
	 * `next_message` (model, effort, instructions), which the service re-reads every message.
	 */
	applies: 'new_sessions' | 'on_restart' | 'next_cycle' | 'next_message' | 'next_conversation';
	default: SettingValue;
	label: string;
	description: string;
	value: SettingValue;
	min_i64?: number;
	max_i64?: number;
	min_f64?: number;
	max_f64?: number;
	max_len?: number;
	enum_variants?: string[];
}

export type SettingValue = boolean | number | string | string[];

export interface CompressionEffective {
	state: 'pure_shadow' | 'sampled_forwarding' | 'forced_forwarding' | 'off' | (string & {});
	mode: string;
	forward_share: number;
	applies: string;
	summary: string;
	session_arm: string;
	/** Absent on backends older than the feature-switch report; treat a missing value as true. */
	enabled?: boolean;
}

export interface SettingsCatalogueResponse {
	settings: SettingMeta[];
	steerer_openrouter_configured_via_env: boolean;
	/** Absent on older backends; the page then renders no forwarding state. */
	file_read_compression_effective?: CompressionEffective;
	/** Absent on older backends; the page then renders no forwarding state. */
	shell_output_compression_effective?: CompressionEffective;
}

export interface FeatureDto {
	key: string;
	enabled: boolean;
	scope: 'session' | 'global';
	applies: string;
}

export interface FeaturesResponse {
	features: FeatureDto[];
}

export interface AppliesNote {
	label: string;
	detail: string;
}

export function toggleAppliesNote(applies: string): AppliesNote {
	switch (applies) {
		case 'new_sessions':
			return {
				label: 'New sessions',
				detail:
					'Applies to sessions that start after the change. Open sessions keep the setting they started with.'
			};
		case 'next_cycle':
			return {
				label: 'Next cycle',
				detail: 'Applies on the next worker cycle, with no restart.'
			};
		case 'immediate':
			return { label: 'At once', detail: 'Applies at once, on or off, with no restart.' };
		case 'live_disable':
			return {
				label: 'Restart to turn on',
				detail:
					'Turning it off takes about a minute, with no restart. Turning it back on needs a restart.'
			};
		case 'on_restart':
		default:
			return { label: 'On restart', detail: 'Applies on the next restart.' };
	}
}

export function describeToggleApplies(applies: string): string {
	return toggleAppliesNote(applies).detail;
}

export async function getFeatures(): Promise<FeatureDto[]> {
	const path = '/api/settings/features';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		const data = (await res.json()) as FeaturesResponse | FeatureDto[];
		return Array.isArray(data) ? data : (data.features ?? []);
	});
}

export async function patchFeature(key: string, enabled: boolean): Promise<FeatureDto> {
	const path = `/api/settings/features/${encodeURIComponent(key)}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: {
			'content-type': 'application/json',
			accept: 'application/json'
		},
		body: JSON.stringify({ enabled })
	});
	if (!res.ok) {
		let message = `PATCH ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	void invalidateApiCache('/api/settings');
	return (await res.json()) as FeatureDto;
}

export type SyncTableCount = [table: string, inserted: number];

export interface SyncReportDto {
	tables: SyncTableCount[];
	total_rows: number;
}

export interface SyncStatusDto {
	last_run_at: string | null;
	dest_reachable: boolean;
	last_report: SyncReportDto;
	last_error: string | null;
}

export async function getSyncStatus(): Promise<SyncStatusDto> {
	const path = '/api/sync/status';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SyncStatusDto;
	});
}

export async function getSettings(): Promise<SettingsCatalogueResponse> {
	const path = '/api/settings';
	return cachedGet(path, async () => {
		const res = await fetch(path, { headers: { accept: 'application/json' } });
		if (!res.ok) {
			throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
		}
		return (await res.json()) as SettingsCatalogueResponse;
	});
}

export async function patchSetting(
	key: string,
	value: SettingValue,
	changeReason?: string
): Promise<SettingMeta> {
	const path = `/api/settings/${encodeURIComponent(key)}`;
	const res = await fetch(path, {
		method: 'PATCH',
		headers: {
			'content-type': 'application/json',
			accept: 'application/json',
			...(changeReason ? { 'X-Change-Reason': encodeURIComponent(changeReason.trim()) } : {})
		},
		body: JSON.stringify({ value })
	});
	if (!res.ok) {
		let message = `PATCH ${path} failed: ${res.status} ${res.statusText}`;
		if (res.status === 503) {
			message =
				'Persistence is disabled — settings cannot be saved until the capture stack restarts with a database.';
		}
		try {
			const errBody = (await res.json()) as { error?: string };
			if (errBody && typeof errBody.error === 'string' && errBody.error.length > 0) {
				message = errBody.error;
			}
		} catch {}
		throw new ApiError(message, res.status, path);
	}
	void invalidateApiCache('/api/settings');
	return (await res.json()) as SettingMeta;
}
