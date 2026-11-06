// The SDK subprocess's env is built from an allowlist, never `{ ...process.env }`.
// This service runs INSIDE a session started by an operator's own Claude Code —
// see G2 in artifacts/2026-09-23/prompt-assistant/gap-audit.md: the parent
// session's CLAUDE_EFFORT, CLAUDE_CODE_CHILD_SESSION, CLAUDE_CODE_BRIDGE_SESSION_ID,
// CLAUDE_CODE_MESSAGING_SOCKET/TOKEN, etc. must never reach the agent it spawns —
// CLAUDE_EFFORT in particular overrode the `effort` option outright (an
// `effort:"low"` setting was sent as a high thinking budget because the parent's
// CLAUDE_EFFORT=high leaked through `{ ...process.env }`).

/** Exact env var names always carried over — the process basics the SDK subprocess needs to run at all. */
const ALLOWED_EXACT_VARS = ['PATH', 'HOME', 'USER', 'LANG', 'LC_ALL', 'TMPDIR'] as const;

/** Env var name prefixes always carried over. */
const ALLOWED_PREFIXES = ['NODE_'] as const;

/**
 * Auth-relevant vars the SDK's own credential resolution needs: API key, OAuth
 * token/profile, and Workload Identity Federation. None of these are
 * `CLAUDE_*`/`CLAUDE_CODE_*` — the service's README already documents that it
 * relies on a `claude` CLI login, not an API key, but any of these that IS set
 * must still reach the subprocess rather than being silently dropped.
 */
const ALLOWED_AUTH_VARS = [
	'ANTHROPIC_API_KEY',
	'ANTHROPIC_AUTH_TOKEN',
	'ANTHROPIC_PROFILE',
	'ANTHROPIC_ORGANIZATION_ID',
	'ANTHROPIC_WORKSPACE_ID',
	'ANTHROPIC_FEDERATION_RULE_ID',
	'ANTHROPIC_SERVICE_ACCOUNT_ID',
	'ANTHROPIC_IDENTITY_TOKEN_FILE',
	'ANTHROPIC_IDENTITY_TOKEN'
] as const;

export interface BuildSdkEnvOptions {
	capture: boolean;
	captureBaseUrl: string;
	/**
	 * The saved Anthropic API key when the operator's Anthropic-integration
	 * setting (mike's `/api/settings/assistant/anthropic/credentials`) is in
	 * `'api_key'` mode and a key is actually saved. When this is a non-empty
	 * string it OVERRIDES `env.ANTHROPIC_API_KEY`, replacing whatever the
	 * `ALLOWED_AUTH_VARS` pass-through below would otherwise have contributed
	 * from `process.env.ANTHROPIC_API_KEY` — `api_key` mode is an explicit,
	 * independent credential and must never silently fall back to the
	 * operator's own env var or `claude` CLI login. Leave it `undefined`/`null`/
	 * empty for `'claude_code_login'` mode (or when no key is saved), which
	 * keeps the existing allowlist pass-through unchanged.
	 */
	anthropicApiKeyOverride?: string | null;
}

/**
 * Builds the env object passed as the SDK's `Options.env`. That field REPLACES
 * the subprocess environment entirely (it is not merged with `process.env`),
 * so this is the only place the allowlist needs enforcing. `ANTHROPIC_BASE_URL`
 * is set explicitly from `captureBaseUrl` when `capture` is on, and otherwise
 * omitted even if the parent process happens to have one set — never carried
 * through by the `ANTHROPIC_` prefix, which is deliberately not in
 * `ALLOWED_PREFIXES`. `ANTHROPIC_CUSTOM_HEADERS` (newline-separated `Name: Value`
 * pairs, per the SDK's own parsing) rides alongside `ANTHROPIC_BASE_URL`: the
 * `x-mike-client: prompt-assistant` header it sets is the G12a mechanism
 * (artifacts/2026-09-23/prompt-assistant/gap-audit.md) that tells the proxy's
 * outbound mutation pipeline (filter rules, prompt compositions, injections) to
 * pass this agent's own traffic through unmodified while still capturing it.
 */
export function buildSdkEnv(
	processEnv: NodeJS.ProcessEnv,
	options: BuildSdkEnvOptions
): Record<string, string | undefined> {
	const env: Record<string, string | undefined> = {};
	for (const key of ALLOWED_EXACT_VARS) {
		const value = processEnv[key];
		if (value !== undefined) env[key] = value;
	}
	for (const key of ALLOWED_AUTH_VARS) {
		const value = processEnv[key];
		if (value !== undefined) env[key] = value;
	}
	for (const [key, value] of Object.entries(processEnv)) {
		if (value === undefined) continue;
		if (ALLOWED_PREFIXES.some((prefix) => key.startsWith(prefix))) {
			env[key] = value;
		}
	}
	if (options.capture) {
		env.ANTHROPIC_BASE_URL = options.captureBaseUrl;
		env.ANTHROPIC_CUSTOM_HEADERS = 'x-mike-client: prompt-assistant';
	}
	if (options.anthropicApiKeyOverride) {
		env.ANTHROPIC_API_KEY = options.anthropicApiKeyOverride;
	}
	return env;
}
