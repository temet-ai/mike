<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { fn, userEvent, within, expect, waitFor } from 'storybook/test';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';
	import SessionPageHarness from './SessionPageHarness.svelte';
	import { stubTimeSpentFetch } from '$lib/components/__fixtures__/timeSpent';

	const FIXTURE_REQUEST_IDS = [900_001, 900_002];

	function stubSessionApiFetch(): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();

			const sessionMatch = url.match(/\/api\/sessions\/([^/?]+)/);
			if (sessionMatch) {
				const sid = decodeURIComponent(sessionMatch[1]!);
				return new Response(
					JSON.stringify({
						summary: {
							id: sid,
							project: null,
							first_seen: '2026-06-20T09:00:00Z',
							last_seen: '2026-06-20T09:45:00Z',
							request_count: FIXTURE_REQUEST_IDS.length,
							models: ['claude-opus-4-5'],
							last_status: 'Complete',
							title: null
						},
						requests: FIXTURE_REQUEST_IDS.map((id, i) => ({
							id,
							method: 'POST',
							path: '/v1/messages',
							model: 'claude-opus-4-5',
							status: 'Complete',
							is_streaming: true,
							message_count: i + 1,
							max_tokens: 4_096,
							created_at: `2026-06-20T09:0${i}:00Z`,
							completed_at: `2026-06-20T09:0${i}:30Z`,
							session_id: sid,
							project: null,
							human_prompt: 'fixture prompt',
							turn_type: 'user_prompt',
							response_tool_count: 0,
							response_tools_used: [],
							tool_names: []
						}))
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}

			const requestMatch = url.match(/\/api\/requests\/(\d+)$/);
			if (requestMatch) {
				const id = Number(requestMatch[1]);
				const isFinalRequest = id === FIXTURE_REQUEST_IDS[FIXTURE_REQUEST_IDS.length - 1];
				const usage = isFinalRequest
					? {
							input_tokens: 7_000,
							output_tokens: 500,
							cache_creation_input_tokens: 100,
							cache_read_input_tokens: 5_000
						}
					: {
							input_tokens: 5_000,
							output_tokens: 300,
							cache_creation_input_tokens: 100,
							cache_read_input_tokens: 4_000
						};
				return new Response(
					JSON.stringify({
						id,
						model: 'claude-opus-4-5',
						token_usage: usage,
						stop_reason: isFinalRequest ? 'end_turn' : null
					}),
					{ status: 200, headers: { 'content-type': 'application/json' } }
				);
			}

			return new Response('Not Found', { status: 404 });
		}) as typeof fetch;
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Routes/Sessions/Page',
		component: SessionPageHarness,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Full sessions list page (/sessions). Renders all captured sessions grouped by project, ordered by most-recently-active project, with a hover/focus-revealed per-row detail band (prompt + model/status/token/cost/stop).'
				}
			}
		}
	});
</script>

