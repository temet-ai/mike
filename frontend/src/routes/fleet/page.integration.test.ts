// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { tick } from 'svelte';
import type { FleetGraphResponse, FleetProjectSummary } from '$lib/fleet/types';

const api = vi.hoisted(() => ({ projects: vi.fn(), graph: vi.fn() }));
const navigation = vi.hoisted(() => ({ page: { url: new URL('http://localhost/fleet') } }));
vi.mock('$app/state', () => ({ page: navigation.page }));
vi.mock('$app/navigation', () => ({ goto: vi.fn().mockResolvedValue(undefined) }));
vi.mock('$app/paths', () => ({
	resolve: (path: string, params?: Record<string, string>) =>
		params ? path.replace(/\[([^\]]+)\]/g, (_, key: string) => params[key] ?? '') : path
}));
vi.mock('$lib/ws.svelte', async () => ({
	MAX_EVENTS: 1024,
	proxyEvents: {
		...(await import('$lib/test-support/ws-mock')).makeWsMock(),
		totalEvents: 0,
		at: () => undefined
	}
}));
vi.mock('$lib/fleet/api', async (original) => ({
	...(await original<typeof import('$lib/fleet/api')>()),
	getFleetProjects: api.projects,
	getFleetGraph: api.graph
}));

import Page from './+page.svelte';

function project(project_path: string): FleetProjectSummary {
	return { project_path, first_seen: '', last_seen: '', pinned: false, custom_name: null };
}

function graph(project_path: string): FleetGraphResponse {
	return { project_path, generated_at: '', nodes: [], edges: [] };
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason: Error) => void;
	const promise = new Promise<T>((yes, no) => {
		resolve = yes;
		reject = no;
	});
	return { promise, resolve, reject };
}

beforeEach(() => {
	navigation.page.url = new URL('http://localhost/fleet');
	api.projects.mockReset().mockResolvedValue([]);
	api.graph.mockReset().mockImplementation(async (path: string) => graph(path));
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 503 }));
});

afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

describe('fleet installed-project onboarding', () => {
	it('waits for the project list before choosing a graph', async () => {
		const pending = deferred<FleetProjectSummary[]>();
		api.projects.mockReturnValue(pending.promise);
		render(Page);
		await screen.findByText('Loading projects');
		expect(api.graph).not.toHaveBeenCalled();
		pending.resolve([project('/user/project')]);
		await waitFor(() => expect(api.graph).toHaveBeenCalledWith('/user/project', 24));
	});

	it('shows the empty-installation state without fetching a maintainer project', async () => {
		render(Page);
		await screen.findByText('No projects captured yet');
		expect(api.graph).not.toHaveBeenCalled();
		expect(document.querySelector('[data-slot="fleet-project-open"]')).toBeNull();
	});

	it('selects a real project deterministically when several are returned', async () => {
		api.projects.mockResolvedValue([project('/user/zebra'), project('/user/alpha')]);
		render(Page);
		await waitFor(() => expect(api.graph).toHaveBeenCalledWith('/user/alpha', 24));
	});

	it('honors an explicit deep-link project while the project list is still loading', async () => {
		navigation.page.url = new URL('http://localhost/fleet?project=%2Fuser%2Fchosen');
		const pending = deferred<FleetProjectSummary[]>();
		api.projects.mockReturnValue(pending.promise);
		render(Page);
		await waitFor(() => expect(api.graph).toHaveBeenCalledWith('/user/chosen', 24));
		pending.resolve([project('/user/first')]);
		await tick();
		expect(api.graph).not.toHaveBeenCalledWith('/user/first', 24);
	});

	it('keeps a project-list error distinct from an empty installation and offers retry', async () => {
		api.projects.mockRejectedValueOnce(new Error('Database unavailable'));
		render(Page);
		await screen.findByText("Couldn't load projects");
		expect(screen.queryByText('No projects captured yet')).toBeNull();
		await fireEvent.click(screen.getByRole('button', { name: /retry/i }));
		await screen.findByText('No projects captured yet');
	});

	it('does not apply a stale error from the previous project over the new selection', async () => {
		const first = deferred<FleetGraphResponse>();
		api.projects.mockResolvedValue([project('/user/alpha'), project('/user/zebra')]);
		api.graph.mockImplementation((path: string) =>
			path === '/user/alpha' ? first.promise : Promise.resolve(graph(path))
		);
		render(Page);
		await waitFor(() => expect(api.graph).toHaveBeenCalledWith('/user/alpha', 24));
		const select = document.querySelector(
			'[data-slot="fleet-project-select"]'
		) as HTMLSelectElement;
		await fireEvent.change(select, { target: { value: '/user/zebra' } });
		await screen.findByText(/No roster activity.*\/user\/zebra/);
		first.reject(new Error('Stale project error'));
		await tick();
		expect(screen.queryByText('Stale project error')).toBeNull();
		expect(select.value).toBe('/user/zebra');
	});
});
