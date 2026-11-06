// @vitest-environment jsdom

import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { tick } from 'svelte';
import { buildStatsResponse } from '$lib/test-support';

vi.mock('$lib/ws.svelte', async () => ({
	proxyEvents: (await import('$lib/test-support/ws-mock')).makeWsMock()
}));
vi.mock('$app/navigation', async () => ({
	goto: (await import('$lib/test-support/app-navigation-mock')).makeGotoMock()
}));
vi.mock('$lib/stores/shellState.svelte', async () =>
	(await import('$lib/test-support/shellState-mock')).makeShellMock()
);

import { goto } from '$app/navigation';
import StatusIndicators from './StatusIndicators.svelte';

const gotoMock = vi.mocked(goto);

beforeEach(() => {
	gotoMock.mockClear();
});

afterEach(() => {
	cleanup();
});

describe('StatusIndicators — capture navigation', () => {
	it('navigates to / when clicked', async () => {
		render(StatusIndicators, {
			props: { stats: buildStatsResponse() }
		});
		await tick();
		const chip = document.querySelector('[data-slot="capture-status"]') as HTMLButtonElement | null;
		chip!.click();
		expect(gotoMock).toHaveBeenCalledWith('/');
	});
});
