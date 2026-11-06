import { ApiError } from '$lib/api';
import { cachedGet, invalidateApiCache } from '$lib/stores/cache/apiCache.svelte';

export interface AnthropicIntegrationStatus {
	mode: 'claude_code_login' | 'api_key';
	configured: boolean;
	masked_key: string | null;
}

const ANTHROPIC_STATUS_PATH = '/api/settings/assistant/anthropic';

export async function getAnthropicIntegration(): Promise<AnthropicIntegrationStatus> {
	// 'passthrough': this status is mutated by direct user action on the same page (mode
	// radio, key save/clear) and re-read immediately after via onStatusChanged — a cheap,
	// always-fresh GET avoids racing the mem/IndexedDB cache invalidation that a mutation
	// elsewhere in the settings surface (e.g. patchSetting's fire-and-forget cache clear)
	// does not guarantee has completed by the time this refetch runs.
	return cachedGet(
		ANTHROPIC_STATUS_PATH,
		async () => {
			const res = await fetch(ANTHROPIC_STATUS_PATH, { headers: { accept: 'application/json' } });
			if (!res.ok) {
				throw new ApiError(
					`GET ${ANTHROPIC_STATUS_PATH} failed: ${res.status} ${res.statusText}`,
					res.status,
					ANTHROPIC_STATUS_PATH
				);
			}
			return (await res.json()) as AnthropicIntegrationStatus;
		},
		'passthrough'
	);
}

export interface AnthropicKeyResult {
	valid: boolean;
	label?: string;
	error?: string;
}

export async function setAnthropicKey(key: string): Promise<AnthropicKeyResult> {
	const path = `${ANTHROPIC_STATUS_PATH}/key`;
	const res = await fetch(path, {
		method: 'PUT',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ key })
	});
	if (res.status !== 200 && res.status !== 422) {
		throw new ApiError(`PUT ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	const result = (await res.json()) as AnthropicKeyResult;
	if (result.valid) await invalidateApiCache(ANTHROPIC_STATUS_PATH);
	return result;
}

export async function clearAnthropicKey(): Promise<void> {
	const path = `${ANTHROPIC_STATUS_PATH}/key`;
	const res = await fetch(path, {
		method: 'DELETE',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) {
		throw new ApiError(`DELETE ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	await invalidateApiCache(ANTHROPIC_STATUS_PATH);
}

export interface AnthropicTestResult {
	ok: boolean;
	message: string;
}

export async function testAnthropicConnection(key?: string): Promise<AnthropicTestResult> {
	const path = `${ANTHROPIC_STATUS_PATH}/test`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify(key !== undefined ? { key } : {})
	});
	if (!res.ok) {
		throw new ApiError(`POST ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as AnthropicTestResult;
}

export interface AnthropicCliStatus {
	installed: boolean;
	binary: string;
	version: string | null;
	logged_in: boolean;
	auth_method: string | null;
	email: string | null;
	org_id: string | null;
	org_name: string | null;
	subscription_type: string | null;
	error: string | null;
}

const CLI_STATUS_PATH = `${ANTHROPIC_STATUS_PATH}/cli-status`;
const CLI_LOGIN_PATH = `${ANTHROPIC_STATUS_PATH}/cli-login`;
const CLI_LOGOUT_PATH = `${ANTHROPIC_STATUS_PATH}/cli-logout`;

export async function getAnthropicCliStatus(): Promise<AnthropicCliStatus> {
	return cachedGet(
		CLI_STATUS_PATH,
		async () => {
			const res = await fetch(CLI_STATUS_PATH, { headers: { accept: 'application/json' } });
			if (!res.ok) {
				throw new ApiError(
					`GET ${CLI_STATUS_PATH} failed: ${res.status} ${res.statusText}`,
					res.status,
					CLI_STATUS_PATH
				);
			}
			return (await res.json()) as AnthropicCliStatus;
		},
		'passthrough'
	);
}

export type AnthropicCliLoginPhase =
	| { state: 'starting' }
	| { state: 'awaiting_code'; url: string }
	| { state: 'succeeded' }
	| { state: 'failed'; message: string }
	| { state: 'cancelled' };

export interface AnthropicCliLoginStatus {
	id: string;
	state: AnthropicCliLoginPhase['state'];
	url?: string;
	message?: string;
}

export interface StartAnthropicCliLoginResult {
	ok: boolean;
	login?: AnthropicCliLoginStatus;
	error?: string;
	fallback_command?: string;
}

export async function startAnthropicCliLogin(): Promise<StartAnthropicCliLoginResult> {
	const res = await fetch(CLI_LOGIN_PATH, {
		method: 'POST',
		headers: { accept: 'application/json' }
	});
	if (res.status !== 200 && res.status !== 409 && res.status !== 501) {
		throw new ApiError(
			`POST ${CLI_LOGIN_PATH} failed: ${res.status} ${res.statusText}`,
			res.status,
			CLI_LOGIN_PATH
		);
	}
	return (await res.json()) as StartAnthropicCliLoginResult;
}

export async function getAnthropicCliLogin(id: string): Promise<AnthropicCliLoginStatus | null> {
	const path = `${CLI_LOGIN_PATH}/${encodeURIComponent(id)}`;
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (res.status === 404) return null;
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as AnthropicCliLoginStatus;
}

export async function submitAnthropicCliLoginCode(id: string, code: string): Promise<boolean> {
	const path = `${CLI_LOGIN_PATH}/${encodeURIComponent(id)}/code`;
	const res = await fetch(path, {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ code })
	});
	return res.ok;
}

export async function cancelAnthropicCliLogin(id: string): Promise<void> {
	const path = `${CLI_LOGIN_PATH}/${encodeURIComponent(id)}/cancel`;
	await fetch(path, { method: 'POST', headers: { accept: 'application/json' } });
}

export interface AnthropicCliLogoutResult {
	ok: boolean;
	error?: string;
}

export async function logoutAnthropicCli(): Promise<AnthropicCliLogoutResult> {
	const res = await fetch(CLI_LOGOUT_PATH, {
		method: 'POST',
		headers: { accept: 'application/json' }
	});
	if (!res.ok) {
		throw new ApiError(
			`POST ${CLI_LOGOUT_PATH} failed: ${res.status} ${res.statusText}`,
			res.status,
			CLI_LOGOUT_PATH
		);
	}
	const result = (await res.json()) as AnthropicCliLogoutResult;
	await invalidateApiCache(CLI_STATUS_PATH);
	return result;
}

export interface CodexRateLimitWindow {
	usedPercent: number;
	windowDurationMins: number | null;
	resetsAt: number | null;
}

export interface CodexIntegrationStatus {
	installed: boolean;
	logged_in: boolean;
	account_type: string | null;
	plan_type: string | null;
	email: string | null;
	selected: boolean;
	rate_limits: {
		rateLimits?: { primary?: CodexRateLimitWindow | null; secondary?: CodexRateLimitWindow | null };
	} | null;
	usage: {
		summary?: { lifetimeTokens?: number | null; peakDailyTokens?: number | null };
		dailyUsageBuckets?: Array<{ startDate: string; tokens: number }> | null;
	} | null;
	error: string | null;
}

const CODEX_STATUS_PATH = '/api/assistant/integrations/codex';

export async function getCodexIntegration(): Promise<CodexIntegrationStatus> {
	const res = await fetch(CODEX_STATUS_PATH, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(
			`GET ${CODEX_STATUS_PATH} failed: ${res.status} ${res.statusText}`,
			res.status,
			CODEX_STATUS_PATH
		);
	}
	return (await res.json()) as CodexIntegrationStatus;
}
