// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import {
	buildCaptureGapHealth,
	buildDegradedCaptureHealth,
	buildHealthResponse
} from '$lib/test-support/healthFixtures';

const api = vi.hoisted(() => ({ getHealth: vi.fn() }));
vi.mock('$lib/api', async () => ({
	...api,
	ApiError: (await import('$lib/test-support/apiError')).ApiError
}));
import { ApiError } from '$lib/test-support/apiError';
vi.mock('$lib/stores/shellState.svelte', async () =>
	(await import('$lib/test-support/shellState-mock')).makeShellMock({ helpDrawerOpen: true })
);
import HelpDrawer from './HelpDrawer.svelte';

beforeEach(() => {
	api.getHealth.mockReset();
});
afterEach(async () => {
	await cleanup();
	// bits-ui restores body styles on a delayed timer after the sheet unmounts.
	// Wait for that work while this test's document still exists.
	await waitFor(() => {
		expect(document.body.style.overflow).toBe('');
		expect(document.body.style.pointerEvents).toBe('');
		expect(document.body.style.getPropertyValue('--scrollbar-width')).toBe('');
	});
});

async function openDiagnostics(): Promise<HTMLElement> {
	const summary = await screen.findByText('Capture-gap diagnostics');
	summary.click();
	return summary.parentElement!;
}

describe('HelpDrawer health diagnostics', () => {
	it('shows checking while the request is pending', async () => {
		let resolveHealth!: (health: ReturnType<typeof buildHealthResponse>) => void;
		api.getHealth.mockReturnValue(
			new Promise((resolve) => {
				resolveHealth = resolve;
			})
		);
		render(HelpDrawer, { stats: null });
		expect(await screen.findByText('checking')).toBeTruthy();
		expect((await openDiagnostics()).textContent).toContain('unknown');
		resolveHealth(buildHealthResponse());
		await screen.findByText('reachable · healthy');
	});

	it('keeps a reachable degraded API distinct from a network failure and preserves gap evidence', async () => {
		const health = buildDegradedCaptureHealth();
		api.getHealth.mockResolvedValue(health);
		render(HelpDrawer, { stats: null });
		expect(await screen.findByText('reachable · degraded')).toBeTruthy();
		const detail = await openDiagnostics();
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('Capture gap detected');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('300 seconds');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('120 seconds');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('1800-second lookback');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('example-session-capture-gap');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('none observed');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('2026-10-13T12:00:00Z');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('420 seconds');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('Reading age: 3 seconds');
		expect(screen.queryByText(/unreachable/)).toBeNull();
	});

	it('does not infer a failure from idle or new sessions when the backend reports no gap', async () => {
		api.getHealth.mockResolvedValue(buildHealthResponse());
		render(HelpDrawer, { stats: { request_count: 0, active_streams: 0, uptime_seconds: 86400 } });
		expect(await screen.findByText('reachable · healthy')).toBeTruthy();
		const detail = await openDiagnostics();
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain('No capture gap detected');
		expect(detail.textContent?.replace(/\s+/g, ' ')).toContain(
			'Idle time alone does not establish a capture failure'
		);
	});

	it.each([
		buildHealthResponse({ capture_gap: buildCaptureGapHealth({ reading_age_secs: null }) }),
		{ status: 'ok' as const }
	])(
		'reports unknown rather than healthy capture when the reading is unavailable',
		async (health) => {
			api.getHealth.mockResolvedValue(health);
			render(HelpDrawer, { stats: null });
			await screen.findByText('reachable · healthy');
			expect((await openDiagnostics()).textContent?.toLowerCase()).toContain('unknown');
			expect(screen.queryByText(/No capture gap detected/)).toBeNull();
		}
	);

	it('reports a failed request separately and does not claim a capture gap', async () => {
		api.getHealth.mockRejectedValue(new TypeError('Failed to fetch'));
		render(HelpDrawer, { stats: null });
		await waitFor(() => expect(screen.getByText('unreachable (Failed to fetch)')).toBeTruthy());
		expect((await openDiagnostics()).textContent).toContain('The health request failed');
		expect(screen.queryByText(/Capture gap detected/)).toBeNull();
	});
	it('does not call an HTTP error response a network outage', async () => {
		api.getHealth.mockRejectedValue(new ApiError('Service unavailable', 503, '/api/health'));
		render(HelpDrawer, { stats: null });
		expect(
			await screen.findByText('reachable · health request failed (Service unavailable)')
		).toBeTruthy();
		expect(screen.queryByText(/unreachable/)).toBeNull();
	});
});
