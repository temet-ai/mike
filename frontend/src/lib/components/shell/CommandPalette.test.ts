// @vitest-environment jsdom

import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { tick } from 'svelte';
import { buildSessionSummary } from '$lib/components/__fixtures__/session';

const apiMock = vi.hoisted(() => ({
	listSessions: vi.fn(),
	patchSessionPin: vi.fn()
}));

const pageMock = vi.hoisted(() => ({
	page: {
		url: { pathname: '/', searchParams: new URLSearchParams() },
		params: {} as Record<string, string | undefined>,
		route: { id: '/' },
		status: 200,
		error: null,
		data: {},
		form: null,
		state: {}
	}
}));

const browserMock = vi.hoisted(() => ({ browser: true }));

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return { ...apiMock, ApiError };
});
vi.mock('$lib/stores/shellState.svelte', async () =>
	(await import('$lib/test-support/shellState-mock')).makeShellMock({ commandPaletteOpen: true })
);
vi.mock('$app/navigation', async () => ({
	goto: (await import('$lib/test-support/app-navigation-mock')).makeGotoMock()
}));
vi.mock('$app/state', () => pageMock);
vi.mock('$app/environment', () => browserMock);

import CommandPalette from './CommandPalette.svelte';
import { shellState } from '$lib/stores/shellState.svelte';
import { goto } from '$app/navigation';

const gotoMock = vi.mocked(goto);

beforeEach(() => {
	vi.useFakeTimers();
	apiMock.listSessions.mockReset();
	apiMock.listSessions.mockResolvedValue([]);
	gotoMock.mockClear();
	shellState.commandPaletteOpen = true;
	if (typeof window !== 'undefined') {
		try {
			window.localStorage.clear();
		} catch {}
	}
});

afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

function findInput(): HTMLInputElement | null {
	return document.querySelector<HTMLInputElement>('input[role="combobox"]');
}

function typeQuery(input: HTMLInputElement, value: string): void {
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
}

async function renderPalette(): Promise<HTMLInputElement> {
	render(CommandPalette, { props: {} });
	await tick();
	const input = findInput();
	expect(input).not.toBeNull();
	return input as HTMLInputElement;
}

describe('CommandPalette — session search (Phase A4)', () => {
	it('does not search for one-character or whitespace-only queries', async () => {
		const input = await renderPalette();
		for (const query of ['a', '     ']) {
			typeQuery(input, query);
			await vi.advanceTimersByTimeAsync(500);
		}
		expect(apiMock.listSessions).not.toHaveBeenCalled();
	});

	it('calls listSessions with the trimmed query and bounded limit after 200ms', async () => {
		apiMock.listSessions.mockResolvedValue([buildSessionSummary({ id: 'sess-trgm-1' })]);

		const input = await renderPalette();
		typeQuery(input, '  pro  ');
		await vi.advanceTimersByTimeAsync(100);
		expect(apiMock.listSessions).not.toHaveBeenCalled();

		await vi.advanceTimersByTimeAsync(200);
		expect(apiMock.listSessions).toHaveBeenCalledTimes(1);
		expect(apiMock.listSessions).toHaveBeenCalledWith({
			search: 'pro',
			limit: 50
		});
	});

	it('coalesces rapid typing into a single trailing-edge fetch', async () => {
		apiMock.listSessions.mockResolvedValue([]);

		const input = await renderPalette();
		typeQuery(input, 'p');
		await vi.advanceTimersByTimeAsync(50);
		typeQuery(input, 'pr');
		await vi.advanceTimersByTimeAsync(50);
		typeQuery(input, 'pro');
		await vi.advanceTimersByTimeAsync(50);
		typeQuery(input, 'proj');
		expect(apiMock.listSessions).not.toHaveBeenCalled();

		await vi.advanceTimersByTimeAsync(250);
		expect(apiMock.listSessions).toHaveBeenCalledTimes(1);
		expect(apiMock.listSessions).toHaveBeenCalledWith({
			search: 'proj',
			limit: 50
		});
		await vi.advanceTimersByTimeAsync(2_000);
		expect(apiMock.listSessions).toHaveBeenCalledTimes(1);
	});

	it('drops below-gate queries WITHOUT firing listSessions, even after a prior >=2 hit', async () => {
		apiMock.listSessions.mockResolvedValue([buildSessionSummary()]);

		const input = await renderPalette();

		typeQuery(input, 'pr');
		await vi.advanceTimersByTimeAsync(250);
		expect(apiMock.listSessions).toHaveBeenCalledTimes(1);

		typeQuery(input, 'p');
		await vi.advanceTimersByTimeAsync(500);
		expect(apiMock.listSessions).toHaveBeenCalledTimes(1);

		typeQuery(input, '');
		await vi.advanceTimersByTimeAsync(500);
		expect(apiMock.listSessions).toHaveBeenCalledTimes(1);
	});
});

describe('CommandPalette — settings', () => {
	function optionTexts(): string[] {
		return [...document.querySelectorAll('[role="option"]')].map((o) => o.textContent ?? '');
	}
	async function pressEnter(input: HTMLInputElement): Promise<void> {
		input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
		await vi.advanceTimersByTimeAsync(10);
	}

	it("lists Settings and each tab for 'settings'", async () => {
		const input = await renderPalette();
		typeQuery(input, 'settings');
		await tick();
		const texts = optionTexts().join('\n');
		for (const tab of [
			'Steering settings',
			'Workers settings',
			'Assistants settings',
			'Embedding settings',
			'Flagging settings',
			'Integrations settings',
			'History settings'
		]) {
			expect(texts).toContain(tab);
		}
		expect(texts).toContain('General settings and switches');
	});

	it("opens the settings page with the switch searched for 'Mike tools'", async () => {
		const input = await renderPalette();
		typeQuery(input, 'Mike tools');
		await tick();
		expect(optionTexts()[0]).toContain('Mike tools in every session');
		await pressEnter(input);
		expect(gotoMock).toHaveBeenCalledWith('/settings?find=Mike%20tools%20in%20every%20session');
	});

	it("opens the settings page with the switch searched for 'Live correction'", async () => {
		const input = await renderPalette();
		typeQuery(input, 'Live correction');
		await tick();
		expect(optionTexts()[0]).toContain('Live correction');
		await pressEnter(input);
		expect(gotoMock).toHaveBeenCalledWith('/settings?find=Live%20correction');
	});
});

describe('CommandPalette — settings tab targets', () => {
	const targets: Array<[string, string]> = [
		['Settings', '/settings'],
		['Steering settings', '/settings/steering'],
		['Workers settings', '/settings/workers'],
		['Assistants settings', '/settings/assistants'],
		['Embedding settings', '/settings/embedding'],
		['Flagging settings', '/settings/flagging'],
		['Integrations settings', '/settings/integrations'],
		['History settings', '/settings/history']
	];
	for (const [title, href] of targets) {
		it(`'${title}' leads to ${href}`, async () => {
			const input = await renderPalette();
			typeQuery(input, title);
			await tick();
			const first = document.querySelector('[role="option"]') as HTMLElement;
			expect(first.textContent).toContain(title);
			first.click();
			await vi.advanceTimersByTimeAsync(10);
			expect(gotoMock).toHaveBeenCalledWith(href);
		});
	}
});
