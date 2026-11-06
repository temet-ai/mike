<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { FleetProjectWithCounts } from '$lib/fleet/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import type { SessionSummary, KnowledgeSearchResponse } from '$lib/types';
	import type { TagsResponse, ThreadsListResponse } from '$lib/comms/types';
	import type { TimeSpentDto } from '$lib/types';
	import { timeSpentFromUrl } from '$lib/components/__fixtures__/timeSpent';
	import 'augmented-ui/augmented-ui.min.css';

	const STORY_PROJECT_PATH = '/Users/dev/temet-proxy';
	const timeSpentUrls: string[] = [];

	const KNOWLEDGE_SEARCH: KnowledgeSearchResponse = {
		retrieval_path: 'lexical',
		items: [],
		has_more: false,
		next_offset: null,
		read_tool: 'read_project_knowledge'
	};

	const PROJECTS: FleetProjectWithCounts[] = [
		{
			project_path: STORY_PROJECT_PATH,
			session_count: 42,
			request_count: 1380,
			first_seen: '2026-04-10T09:00:00Z',
			last_seen: '2026-07-16T08:30:00Z',
			pinned: true,
			custom_name: null
		},
		{
			project_path: '/Users/dev/temet-aide',
			session_count: 9,
			request_count: 210,
			first_seen: '2026-05-02T11:00:00Z',
			last_seen: '2026-07-10T14:20:00Z',
			pinned: false,
			custom_name: null
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Projects/Detail',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	loaders={[
		async () => {
			stubFetchJsonRouter((method, url, body) => {
				if (url.includes('/api/time_spent')) {
					timeSpentUrls.push(url);
					return typed<TimeSpentDto>(timeSpentFromUrl(url));
				}
				if (url.includes('/api/sessions')) return typed<SessionSummary[]>([]);
				if (url.includes('/api/tags')) return typed<TagsResponse>({ tags: [] });
				if (url.includes('/api/threads')) {
					return typed<ThreadsListResponse>({
						threads: [
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
						total_count: 1
					});
				}
				if (url.includes('/api/knowledge/search')) return typed<KnowledgeSearchResponse>(KNOWLEDGE_SEARCH);
				if (method === 'PATCH' && url.includes('/name')) {
					const sent = (body ?? {}) as { custom_name?: string | null };
					return typed<{ project_path: string; custom_name: string | null }>({
						project_path: STORY_PROJECT_PATH,
						custom_name: sent.custom_name ?? null
					});
				}
				if (url.includes('/api/projects/by-path/')) return typed<FleetProjectWithCounts>(PROJECTS[0]!);
				return typed<FleetProjectWithCounts[]>(PROJECTS);
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL(`http://localhost/projects/${encodeURIComponent(STORY_PROJECT_PATH)}`),
					params: { projectPath: STORY_PROJECT_PATH }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const path = await canvas.findByText(STORY_PROJECT_PATH, {
			selector: '[data-slot="project-detail-path"]'
		});
		await expect(path).toBeInTheDocument();

		const vitals = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="project-detail-vitals"]');
			if (!el) throw new Error('vitals card not yet rendered');
			return el as HTMLElement;
		});

		await expect(
			within(vitals).getByText('42', { selector: '[data-slot="project-detail-session-count"]' })
		).toBeInTheDocument();
		const requestCountText = vitals.querySelector(
			'[data-slot="project-detail-request-count"]'
		)?.textContent;
		await expect(requestCountText === '1,380' || requestCountText === '1380').toBe(true);
		await expect(
			within(vitals).getByText('Pinned', { selector: '[data-slot="project-detail-pinned"]' })
		).toBeInTheDocument();
		await expect(canvas.getByText(/still being designed/i)).toBeInTheDocument();
		await waitFor(() => expect(canvas.getByText('Plain thread')).toBeInTheDocument());
		await expect(canvas.getByText('Comms · 1 thread')).toBeInTheDocument();

		const timeSpent = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="time-spent-summary"]');
			if (!el || el.textContent?.includes('Loading')) throw new Error('time spent not loaded');
			return el as HTMLElement;
		});
		await expect(timeSpent.textContent).toBe('108 h model · 116 h waiting · last 14 days');
		const timeSpentUrl = new URL(timeSpentUrls[0]!, 'http://localhost');
		await expect(timeSpentUrl.searchParams.get('project_path')).toBe(STORY_PROJECT_PATH);
		await expect(timeSpentUrl.searchParams.get('project')).toBeNull();

		const stack = canvasElement.querySelector('[data-slot="project-detail-stack"]') as HTMLElement;
		await expect(stack.classList.contains('edge')).toBe(false);

		const topRow = canvasElement.querySelector(
			'[data-slot="project-detail-top-row"]'
		) as HTMLElement;
		const flush = canvasElement.querySelector('[data-slot="project-detail-flush"]') as HTMLElement;
		await expect(topRow).not.toBeNull();
		await expect(flush).not.toBeNull();

		const sessionsPanel = await waitFor(() => {
			const el = canvasElement.querySelector('[data-slot="project-sessions-panel"]');
			if (!el) throw new Error('sessions panel not yet rendered');
			return el as HTMLElement;
		});
		const embeddingPanel = canvasElement.querySelector(
			'[data-slot="project-embedding-panel"]'
		) as HTMLElement;
		const memoryPanel = canvasElement.querySelector(
			'[data-slot="project-memory-panel"]'
		) as HTMLElement;
		const launchesPanel = canvasElement.querySelector(
			'[data-slot="project-launches-panel"]'
		) as HTMLElement;
		const sessionsCard = sessionsPanel.querySelector('.dsc-card') as HTMLElement;
		const embeddingCard = embeddingPanel.querySelector('.dsc-card') as HTMLElement;
		const memoryCard = memoryPanel.querySelector('.dsc-card') as HTMLElement;
		const knowledgePanel = canvasElement.querySelector(
			'[data-slot="project-knowledge-panel"]'
		) as HTMLElement;
		const knowledgeCard = knowledgePanel.querySelector('.dsc-card') as HTMLElement;
		const members = [vitals, embeddingPanel, sessionsPanel, memoryPanel];

		await expect(topRow.contains(vitals)).toBe(true);
		await expect(topRow.contains(embeddingPanel)).toBe(true);
		await expect(topRow.contains(sessionsPanel)).toBe(false);
		await expect(topRow.contains(memoryPanel)).toBe(false);
		await expect(flush.contains(knowledgePanel)).toBe(true);
		await expect(flush.firstElementChild).toBe(knowledgePanel);
		await expect(flush.contains(sessionsPanel)).toBe(true);
		await expect(flush.contains(memoryPanel)).toBe(true);

		await expect(vitals.classList.contains('funk-sec')).toBe(true);
		await expect(vitals.classList.contains('edge')).toBe(true);
		await expect(getComputedStyle(vitals).clipPath).not.toBe('none');
		await expect(getComputedStyle(vitals.querySelector('.funk-sec-tag')!).clipPath).not.toBe(
			'none'
		);

		await expect(embeddingCard.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		await expect(getComputedStyle(embeddingCard).clipPath).not.toBe('none');
		await expect(sessionsCard.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		await expect(getComputedStyle(sessionsCard).clipPath).not.toBe('none');
		await expect(memoryCard.hasAttribute('data-augmented-ui')).toBe(false);

		const dc = (el: HTMLElement, n: string) => getComputedStyle(el).getPropertyValue(n).trim();
		await expect(dc(vitals, '--edge-tl')).not.toBe('0px');
		await expect(dc(vitals, '--edge-br')).not.toBe('0px');
		await expect(dc(embeddingCard, '--edge-tl')).not.toBe('0px');
		await expect(dc(embeddingCard, '--edge-br')).not.toBe('0px');
		await expect(dc(knowledgeCard, '--edge-tl')).not.toBe('0px');
		await expect(dc(knowledgeCard, '--edge-br')).toBe('0px');
		await expect(dc(sessionsCard, '--edge-tl')).toBe('0px');
		await expect(dc(sessionsCard, '--edge-br')).toBe('0px');
		await expect(dc(memoryCard, '--edge-tl')).toBe('0px');
		await expect(dc(memoryCard, '--edge-br')).not.toBe('0px');

		await expect(vitals.querySelector('.funk-sec-tag')?.textContent?.trim()).toBe('Vitals');
		await expect(within(embeddingCard).getByText('Session map')).toBeInTheDocument();
		await expect(within(sessionsCard).getByText('Sessions')).toBeInTheDocument();
		await expect(within(memoryCard).getByText('Project memory')).toBeInTheDocument();

		await expect(vitals.getAttribute('data-accent')).toBe('teal');
		await expect(embeddingCard.hasAttribute('data-accent')).toBe(false);
		await expect(sessionsCard.hasAttribute('data-accent')).toBe(false);
		await expect(memoryCard.hasAttribute('data-accent')).toBe(false);

		await expect((vitals.querySelector('.funk-sec-hd') as HTMLElement).tagName).toBe('DIV');
		await expect(vitals.querySelector('.funk-sec-hd .dm')).toBeNull();
		for (const card of [sessionsCard, embeddingCard, memoryCard]) {
			const toggle = card.querySelector('[data-elevation-trigger]') as HTMLElement;
			await expect(toggle.tagName).toBe('BUTTON');
			await expect(toggle.querySelector('.dm')).not.toBeNull();
		}

		const note = canvasElement.querySelector(
			'[data-slot="project-detail-placeholder-note"]'
		) as HTMLElement;
		await expect(vitals.contains(note)).toBe(true);
		await expect(note.hasAttribute('data-augmented-ui')).toBe(false);

		const detailPanelRoot = canvasElement.querySelector(
			'[data-slot="project-detail-panel"]'
		) as HTMLElement;
		const detailPanelRect = detailPanelRoot.getBoundingClientRect();
		const embeddingRect = embeddingPanel.getBoundingClientRect();
		await expect(Math.abs(detailPanelRect.top - embeddingRect.top)).toBeLessThan(4);
		await expect(embeddingRect.left).toBeGreaterThan(detailPanelRect.right);

		await expect(detailPanelRoot.querySelector('[data-slot="project-detail-heading"]')).toBeNull();
		const pageHeader = canvasElement.querySelector('header') as HTMLElement;
		await expect(pageHeader.querySelector('[data-slot="project-detail-heading"]')).not.toBeNull();
		await expect(
			within(pageHeader).getByText(STORY_PROJECT_PATH, {
				selector: '[data-slot="project-detail-path"]'
			})
		).toBeInTheDocument();

		const sessionsRect = sessionsPanel.getBoundingClientRect();
		const launchesRect = launchesPanel.getBoundingClientRect();
		const memoryRect = memoryPanel.getBoundingClientRect();
		await expect(sessionsRect.top).toBeGreaterThanOrEqual(
			Math.max(detailPanelRect.bottom, embeddingRect.bottom) - 1
		);
		// `.pdp-flush` stacks knowledge / sessions / launches / memory with no
		// gap between any adjacent pair — each later member's top overlaps the
		// one before it by exactly the shared 1px hairline.
		await expect(sessionsRect.bottom - launchesRect.top).toBeGreaterThan(0.5);
		await expect(sessionsRect.bottom - launchesRect.top).toBeLessThan(1.5);
		await expect(launchesRect.bottom - memoryRect.top).toBeGreaterThan(0.5);
		await expect(launchesRect.bottom - memoryRect.top).toBeLessThan(1.5);

		const previousRowWidth = topRow.style.width;
		topRow.style.width = '300px';
		await waitFor(() => {
			const dRect = detailPanelRoot.getBoundingClientRect();
			const eRect = embeddingPanel.getBoundingClientRect();
			if (eRect.top - dRect.bottom < 1) throw new Error('row has not wrapped yet');
		});
		topRow.style.width = previousRowWidth;
		await waitFor(() => {
			const dRect = detailPanelRoot.getBoundingClientRect();
			const eRect = embeddingPanel.getBoundingClientRect();
			if (Math.abs(dRect.top - eRect.top) >= 4) throw new Error('row has not restored yet');
		});
		await userEvent.click(
			canvas.getByRole('button', { name: /edit friendly name for this project/i })
		);
		const nameInput = canvas.getByLabelText(/friendly name for this project/i) as HTMLInputElement;
		await waitFor(() => expect(nameInput).toHaveFocus());
		await userEvent.clear(nameInput);
		await userEvent.type(nameInput, 'Scratch renamed');
		await userEvent.click(canvas.getByRole('button', { name: /save friendly name/i }));

		await waitFor(() => {
			const heading = canvasElement.querySelector('[data-slot="project-detail-custom-name"]');
			if (heading?.textContent !== 'Scratch renamed') throw new Error('heading not updated yet');
		});
		await expect(
			canvasElement.querySelector('[data-slot="project-detail-path"]')?.textContent
		).toBe(STORY_PROJECT_PATH);

		await userEvent.click(
			canvas.getByRole('button', { name: /edit friendly name for this project/i })
		);
		await userEvent.click(canvas.getByRole('button', { name: /clear friendly name/i }));
		await waitFor(() => {
			const heading = canvasElement.querySelector('[data-slot="project-detail-custom-name"]');
			if (heading?.textContent !== STORY_PROJECT_PATH) throw new Error('heading not reverted yet');
		});

		const back = canvasElement.querySelector('[data-slot="project-detail-back"]') as HTMLElement;
		await expect(back).not.toBeNull();
		await expect(back).toHaveAttribute('href', '/projects');
	}}
>
	{#snippet template()}
		<div class="h-[600px] w-[900px] overflow-hidden bg-[var(--surface-bg)]">
			<Page />
		</div>
	{/snippet}
</Story>