<Story
	name="Populated"
	tags={['data-driven', 'interactive']}
	args={{ scenario: 'populated', onToggle: fn() }}
	loaders={[
		async () => {
			__resetApiCacheForTests();
			const restore = stubSessionApiFetch();
			stubTimeSpentFetch();
			return { restore };
		}
	]}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByRole('heading', { name: 'Sessions' })).toBeInTheDocument();
		await waitFor(() =>
			expect(canvas.getByText('108 h model · 116 h waiting · last 14 days')).toBeInTheDocument()
		);

		function queryGroupBtn(): HTMLElement | null {
			return canvasElement.querySelector('.ssn-project-head') as HTMLElement | null;
		}
		function queryToggleBtn(): HTMLElement | null {
			return canvasElement.querySelector('.ssn-project-toggle') as HTMLElement | null;
		}
		await expect(queryGroupBtn(), 'project group header should be present').toBeTruthy();
		await expect(queryToggleBtn(), 'project group toggle button should be present').toBeTruthy();

		const rowsBefore = canvasElement.querySelectorAll('.ssn-row-wrap');
		await expect(rowsBefore.length).toBeGreaterThan(0);

		await userEvent.click(queryToggleBtn()!);
		await waitFor(() => expect(queryToggleBtn()).toHaveAttribute('aria-expanded', 'false'));
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBeLessThan(rowsBefore.length)
		);
		await expect(args.onToggle).toHaveBeenCalledTimes(1);

		await userEvent.click(queryToggleBtn()!);
		await waitFor(() => expect(queryToggleBtn()).toHaveAttribute('aria-expanded', 'true'));
		await expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBeGreaterThan(0);

		const firstRowWrap = canvasElement.querySelector('.ssn-row-wrap') as HTMLElement;
		await userEvent.hover(firstRowWrap);
		await waitFor(
			() => {
				const inValue = firstRowWrap.querySelector('.ssn-detail-meta')?.textContent ?? '';
				if (!inValue.includes('12,000')) throw new Error('IN tokens not yet resolved');
			},
			{ timeout: 3_000 }
		);
		await expect(firstRowWrap.textContent).toContain('claude-opus-4-5');

		const copyBtn = firstRowWrap.querySelector(
			'button[aria-label="Copy full session id"]'
		) as HTMLButtonElement | null;
		await expect(copyBtn, 'CopyIconButton should render for the row').toBeTruthy();

		const pinBtn = firstRowWrap.querySelector(
			'button[aria-label="Pin session"]'
		) as HTMLButtonElement | null;
		if (pinBtn) {
			await userEvent.click(pinBtn);
			await waitFor(() => expect(pinBtn).toHaveAttribute('aria-pressed', 'true'));
		}

		const showMoreBtn = [...canvasElement.querySelectorAll('button.ssn-showmore')][0] as
			HTMLButtonElement | undefined;
		await expect(
			showMoreBtn,
			'a >10-session project should render a show-more footer'
		).toBeTruthy();
		if (!showMoreBtn) throw new Error('No .ssn-showmore button found');

		const bigProjectTable = showMoreBtn.closest('.ssn-table') as HTMLElement;
		const rowsBeforeExpand = bigProjectTable.querySelectorAll('.ssn-row-wrap').length;
		await expect(rowsBeforeExpand).toBe(10);
		await expect(showMoreBtn.textContent).toContain('Show 2 more sessions');

		await userEvent.click(showMoreBtn);
		await waitFor(() => expect(showMoreBtn.textContent).toContain('Show fewer'));
		await waitFor(() =>
			expect(bigProjectTable.querySelectorAll('.ssn-row-wrap').length).toBe(rowsBeforeExpand + 2)
		);

		await userEvent.click(showMoreBtn);
		await waitFor(() => expect(showMoreBtn.textContent).toContain('Show 2 more sessions'));
		await waitFor(() =>
			expect(bigProjectTable.querySelectorAll('.ssn-row-wrap').length).toBe(rowsBeforeExpand)
		);

		const loadOlderBtn = canvasElement.querySelector('.ssn-load-older') as HTMLButtonElement | null;
		await expect(
			loadOlderBtn,
			'a fully-shown, non-exhausted project should offer "Load older sessions"'
		).toBeTruthy();
		if (!loadOlderBtn) throw new Error('No .ssn-load-older button found');

		const loadOlderTable = loadOlderBtn.closest('.ssn-table') as HTMLElement;
		const rowCountBeforeLoadOlder = loadOlderTable.querySelectorAll('.ssn-row-wrap').length;

		await userEvent.click(loadOlderBtn);
		await waitFor(() =>
			expect(loadOlderTable.querySelectorAll('.ssn-row-wrap').length).toBe(
				rowCountBeforeLoadOlder + 1
			)
		);
		await waitFor(() => expect(loadOlderTable.querySelector('.ssn-load-older')).toBeNull());

		const searchInput = canvas.getByPlaceholderText(
			'Search sessions or content'
		) as HTMLInputElement;
		await userEvent.type(searchInput, 'token');

		await waitFor(() => {
			const cards = canvasElement.querySelectorAll('.ssn-row-wrap');
			if (cards.length !== 2)
				throw new Error(`expected 2 matching session cards, got ${cards.length}`);
		});

		const projectHeads = [...canvasElement.querySelectorAll('.ssn-project-head')];
		await expect(projectHeads.length).toBe(1);
		await expect(projectHeads[0]!.textContent).toContain('alpha');
		await expect(projectHeads[0]!.textContent).toContain('2 matches');

		const cards = [...canvasElement.querySelectorAll('.ssn-row-wrap')] as HTMLElement[];
		const firstCard = cards[0]!;
		const secondCard = cards[1]!;
		await expect(firstCard.textContent).toContain('Implement auth middleware');
		await expect(secondCard.textContent).toContain('Fix rate limit headers');

		const firstChips = [...firstCard.querySelectorAll('.ssn-match-chip')].map(
			(el) => el.textContent
		);
		await expect(firstChips).toEqual(['5 conversation', '3 raw body']);
		const secondChips = [...secondCard.querySelectorAll('.ssn-match-chip')].map(
			(el) => el.textContent
		);
		await expect(secondChips).toEqual(['1 conversation', '1 raw body']);

		await expect(firstCard.querySelectorAll('.sbm-row').length).toBe(0);
		const firstToggle = firstCard.querySelector('.ssn-match-toggle') as HTMLButtonElement;
		await expect(firstToggle).toHaveAttribute('aria-expanded', 'false');

		await userEvent.click(firstToggle);
		await waitFor(() => expect(firstToggle).toHaveAttribute('aria-expanded', 'true'));
		await expect(firstCard.querySelectorAll('.sbm-row').length).toBe(5);
		await expect(firstCard.textContent?.replace(/\s+/g, ' ')).toContain(
			'Showing 2 of 5 conversation matches'
		);

		const firstBadges = [...firstCard.querySelectorAll('.sbm-badge')].map((el) => el.textContent);
		await expect(firstBadges).toContain('prompt');
		await expect(firstBadges).toContain('assistant');
		await expect(firstBadges).toContain('response');
		const firstRequestLinks = [
			...firstCard.querySelectorAll('.sbm-link--primary')
		] as HTMLAnchorElement[];
		await expect(firstRequestLinks.length).toBe(5);
		await expect(
			firstRequestLinks.every((a) => a.getAttribute('href')?.includes('/requests/'))
		).toBe(true);

		await expect(secondCard.querySelectorAll('.sbm-row').length).toBe(0);
		const secondToggle = secondCard.querySelector('.ssn-match-toggle') as HTMLButtonElement;
		await userEvent.click(secondToggle);
		await waitFor(() => expect(secondToggle).toHaveAttribute('aria-expanded', 'true'));
		await expect(secondCard.querySelectorAll('.sbm-row').length).toBe(2);
		await expect(secondCard.textContent).not.toContain('refine the search to narrow');

		const scopeDl = canvasElement.querySelector(
			'dl[aria-label="Search result scope"]'
		) as HTMLElement;
		await expect(scopeDl, 'search mode should show a match-scope summary').toBeTruthy();
		await expect(scopeDl.textContent).toContain('Sessions matched');
		await expect(scopeDl.textContent).toContain('Projects matched');
		const scopeCounts = [...scopeDl.querySelectorAll('dd')].map((el) => el.textContent);
		await expect(scopeCounts).toEqual(['2', '1']);

		await userEvent.clear(searchInput);
		await userEvent.type(searchInput, 'zzzznomatch');
		await waitFor(() => {
			const state = canvasElement.querySelector('[data-slot="filtered-empty-state-text"]');
			if (!state?.textContent?.includes('No sessions or content match')) {
				throw new Error('expected the no-match message');
			}
		});
		const noMatchScopeDl = canvasElement.querySelector(
			'dl[aria-label="Search result scope"]'
		) as HTMLElement;
		const noMatchCounts = [...noMatchScopeDl.querySelectorAll('dd')].map((el) => el.textContent);
		await expect(noMatchCounts).toEqual(['0', '0']);

		const clearSearchBtn = canvasElement.querySelector(
			'[data-slot="filtered-empty-state-clear"]'
		) as HTMLButtonElement | null;
		await expect(
			clearSearchBtn,
			'the no-match state should offer a Clear search reset affordance'
		).toBeTruthy();
		if (!clearSearchBtn) throw new Error('No Clear search button found');

		await userEvent.click(clearSearchBtn);
		await waitFor(() => expect(searchInput.value).toBe(''));
		await waitFor(() => {
			const overviewDl = canvasElement.querySelector('dl[aria-label="Loaded session overview"]');
			if (!overviewDl) throw new Error('expected browse-mode overview to return');
		});
		await waitFor(() =>
			expect(canvasElement.querySelectorAll('.ssn-row-wrap').length).toBeGreaterThan(0)
		);
	}}
>
	{#snippet template(args)}
		<SessionPageHarness {...args} />
	{/snippet}
</Story>
