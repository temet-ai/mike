<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import type { SessionSummary } from '$lib/types';
	import type { TagsResponse, ThreadsListResponse } from '$lib/comms/types';
	import 'augmented-ui/augmented-ui.min.css';

	const PROJECTS: FleetProjectWithCounts[] = [
		{
			project_path: 'standalone/watchtower',
			session_count: 5,
			request_count: 120,
			first_seen: '2026-07-01T08:00:00Z',
			last_seen: '2026-07-16T09:00:00Z',
			pinned: true,
			custom_name: null
		},
		{
			project_path: 'mobile/storefront/ios-app',
			session_count: 12,
			request_count: 340,
			first_seen: '2026-06-01T08:00:00Z',
			last_seen: '2026-07-14T10:00:00Z',
			pinned: true,
			custom_name: null
		},
		{
			project_path: 'mobile/storefront/android-app',
			session_count: 8,
			request_count: 210,
			first_seen: '2026-06-05T08:00:00Z',
			last_seen: '2026-07-12T09:30:00Z',
			pinned: true,
			custom_name: null
		},
		{
			project_path: 'ai/tools/proxy\n\\',
			session_count: 7,
			request_count: 33,
			first_seen: '2026-07-09T10:00:00Z',
			last_seen: '2026-07-16T06:00:00Z',
			pinned: false,
			custom_name: 'AI tools scratch'
		},
		{
			project_path: 'projects/temet/temet-proxy',
			session_count: 42,
			request_count: 1380,
			first_seen: '2026-04-10T09:00:00Z',
			last_seen: '2026-07-16T08:30:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'projects/temet/temet-aide',
			session_count: 9,
			request_count: 210,
			first_seen: '2026-05-02T11:00:00Z',
			last_seen: '2026-07-10T14:20:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'work/ankh-morpork/night-watch',
			session_count: 65,
			request_count: 3200,
			first_seen: '2026-06-11T08:00:00Z',
			last_seen: '2026-07-15T08:00:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'repositories/am-supplier-product/supplier-product-catalog',
			session_count: 1,
			request_count: 44,
			first_seen: '2026-07-03T09:00:00Z',
			last_seen: '2026-07-13T14:00:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'repositories/am-ledger-clerk/ledger-clerk-ui',
			session_count: 3,
			request_count: 40,
			first_seen: '2026-06-30T10:00:00Z',
			last_seen: '2026-07-02T13:00:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'repositories',
			session_count: 1,
			request_count: 2,
			first_seen: '2026-07-14T09:00:00Z',
			last_seen: '2026-07-14T09:05:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: '(no project)',
			session_count: 225,
			request_count: 791,
			first_seen: '2026-06-09T06:43:00Z',
			last_seen: '2026-07-16T07:46:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'd309b653-5065-466a-bf4b-9f82bff2fd49/scratchpad/spawn-test',
			session_count: 1,
			request_count: 5,
			first_seen: '2026-07-10T11:24:00Z',
			last_seen: '2026-07-10T11:24:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: '2hhvhg9s7fg5f80flxq139d80000gn/T/tmp.qXWuJ926d2',
			session_count: 2,
			request_count: 10,
			first_seen: '2026-07-12T15:55:00Z',
			last_seen: '2026-07-12T15:55:00Z',
			pinned: false,
			custom_name: null
		},
		{
			project_path: 'projects/temet/temet-archived',
			session_count: 0,
			request_count: 0,
			first_seen: '2026-03-01T09:00:00Z',
			last_seen: '2026-03-01T09:00:00Z',
			pinned: false,
			custom_name: null
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Projects/Index',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	loaders={[
		async () => {
			let liveProjects = PROJECTS.map((p) => ({ ...p }));
			stubFetchJsonRouter((method, url, body) => {
				if (method === 'PATCH') {
					const match = /\/api\/projects\/([^/]+)\/name$/.exec(url);
					const projectPath = match?.[1] ? decodeURIComponent(match[1]) : '';
					const customName = (body as { custom_name: string | null }).custom_name;
					liveProjects = liveProjects.map((p) =>
						p.project_path === projectPath ? { ...p, custom_name: customName } : p
					);
					return typed<{ project_path: string; custom_name: string | null }>({
						project_path: projectPath,
						custom_name: customName
					});
				}
				if (url.includes('/api/sessions')) return typed<SessionSummary[]>([]);
				if (url.includes('/api/tags')) return typed<TagsResponse>({ tags: [] });
				if (url.includes('/api/threads')) {
					return typed<ThreadsListResponse>({
						threads: [
							{
								thread_uuid: 'fleet-thread-1',
								subject: 'Fleet core-game: build the whole game',
								created_at: '2026-07-10T10:00:00Z',
								last_message_at: '2026-07-10T11:00:00Z',
								resolved_at: null,
								created_by_session_uuid: null,
								participants: [],
								tags: [],
								message_count: 1,
								unread_count: 0,
								worst_delivery_state: 'delivered',
								has_stale_pending: false,
								project: null,
								fleet: { fleet_uuid: 'fleet-thread-1', slug: 'core-game', state: 'running' }
							},
							{
								thread_uuid: 'plain-thread-1',
								subject: 'Plain thread',
								created_at: '2026-07-10T10:00:00Z',
								last_message_at: '2026-07-10T11:00:00Z',
								resolved_at: null,
								created_by_session_uuid: null,
								participants: [],
								tags: [],
								message_count: 1,
								unread_count: 0,
								worst_delivery_state: 'delivered',
								has_stale_pending: false,
								project: null,
								fleet: null
							}
						],
						total_count: 2
					});
				}
				return typed<FleetProjectWithCounts[]>(liveProjects);
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/projects'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const pinnedSection = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="projects-pinned"]');
			if (!el) throw new Error('pinned section not yet rendered');
			return el as HTMLElement;
		});
		await expect(within(pinnedSection).getByText('standalone/watchtower')).toBeInTheDocument();
		expect(within(pinnedSection).queryByText(/sessions?\b/)).toBeNull();
		expect(within(pinnedSection).queryByText(/requests?\b/)).toBeNull();
		const treePanel = canvasElement.querySelector('[data-slot="projects-tree"]') as HTMLElement;
		await expect(within(treePanel).queryByText('watchtower')).toBeNull();

		const clusterHeader = within(pinnedSection).getByText('mobile/storefront');
		const iosRow = canvasElement.querySelector(
			'[data-project-path="mobile/storefront/ios-app"]'
		) as HTMLElement;
		const androidRow = canvasElement.querySelector(
			'[data-project-path="mobile/storefront/android-app"]'
		) as HTMLElement;
		await expect(iosRow).toBeInTheDocument();
		await expect(androidRow).toBeInTheDocument();
		await expect(iosRow.querySelector('[data-slot="project-row-path"]')?.textContent).toBe(
			'ios-app'
		);
		await expect(androidRow.querySelector('[data-slot="project-row-path"]')?.textContent).toBe(
			'android-app'
		);
		await expect(iosRow).toHaveAttribute('title', 'mobile/storefront/ios-app');
		expect(clusterHeader).toHaveAttribute('aria-hidden', 'true');
		await expect(iosRow).toHaveAttribute('aria-label', 'mobile/storefront/ios-app');
		await expect(androidRow).toHaveAttribute('aria-label', 'mobile/storefront/android-app');
		expect(within(pinnedSection).queryByText('standalone')).toBeNull();
		const watchtowerRow = canvasElement.querySelector(
			'[data-project-path="standalone/watchtower"]'
		) as HTMLElement;
		await expect(watchtowerRow.querySelector('[data-slot="project-row-path"]')?.textContent).toBe(
			'standalone/watchtower'
		);

		await expect(within(treePanel).queryByText(/storefront/)).toBeNull();

		const pinnedList = canvasElement.querySelector('[data-slot="projects-pinned"]') as HTMLElement;
		const listPanel = canvasElement.querySelector(
			'[data-slot="projects-directory"]'
		) as HTMLElement;
		const unknownPanel = canvasElement.querySelector(
			'[data-slot="projects-unknown"]'
		) as HTMLElement;
		expect(pinnedList).not.toBeNull();
		expect(listPanel).not.toBeNull();
		expect(unknownPanel).not.toBeNull();

		expect(listPanel.contains(pinnedList)).toBe(true);
		expect(pinnedList.classList.contains('edge')).toBe(false);

		expect(canvasElement.querySelector('.funk-sec-tag')).toBeNull();

		expect(canvasElement.querySelector('[data-slot="page-intro"]')).toBeNull();
		expect(canvasElement.querySelector('[data-slot="projects-toolbar"]')).not.toBeNull();

		const panels = canvasElement.querySelector('[data-slot="projects-panels"]') as HTMLElement;
		expect(panels.classList.contains('edge')).toBe(false);
		for (const el of [listPanel, unknownPanel]) {
			expect(el.classList.contains('edge')).toBe(true);
			expect(getComputedStyle(el).clipPath).not.toBe('none');
		}
		const listRect = listPanel.getBoundingClientRect();
		const unknownRect = unknownPanel.getBoundingClientRect();
		expect(listRect.bottom - unknownRect.top).toBeGreaterThan(0.5);
		expect(listRect.bottom - unknownRect.top).toBeLessThan(1.5);

		const corner = (el: HTMLElement, name: string) =>
			getComputedStyle(el).getPropertyValue(name).trim();
		expect(corner(listPanel, '--edge-tl')).not.toBe('0px');
		expect(corner(listPanel, '--edge-br')).toBe('0px');
		expect(corner(unknownPanel, '--edge-tl')).toBe('0px');
		expect(corner(unknownPanel, '--edge-br')).not.toBe('0px');

		expect(listPanel.getAttribute('data-accent')).toBe('teal');
		expect(unknownPanel.getAttribute('data-accent')).toBe('muted');

		expect(
			canvasElement.querySelector('[data-project-path="projects/temet/temet-archived"]')
		).toBeNull();
		const hiddenNote = canvasElement.querySelector(
			'[data-slot="projects-empty-hidden-note"]'
		) as HTMLElement;
		await expect(hiddenNote).toBeInTheDocument();
		await expect(hiddenNote.textContent?.replace(/\s+/g, ' ').trim()).toBe(
			'1 empty project hidden (no captured sessions or requests)'
		);

		const temetFolder = (await canvas.findByRole('button', {
			name: /projects\/temet/
		})) as HTMLElement;
		await expect(within(temetFolder).getByText(/51 sessions/)).toBeInTheDocument();
		await expect(within(temetFolder).getByText(/1,590 requests/)).toBeInTheDocument();

		const proxyText = await canvas.findByText('temet-proxy');
		const proxyRow = proxyText.closest('[data-slot="project-row"]') as HTMLElement;
		await expect(proxyRow).toHaveAttribute(
			'href',
			`/projects/${encodeURIComponent('projects/temet/temet-proxy')}`
		);
		await expect(within(proxyRow).queryByText('Pinned')).toBeNull();
		expect(within(proxyRow).queryByText(/sessions?\b/)).toBeNull();
		expect(within(proxyRow).queryByText(/requests?\b/)).toBeNull();

		const aideRow = (await canvas.findByText('temet-aide')).closest(
			'[data-slot="project-row"]'
		) as HTMLElement;
		await expect(within(aideRow).queryByText('Pinned')).toBeNull();

		await expect(canvasElement.querySelector('[data-slot="projects-detail-pane"]')).toBeNull();
		await expect(proxyRow.tagName).toBe('A');
		await expect(proxyRow).toHaveAttribute('aria-expanded', 'false');
		await expect(proxyRow.querySelector('.dm')).toBeNull();

		await userEvent.click(proxyRow);
		await waitFor(() => expect(proxyRow).toHaveAttribute('aria-expanded', 'true'));
		const expanded = canvasElement.querySelector(
			'[data-slot="project-row-expanded"]'
		) as HTMLElement;
		await expect(expanded).not.toBeNull();
		await expect(proxyRow.closest('.pt-row')?.contains(expanded)).toBe(true);
		await expect(expanded.getAttribute('data-project-path')).toBe('projects/temet/temet-proxy');
		await waitFor(() =>
			expect(expanded.querySelectorAll('[data-slot="project-comms"] .thread-row').length).toBe(2)
		);
		const commsSection = expanded.querySelector('[data-slot="project-comms"]') as HTMLElement;
		await expect(within(commsSection).getByText('Comms · 2 threads')).toBeInTheDocument();
		await expect(commsSection.querySelector('.thread-row__subject')?.textContent).toContain(
			'core-game'
		);
		await expect(within(commsSection).getByRole('link', { name: 'Open in Comms' })).toHaveAttribute(
			'href',
			`/comms?project=${encodeURIComponent('projects/temet/temet-proxy')}`
		);
		await waitFor(() =>
			expect(expanded.querySelector('[data-slot="project-detail-vitals"]')).not.toBeNull()
		);
		const expandedVitals = expanded.querySelector(
			'[data-slot="project-detail-vitals"]'
		) as HTMLElement;
		const expandedEmbedding = expanded.querySelector(
			'[data-slot="project-embedding-panel"]'
		) as HTMLElement;
		const expandedSessions = expanded.querySelector(
			'[data-slot="project-sessions-panel"]'
		) as HTMLElement;
		await expect(expandedEmbedding).not.toBeNull();
		await expect(expandedSessions).not.toBeNull();
		for (const el of [expandedVitals, expandedEmbedding, expandedSessions]) {
			await expect(el.classList.contains('edge')).toBe(false);
		}
		const expandedRow = expanded.querySelector(
			'[data-slot="project-row-expanded-top"]'
		) as HTMLElement;
		await expect(expandedRow).not.toBeNull();
		await expect(expandedRow.contains(expandedVitals)).toBe(true);
		await expect(expandedRow.contains(expandedEmbedding)).toBe(true);
		await expect(expandedRow.contains(expandedSessions)).toBe(false);
		const vitalsRect = expandedVitals.getBoundingClientRect();
		const embeddingRect = expandedEmbedding.getBoundingClientRect();
		const sessionsRect = expandedSessions.getBoundingClientRect();
		await expect(Math.abs(vitalsRect.top - embeddingRect.top)).toBeLessThan(4);
		await expect(embeddingRect.left).toBeGreaterThanOrEqual(vitalsRect.right - 1);
		await expect(sessionsRect.top).toBeGreaterThanOrEqual(
			Math.max(vitalsRect.bottom, embeddingRect.bottom) - 1
		);
		const previousRowWidth = expandedRow.style.width;
		expandedRow.style.width = '250px';
		await waitFor(() => {
			const vRect = expandedVitals.getBoundingClientRect();
			const eRect = expandedEmbedding.getBoundingClientRect();
			if (eRect.top - vRect.bottom < 1) throw new Error('row has not wrapped yet');
		});
		expandedRow.style.width = previousRowWidth;
		await waitFor(() => {
			const vRect = expandedVitals.getBoundingClientRect();
			const eRect = expandedEmbedding.getBoundingClientRect();
			if (Math.abs(vRect.top - eRect.top) >= 4) throw new Error('row has not restored yet');
		});
		await expect(expanded.querySelector('[data-slot="project-detail-heading"]')).toBeNull();

		await expect(canvasElement.querySelectorAll('[data-slot="project-row-expanded"]').length).toBe(
			1
		);
		await userEvent.click(proxyRow);
		await waitFor(() => expect(proxyRow).toHaveAttribute('aria-expanded', 'false'));
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="project-row-expanded"]')).toBeNull()
		);

		await userEvent.click(temetFolder);
		await waitFor(() => expect(temetFolder.getAttribute('aria-expanded')).toBe('false'));
		expect(canvas.queryByText('temet-proxy')).toBeNull();

		await userEvent.click(temetFolder);
		await waitFor(() => expect(temetFolder.getAttribute('aria-expanded')).toBe('true'));
		await expect(canvas.findByText('temet-proxy')).resolves.toBeInTheDocument();

		const nightWatchRow = (await canvas.findByText('work/ankh-morpork/night-watch')).closest(
			'[data-slot="project-row"]'
		) as HTMLElement;
		await expect(nightWatchRow).toHaveAttribute(
			'href',
			`/projects/${encodeURIComponent('work/ankh-morpork/night-watch')}`
		);
		expect(within(nightWatchRow).queryByText(/sessions?\b/)).toBeNull();
		expect(within(nightWatchRow).queryByText(/requests?\b/)).toBeNull();
		expect(canvas.queryByRole('button', { name: /folder work\/ankh-morpork/ })).toBeNull();

		const repositoriesFolder = (await canvas.findByRole('button', {
			name: /^Collapse folder repositories$/
		})) as HTMLElement;
		await expect(within(repositoriesFolder).getByText(/4 sessions/)).toBeInTheDocument();
		await expect(within(repositoriesFolder).getByText(/84 requests/)).toBeInTheDocument();
		await expect(
			canvas.getByText('am-supplier-product/supplier-product-catalog')
		).toBeInTheDocument();
		await expect(canvas.getByText('am-ledger-clerk/ledger-clerk-ui')).toBeInTheDocument();
		expect(canvas.queryByRole('button', { name: /folder am-supplier-product/i })).toBeNull();
		expect(canvas.queryByRole('button', { name: /folder am-ledger-clerk/i })).toBeNull();

		await expect(canvas.getByText(/no resolvable real project path/i)).toBeInTheDocument();
		await expect(canvas.getAllByText('Unknown')).toHaveLength(3);
		await expect(canvas.getByText('(no project)')).toBeInTheDocument();
		await expect(
			canvas.getByText('d309b653-5065-466a-bf4b-9f82bff2fd49/scratchpad/spawn-test')
		).toBeInTheDocument();
		await expect(
			canvas.getByText('2hhvhg9s7fg5f80flxq139d80000gn/T/tmp.qXWuJ926d2')
		).toBeInTheDocument();

		const noProjectRow = canvas
			.getByText('(no project)')
			.closest('[data-slot="project-row"]') as HTMLElement;
		await expect(noProjectRow).toHaveAttribute(
			'href',
			`/projects/${encodeURIComponent('(no project)')}`
		);

		const messyPath = 'ai/tools/proxy\n\\';
		const projectRows = Array.from(
			canvasElement.querySelectorAll('[data-slot="project-row"]')
		) as HTMLElement[];
		const messyRow = projectRows.find(
			(el) => el.getAttribute('data-project-path') === messyPath
		) as HTMLElement;
		await expect(messyRow).toBeTruthy();
		await expect(messyRow).toHaveAttribute('href', `/projects/${encodeURIComponent(messyPath)}`);

		const messyNameSpan = messyRow.querySelector('[data-slot="project-row-path"]');
		await expect(messyNameSpan?.textContent).toBe('AI tools scratch');
		const messyPathSpan = messyRow.querySelector('[data-slot="project-row-fullpath"]');
		await expect(messyPathSpan?.textContent).toBe(messyPath);
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
