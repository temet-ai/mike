import { afterEach, describe, expect, it, vi } from 'vitest';
import { directClaudeCommand, getConnectionInfo } from './connectionInfo';

afterEach(() => vi.unstubAllGlobals());

function respond(body: unknown, status = 200): ReturnType<typeof vi.fn> {
	const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status }));
	vi.stubGlobal('fetch', fetcher);
	return fetcher;
}

describe('installed connection information', () => {
	it('uses the same origin and actual non-default ports', async () => {
		const body = {
			proxy_url: 'http://127.0.0.1:7123',
			inspect_url: 'http://[::1]:7124',
			browser_url: 'http://127.0.0.1:7125'
		};
		const fetcher = respond(body);
		expect(await getConnectionInfo()).toEqual(body);
		expect(fetcher).toHaveBeenCalledWith(
			'/api/connection-info',
			expect.objectContaining({ headers: { accept: 'application/json' } })
		);
		expect(directClaudeCommand(body.proxy_url)).toBe(
			"ANTHROPIC_BASE_URL='http://127.0.0.1:7123' claude"
		);
	});

	it('keeps unavailable listener addresses absent instead of inventing defaults', async () => {
		respond({ proxy_url: null, inspect_url: null, browser_url: null });
		expect(await getConnectionInfo()).toEqual({
			proxy_url: null,
			inspect_url: null,
			browser_url: null
		});
	});

	it('reports API errors without a hardcoded port fallback', async () => {
		respond({}, 503);
		await expect(getConnectionInfo()).rejects.toThrow('503');
	});

	it.each([
		'javascript:alert(1)',
		'http://user:secret@localhost:9000',
		'http://localhost:9000/path',
		'http://localhost:9000?secret=value',
		'http://localhost:9000/#fragment',
		undefined
	])('rejects unsafe or malformed endpoint %s', async (proxy_url) => {
		respond({ proxy_url, inspect_url: null, browser_url: null });
		await expect(getConnectionInfo()).rejects.toThrow();
	});

	it('passes cancellation through to fetch', async () => {
		const fetcher = respond({ proxy_url: null, inspect_url: null, browser_url: null });
		const controller = new AbortController();
		await getConnectionInfo(controller.signal);
		expect(fetcher).toHaveBeenCalledWith(
			'/api/connection-info',
			expect.objectContaining({ signal: controller.signal })
		);
	});

	it('shell-quotes rather than evaluates an embedded quote', () => {
		expect(directClaudeCommand("http://example'host:1234")).toBe(
			"ANTHROPIC_BASE_URL='http://example'\\''host:1234' claude"
		);
	});
});
