import { readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

/** Header mike's credentials route checks; mirrors `HEADER` in crates/mike-features/src/service_token.rs. */
export const SERVICE_TOKEN_HEADER = 'x-mike-service-token';

export type ServiceTokenReader = () => Promise<string | null>;

/** Set MIKE_STATE_DIR to print-config --paths's state_dir for a selected installation. */
export function mikeStateDir(
	env: NodeJS.ProcessEnv = process.env,
	platform: NodeJS.Platform = process.platform
): string {
	if (env.MIKE_STATE_DIR) {
		if (!path.isAbsolute(env.MIKE_STATE_DIR)) throw new Error('MIKE_STATE_DIR must be absolute');
		return env.MIKE_STATE_DIR;
	}
	const home = env.HOME || os.homedir();
	if (platform === 'darwin')
		return path.join(home, 'Library', 'Application Support', 'mike', 'state');
	const xdg = env.XDG_STATE_HOME;
	const base = xdg && xdg.length > 0 ? xdg : path.join(home, '.local', 'state');
	return path.join(base, 'mike');
}

/**
 * Where the mike that serves `inspectUrl` published its token. The file name
 * carries the inspect port, so a second mike on other ports (a deploy smoke
 * run) never overwrites the token of the one this service talks to.
 */
export function serviceTokenPath(
	inspectUrl: string,
	env: NodeJS.ProcessEnv = process.env,
	platform: NodeJS.Platform = process.platform
): string | null {
	try {
		const url = new URL(inspectUrl);
		if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) return null;
		if (!isLoopbackHost(url.hostname)) return null;
		const port = url.port || (url.protocol === 'https:' ? '443' : '80');
		return path.join(mikeStateDir(env, platform), `service-token.${port}`);
	} catch {
		return null;
	}
}

function isLoopbackHost(host: string): boolean {
	return host === 'localhost' || host === '[::1]' || /^127(?:\.\d{1,3}){3}$/.test(host);
}

/**
 * Reads the token fresh on every call: mike writes a new one each time it
 * starts, and this service outlives mike restarts. A missing or unreadable
 * file yields `null`, and the credentials request then goes out without the
 * header and is refused.
 */
export function fileServiceTokenReader(
	inspectUrl: string,
	env: NodeJS.ProcessEnv = process.env
): ServiceTokenReader {
	return async () => {
		const file = serviceTokenPath(inspectUrl, env);
		if (file === null) return null;
		try {
			const token = (await readFile(file, 'utf8')).trim();
			return token.length > 0 ? token : null;
		} catch {
			return null;
		}
	};
}
