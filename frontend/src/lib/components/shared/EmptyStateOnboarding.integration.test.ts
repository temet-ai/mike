// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import EmptyStateOnboarding from './EmptyStateOnboarding.svelte';

vi.mock('$app/paths', () => ({ resolve: (path: string) => path }));

beforeEach(() => {
	Object.defineProperty(navigator, 'clipboard', {
		configurable: true,
		value: { writeText: vi.fn() }
	});
});

afterEach(() => {
	Reflect.deleteProperty(navigator, 'clipboard');
	cleanup();
	vi.unstubAllGlobals();
});

describe('installed empty-state onboarding', () => {
	it('loads the running endpoint before offering a copyable direct command', async () => {
		const fetcher = vi.fn().mockResolvedValue({
			ok: true,
			json: async () => ({
				proxy_url: 'http://127.0.0.1:7451',
				inspect_url: 'http://127.0.0.1:7452',
				browser_url: null
			})
		});
		vi.stubGlobal('fetch', fetcher);
		render(EmptyStateOnboarding);
		await screen.findByText("ANTHROPIC_BASE_URL='http://127.0.0.1:7451' claude");
		expect(screen.getByRole('link', { name: 'Getting started' }).getAttribute('href')).toBe(
			'/help/guides/getting-started'
		);
		expect(fetcher.mock.calls[0]![0]).toBe('/api/connection-info');
		expect(screen.getByRole('button', { name: 'Copy command' })).toBeTruthy();
	});

	it('shows a connection error without a false command when the API is unavailable', async () => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 503 }));
		render(EmptyStateOnboarding);
		await screen.findByText(/Connection information unavailable/);
		expect(screen.queryByRole('button', { name: 'Copy command' })).toBeNull();
		expect(document.body.textContent).not.toContain('9988');
	});

	it('does not load connection metadata when a caller explicitly suppresses the command', async () => {
		const fetcher = vi.fn();
		vi.stubGlobal('fetch', fetcher);
		render(EmptyStateOnboarding, { command: null });
		await waitFor(() => expect(screen.getByRole('link', { name: 'Getting started' })).toBeTruthy());
		expect(fetcher).not.toHaveBeenCalled();
	});

	it('aborts the metadata request on unmount', async () => {
		const fetcher = vi.fn().mockImplementation(() => new Promise(() => {}));
		vi.stubGlobal('fetch', fetcher);
		const view = render(EmptyStateOnboarding);
		await waitFor(() => expect(fetcher).toHaveBeenCalledOnce());
		const signal = fetcher.mock.calls[0]![1].signal as AbortSignal;
		view.unmount();
		expect(signal.aborted).toBe(true);
	});
});
