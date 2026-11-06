import type { IncomingMessage, ServerResponse } from 'node:http';
import type { AnthropicCredentials, FetchFn } from '../src/inspectClient.js';

/** A minimal node ServerResponse stand-in that records every write for assertions. */
export class FakeResponse {
	statusCode = 200;
	headers: Record<string, unknown> = {};
	chunks: string[] = [];
	headersSent = false;
	ended = false;

	private closeListeners: Array<() => void> = [];

	writeHead(status: number, headers?: Record<string, unknown>): this {
		this.statusCode = status;
		this.headers = { ...this.headers, ...headers };
		this.headersSent = true;
		return this;
	}
	write(chunk: string): boolean {
		this.chunks.push(chunk);
		return true;
	}
	end(chunk?: string): void {
		if (chunk) this.chunks.push(chunk);
		this.ended = true;
	}
	/** Minimal EventEmitter stand-in — `handlePostMessage` listens for 'close' to abort an in-flight turn. */
	on(event: string, listener: () => void): this {
		if (event === 'close') this.closeListeners.push(listener);
		return this;
	}
	/** Test helper: simulates the client disconnecting. */
	simulateClose(): void {
		for (const listener of this.closeListeners) listener();
	}
	get body(): string {
		return this.chunks.join('');
	}
}

/** A minimal IncomingMessage stand-in: async-iterates the given body chunks. */
export function fakeRequest(method: string, url: string, jsonBody?: unknown): IncomingMessage {
	const chunks = jsonBody === undefined ? [] : [Buffer.from(JSON.stringify(jsonBody))];
	const iterator = (async function* () {
		for (const c of chunks) yield c;
	})();
	return Object.assign(iterator, { method, url }) as unknown as IncomingMessage;
}

export function asResponse(fake: FakeResponse): ServerResponse {
	return fake as unknown as ServerResponse;
}

/** A route table for a stub fetch: exact "METHOD path" -> a JSON body or a responder function. */
export function stubFetch(
	routes: Record<string, unknown | ((init?: RequestInit) => unknown)>
): FetchFn {
	return (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = typeof input === 'string' ? input : input.toString();
		const method = (init?.method ?? 'GET').toUpperCase();
		const path = new URL(url).pathname + new URL(url).search;
		const key = `${method} ${path}`;
		const exactKey = Object.keys(routes).find((k) => k === key);
		// Prefix match only forgives a trailing query string on an otherwise-exact
		// path (`key === k + '?...'`) — it must NOT match a route key that is
		// merely a path ancestor (`'GET /api/settings'` would otherwise wrongly
		// swallow `'GET /api/settings/assistant/anthropic/credentials'`, a
		// distinct endpoint one directory down). Longest match wins if several
		// stubs' query-less paths agree.
		const prefixKey = Object.keys(routes)
			.filter((k) => key.startsWith(k) && key.length > k.length && key[k.length] === '?')
			.sort((a, b) => b.length - a.length)[0];
		const matched = exactKey ?? prefixKey;
		if (matched === undefined) {
			return new Response(JSON.stringify({ error: `no stub for ${key}` }), { status: 404 });
		}
		const entry = routes[matched];
		const payload = typeof entry === 'function' ? entry(init) : entry;
		return new Response(JSON.stringify(payload), {
			status: 200,
			headers: { 'content-type': 'application/json' }
		});
	}) as FetchFn;
}

/**
 * A `stubFetch` route entry for `InspectClient.getAnthropicCredentials()`.
 * Spread into a `stubFetch({...})` route table the same way the inline
 * `'GET /api/settings'`/`'GET /api/settings/features'` entries are, e.g.
 * `stubFetch({ ...anthropicCredentialsRoute('api_key', null), ... })`. Omitting
 * this route entirely (the default in most tests) exercises the 404
 * degrade-gracefully path, which callers catch and fall back to
 * `{ mode: 'claude_code_login', api_key: null }`.
 */
export function anthropicCredentialsRoute(
	mode: AnthropicCredentials['mode'],
	apiKey: string | null
): Record<string, AnthropicCredentials> {
	return {
		'GET /api/settings/assistant/anthropic/credentials': { mode, api_key: apiKey }
	};
}
