import { ApiError } from '$lib/api';
import { patchFeature, patchSetting } from '$lib/api/settings';
import type { FeatureDto } from '$lib/api/settings';

export type CheckStatus = 'ok' | 'warn' | 'fail';

export interface SetupCheck {
	id: string;
	name: string;
	status: CheckStatus;
	detail: string;
	setup_step: string | null;
	fix: string | null;
}

export interface DoctorReport {
	version: string;
	passed: boolean;
	ok: number;
	warn: number;
	fail: number;
	checks: SetupCheck[];
}

export interface ClaudeCodeState {
	route: 'sidecar' | 'plain' | null;
	function_name: string | null;
	auto_trust: boolean | null;
	mods_in_place: boolean;
}

export type PresetId = 'watch-only' | 'recommended' | 'everything';

export interface FeaturePreset {
	id: PresetId;
	toggles: { key: string; enabled: boolean }[];
	lessons_setting: string;
	session_lessons: boolean;
}

export interface SetupState {
	doctor: DoctorReport;
	claude_code: ClaudeCodeState | null;
	feature_presets: FeaturePreset[];
}

export interface ProviderModel {
	id: string;
	ref: string;
	label: string;
	known_good?: boolean;
}

export interface LlmProvider {
	id: string;
	label: string;
	prefix: string;
	available: boolean;
	detail: string;
	fix: string;
	key_env_var: string | null;
	models: ProviderModel[];
	models_error: string | null;
}

export interface LlmProvidersResponse {
	llm_provider: string;
	providers: LlmProvider[];
}

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as T;
}

export function getSetupState(): Promise<SetupState> {
	return getJson<SetupState>('/api/setup/state');
}

export function getLlmProviders(): Promise<LlmProvidersResponse> {
	return getJson<LlmProvidersResponse>('/api/llm/providers');
}

export function presetMatches(preset: FeaturePreset, features: FeatureDto[]): boolean {
	const known = preset.toggles.filter((toggle) => features.some((f) => f.key === toggle.key));
	return (
		known.length > 0 &&
		known.every((toggle) => features.find((f) => f.key === toggle.key)?.enabled === toggle.enabled)
	);
}

export function currentPreset(presets: FeaturePreset[], features: FeatureDto[]): PresetId | null {
	return presets.find((preset) => presetMatches(preset, features))?.id ?? null;
}

export function switchesTurnedOff(preset: FeaturePreset, features: FeatureDto[]): string[] {
	return preset.toggles
		.filter(
			(toggle) => !toggle.enabled && features.find((f) => f.key === toggle.key)?.enabled === true
		)
		.map((toggle) => toggle.key);
}

export function switchesToChange(
	preset: FeaturePreset,
	features: FeatureDto[]
): { key: string; enabled: boolean }[] {
	return preset.toggles.filter((toggle) => {
		const current = features.find((f) => f.key === toggle.key);
		return current !== undefined && current.enabled !== toggle.enabled;
	});
}

export async function applyFeaturePreset(
	preset: FeaturePreset,
	features: FeatureDto[],
	lessonsNow: boolean | null
): Promise<number> {
	const changes = switchesToChange(preset, features);
	for (const change of changes) {
		await patchFeature(change.key, change.enabled);
	}
	if (lessonsNow !== preset.session_lessons) {
		await patchSetting(preset.lessons_setting, preset.session_lessons);
	}
	return changes.length;
}
