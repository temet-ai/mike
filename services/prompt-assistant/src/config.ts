import os from 'node:os';
import path from 'node:path';

export interface Config {
	port: number;
	host: string;
	inspectUrl: string;
	captureBaseUrl: string;
	stateDir: string;
	sweepIntervalMs: number;
	/** `~/.claude` — root the `read_memory` tool reads `projects/<slug>/memory/*.md` under. */
	claudeHome: string;
}

function intFromEnv(name: string, fallback: number): number {
	const raw = process.env[name];
	if (raw === undefined || raw.trim() === '') return fallback;
	const n = Number.parseInt(raw, 10);
	return Number.isFinite(n) ? n : fallback;
}

/**
 * Reads process.env fresh on every call rather than caching a module-level
 * singleton, so tests can flip env vars between cases without reimporting.
 */
export function loadConfig(): Config {
	const port = intFromEnv('PROMPT_ASSISTANT_PORT', 9985);
	const host = (process.env.PROMPT_ASSISTANT_HOST ?? '127.0.0.1').trim() || '127.0.0.1';
	const inspectUrl = (process.env.INSPECT_URL ?? 'http://127.0.0.1:9989').trim();
	const captureBaseUrl = (
		process.env.PROMPT_ASSISTANT_CAPTURE_BASE_URL ?? 'http://localhost:9988'
	).trim();
	const stateDir = (
		process.env.PROMPT_ASSISTANT_STATE_DIR ??
		path.join(os.homedir(), '.local', 'state', 'mike', 'prompt-assistant')
	).trim();
	const sweepIntervalMs = intFromEnv('PROMPT_ASSISTANT_SWEEP_INTERVAL_MS', 60 * 60 * 1000);
	const claudeHome = (
		process.env.PROMPT_ASSISTANT_CLAUDE_HOME ?? path.join(os.homedir(), '.claude')
	).trim();
	return { port, host, inspectUrl, captureBaseUrl, stateDir, sweepIntervalMs, claudeHome };
}
