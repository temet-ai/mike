import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { InspectClient, type FetchFn } from '../src/inspectClient.js';
import {
	SERVICE_TOKEN_HEADER,
	fileServiceTokenReader,
	mikeStateDir,
	serviceTokenPath
} from '../src/serviceToken.js';

const CREDENTIALS = { mode: 'api_key', api_key: 'sk-ant-test' };

/** Records the headers of every request it answers, and answers with `CREDENTIALS`. */
function recordingFetch(seen: Array<Record<string, string>>): FetchFn {
	return (async (_input: RequestInfo | URL, init?: RequestInit) => {
		seen.push({ ...((init?.headers ?? {}) as Record<string, string>) });
		return new Response(JSON.stringify(CREDENTIALS), { status: 200 });
	}) as FetchFn;
}

describe('service token location', () => {
	it('uses the explicitly selected installation state without default lookup', () => {
		expect(
			mikeStateDir({
				MIKE_STATE_DIR: '/private/custom/state',
				XDG_STATE_HOME: '/other'
			})
		).toBe('/private/custom/state');
		expect(() => mikeStateDir({ MIKE_STATE_DIR: 'relative' })).toThrow('must be absolute');
	});

	it('matches the platform default selected installation on macOS', () => {
		expect(mikeStateDir({ HOME: '/Users/test', XDG_STATE_HOME: '/ignored' }, 'darwin')).toBe(
			'/Users/test/Library/Application Support/mike/state'
		);
	});
	it('sits under XDG_STATE_HOME when that is set', () => {
		expect(mikeStateDir({ XDG_STATE_HOME: '/xdg' }, 'linux')).toBe('/xdg/mike');
	});

	it('falls back to ~/.local/state when XDG_STATE_HOME is unset or empty', () => {
		const expected = path.join(os.homedir(), '.local', 'state', 'mike');
		expect(mikeStateDir({}, 'linux')).toBe(expected);
		expect(mikeStateDir({ XDG_STATE_HOME: '' }, 'linux')).toBe(expected);
	});

	it('names the file after the inspect port', () => {
		expect(serviceTokenPath('http://127.0.0.1:9989', { XDG_STATE_HOME: '/xdg' }, 'linux')).toBe(
			'/xdg/mike/service-token.9989'
		);
		expect(serviceTokenPath('http://localhost', { XDG_STATE_HOME: '/xdg' }, 'linux')).toBe(
			'/xdg/mike/service-token.80'
		);
	});

	it('refuses remote, credential-bearing and non-HTTP destinations before reading a token', () => {
		for (const url of [
			'http://remote.example:9989',
			'http://192.168.1.2:9989',
			'http://127.0.0.1.remote.example:9989',
			'http://user:secret@127.0.0.1:9989',
			'https://user@localhost:9989',
			'file://localhost/token',
			'ftp://127.0.0.1:9989'
		])
			expect(serviceTokenPath(url, { MIKE_STATE_DIR: '/private/state' })).toBeNull();
	});

	it('accepts only canonical local loopback hosts for service credentials', () => {
		for (const host of ['localhost', '127.0.0.1', '127.23.45.67', '[::1]']) {
			expect(
				serviceTokenPath(`http://${host}:9989`, {
					MIKE_STATE_DIR: '/private/state'
				})
			).toBe('/private/state/service-token.9989');
		}
	});

	it('has no path for an inspect URL that cannot be parsed', () => {
		expect(serviceTokenPath('not a url', { XDG_STATE_HOME: '/xdg' }, 'linux')).toBeNull();
	});
});

describe('reading the service token', () => {
	let stateHome: string;

	beforeEach(async () => {
		stateHome = await mkdtemp(path.join(os.tmpdir(), 'mike-service-token-'));
	});

	afterEach(async () => {
		await rm(stateHome, { recursive: true, force: true });
	});

	async function publish(port: number, contents: string): Promise<void> {
		const { mkdir } = await import('node:fs/promises');
		await mkdir(path.join(stateHome, 'mike'), { recursive: true });
		await writeFile(path.join(stateHome, 'mike', `service-token.${port}`), contents);
	}

	it('returns the published token, without surrounding whitespace', async () => {
		await publish(9989, 'abc123\n');
		const read = fileServiceTokenReader('http://127.0.0.1:9989', {
			MIKE_STATE_DIR: path.join(stateHome, 'mike')
		});
		expect(await read()).toBe('abc123');
	});

	it('picks up a token mike rewrote after a restart', async () => {
		const read = fileServiceTokenReader('http://127.0.0.1:9989', {
			MIKE_STATE_DIR: path.join(stateHome, 'mike')
		});
		await publish(9989, 'first');
		expect(await read()).toBe('first');
		await publish(9989, 'second');
		expect(await read()).toBe('second');
	});

	it('returns null when nothing is published for that port', async () => {
		await publish(9989, 'abc123');
		const read = fileServiceTokenReader('http://127.0.0.1:19989', {
			MIKE_STATE_DIR: path.join(stateHome, 'mike')
		});
		expect(await read()).toBeNull();
	});

	it('returns null for an empty file', async () => {
		await publish(9989, '\n');
		const read = fileServiceTokenReader('http://127.0.0.1:9989', {
			MIKE_STATE_DIR: path.join(stateHome, 'mike')
		});
		expect(await read()).toBeNull();
	});
});

describe('InspectClient and the service token', () => {
	it('rejects redirects on requests carrying the service credential', async () => {
		const requests: RequestInit[] = [];
		const fetchImpl = (async (_input: RequestInfo | URL, init?: RequestInit) => {
			requests.push(init ?? {});
			return new Response(JSON.stringify(CREDENTIALS), { status: 200 });
		}) as FetchFn;
		const client = new InspectClient('http://127.0.0.1:9989', fetchImpl, async () => 'tok');
		await client.getAnthropicCredentials();
		expect(requests[0].redirect).toBe('error');
		expect((requests[0].headers as Record<string, string>)[SERVICE_TOKEN_HEADER]).toBe('tok');
	});

	it('sends the token on the credentials request', async () => {
		const seen: Array<Record<string, string>> = [];
		const client = new InspectClient(
			'http://inspect.local',
			recordingFetch(seen),
			async () => 'tok'
		);
		expect(await client.getAnthropicCredentials()).toEqual(CREDENTIALS);
		expect(seen).toHaveLength(1);
		expect(seen[0][SERVICE_TOKEN_HEADER]).toBe('tok');
	});

	it('sends no token header when none is published', async () => {
		const seen: Array<Record<string, string>> = [];
		const client = new InspectClient(
			'http://inspect.local',
			recordingFetch(seen),
			async () => null
		);
		await client.getAnthropicCredentials();
		expect(SERVICE_TOKEN_HEADER in seen[0]).toBe(false);
	});

	it('keeps the token off every other request', async () => {
		const seen: Array<Record<string, string>> = [];
		const client = new InspectClient(
			'http://inspect.local',
			recordingFetch(seen),
			async () => 'tok'
		);
		await client.listProjects();
		await client.listFilterRules();
		expect(seen).toHaveLength(2);
		for (const headers of seen) expect(SERVICE_TOKEN_HEADER in headers).toBe(false);
	});
});
