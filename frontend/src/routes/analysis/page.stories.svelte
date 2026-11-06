<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import { __setRetryDelaysForTesting, __resetRetryDelaysForTesting } from '$lib/api';
	import type { DiagnosticStats, Finding, StatsResponse } from '$lib/types';
	import { buildStatsResponse } from '$lib/test-support';

	function buildDiagnosticStats(overrides: Partial<DiagnosticStats> = {}): DiagnosticStats {
		return {
			totals_by_category: {},
			totals_by_analyzer: {},
			last_24h: 0,
			...overrides
		};
	}

	function buildFinding(overrides: {
		request_id: number;
		session_id: string;
		category: Finding['category'];
		analyzer: string;
		advice: string;
		matched_text: string;
		other_hits?: number;
	}): Finding {
		return {
			request_id: overrides.request_id,
			session_id: overrides.session_id,
			category: overrides.category,
			analyzer: overrides.analyzer,
			advice: overrides.advice,
			matched_text: overrides.matched_text,
			match_range: [0, overrides.matched_text.length],
			other_hits: overrides.other_hits ?? 0,
			created_at: '2026-05-04T10:00:00Z',
			request: {
				model: 'claude-opus-4-7',
				status: 'Complete',
				path: '/v1/messages',
				project: '/Users/dev/project',
				human_prompt:
					'Write a comprehensive test suite for the authentication module including unit tests for the JWT validation logic and integration tests for the OAuth2 flow.',
				created_at: '2026-05-04T09:58:00Z'
			}
		};
	}

	const POPULATED_STATS: DiagnosticStats = buildDiagnosticStats({
		totals_by_category: {
			avoiding_work: 4,
			deflecting_blame: 2,
			justifying_shortcuts: 3,
			faking_understanding: 1,
			overselling: 2
		},
		totals_by_analyzer: {
			overselling_analyzer: 5,
			deflecting_blame_analyzer: 4,
			justifying_shortcuts_analyzer: 3
		},
		last_24h: 8
	});

	const POPULATED_FINDINGS: Finding[] = [
		buildFinding({
			request_id: 42,
			session_id: 'storybook-session-abc',
			category: 'avoiding_work',
			analyzer: 'overselling_analyzer',
			advice:
				'Passive constructions defer accountability. Prefer active voice so the agent names what it is doing and who is responsible.',
			matched_text:
				'This could potentially be handled by refactoring the module at some point in the future.'
		}),
		buildFinding({
			request_id: 43,
			session_id: 'storybook-session-abc',
			category: 'deflecting_blame',
			analyzer: 'deflecting_blame_analyzer',
			advice:
				"Hedge phrases reduce the agent's apparent commitment to correctness. State what is known directly.",
			matched_text: 'It seems like the tests might be failing due to an environment issue.'
		}),
		buildFinding({
			request_id: 44,
			session_id: 'storybook-session-def',
			category: 'justifying_shortcuts',
			analyzer: 'justifying_shortcuts_analyzer',
			advice: 'Vague justifications hide the actual trade-off. Name the constraint explicitly.',
			matched_text:
				'There are various reasons why the full implementation would be complex to add right now.'
		}),
		buildFinding({
			request_id: 45,
			session_id: 'storybook-session-def',
			category: 'faking_understanding',
			analyzer: 'deflecting_blame_analyzer',
			advice: 'Acknowledge what is unknown directly rather than appearing to understand.',
			matched_text: 'This generally follows the standard pattern for these types of operations.'
		}),
		buildFinding({
			request_id: 46,
			session_id: 'storybook-session-ghi',
			category: 'overselling',
			analyzer: 'justifying_shortcuts_analyzer',
			advice: 'Avoid claiming completeness when the scope was narrower than stated.',
			matched_text:
				'The implementation is now fully complete and handles all edge cases comprehensively.'
		})
	];

	type FetchRoute = {
		stats: StatsResponse;
		diagnosticStats: DiagnosticStats;
		findings: Finding[];
	};

	function stubAnalysisFetch(routes: FetchRoute): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/diagnostics/stats')) {
				return new Response(JSON.stringify(routes.diagnosticStats), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/diagnostics/findings')) {
				return new Response(JSON.stringify(routes.findings), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/api/stats')) {
				return new Response(JSON.stringify(routes.stats), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	function stubAnalysisFetchError(): void {
		globalThis.fetch = async () =>
			new Response(JSON.stringify({ error: 'Service Unavailable' }), {
				status: 503,
				statusText: 'Service Unavailable'
			});
	}

	const { Story } = defineMeta({
		title: 'Routes/Analysis',
		component: Page,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 120 }),
				diagnosticStats: POPULATED_STATS,
				findings: POPULATED_FINDINGS
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Empty"
	loaders={[
		async () => {
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 47 }),
				diagnosticStats: buildDiagnosticStats({
					totals_by_category: {},
					totals_by_analyzer: {},
					last_24h: 0
				}),
				findings: []
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	loaders={[
		async () => {
			globalThis.fetch = () => new Promise(() => {});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="Error"
	loaders={[
		async () => {
			stubAnalysisFetchError();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="NoObservedTraffic"
	loaders={[
		async () => {
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 0 }),
				diagnosticStats: buildDiagnosticStats({
					totals_by_category: {},
					totals_by_analyzer: {},
					last_24h: 0
				}),
				findings: []
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="ManyFindingsHasMore"
	loaders={[
		async () => {
			const cats = [
				'avoiding_work',
				'deflecting_blame',
				'justifying_shortcuts',
				'faking_understanding',
				'overselling'
			] as const;
			const analyzers = [
				'overselling_analyzer',
				'deflecting_blame_analyzer',
				'justifying_shortcuts_analyzer'
			] as const;
			const many: Finding[] = Array.from({ length: 50 }, (_, i) =>
				buildFinding({
					request_id: 5000 + i,
					session_id: `01J9X4QF7H8K2N3P5R6T7V8W9Y-route-blitz-${i % 5}`,
					category: cats[i % cats.length]!,
					analyzer: analyzers[i % analyzers.length]!,
					advice:
						'This finding is rendered with its full advice text — no truncation. The analyzer registry attaches the complete remediation guidance for every match.',
					matched_text: `Finding #${i}: this could potentially be handled by refactoring the module at some point in the future, which arguably might address most of the edge cases.`,
					other_hits: i % 4 === 0 ? 3 + (i % 7) : 0
				})
			);
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 1280 }),
				diagnosticStats: POPULATED_STATS,
				findings: many
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="DeepTreeExpanded"
	loaders={[
		async () => {
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 120 }),
				diagnosticStats: POPULATED_STATS,
				findings: POPULATED_FINDINGS
			});
			return {};
		}
	]}
	parameters={{
		a11y: { test: 'todo' },
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const firstCat = await waitFor(
			() => {
				const row = canvasElement.querySelector(
					'[data-slot="category-row"] [role="button"][aria-expanded]'
				);
				if (!row) throw new Error('category row not yet rendered');
				return row as HTMLButtonElement;
			},
			{ timeout: 4000 }
		);
		await expect(firstCat).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(firstCat);
		await waitFor(
			() => {
				if (firstCat.getAttribute('aria-expanded') !== 'true') {
					throw new Error('category not yet expanded');
				}
			},
			{ timeout: 3000 }
		);
		const firstSession = await waitFor(
			() => {
				const row = canvasElement.querySelector(
					'[data-slot="session-row"] [role="button"][aria-expanded]'
				);
				if (!row) throw new Error('session row not yet revealed');
				return row as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(firstSession);
		await expect(firstSession).toHaveAttribute('aria-expanded', 'true');
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="AnalyzerFilterActive"
	loaders={[
		async () => {
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 120 }),
				diagnosticStats: POPULATED_STATS,
				findings: POPULATED_FINDINGS
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const analyzerRow = await waitFor(
			() => {
				const tbl = canvasElement.querySelector('[data-slot="analyzer-table"]');
				if (!tbl) throw new Error('analyzer table not yet rendered');
				const row = tbl.querySelector('[role="button"][aria-label^="Filter by "]');
				if (!row) throw new Error('analyzer row not yet rendered');
				return row as HTMLElement;
			},
			{ timeout: 4000 }
		);
		await userEvent.click(analyzerRow);
		const chip = await waitFor(
			() => {
				const el = canvasElement.querySelector('[data-slot="analyzer-filter-chip"]');
				if (!el) throw new Error('analyzer-filter chip not yet rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(chip).toBeInTheDocument();
		await expect(chip).toHaveTextContent(/filter: check=/i);
		void canvas;
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>

<Story
	name="FindingsLoadMoreError"
	loaders={[
		async () => {
			const many: Finding[] = Array.from({ length: 50 }, (_, i) =>
				buildFinding({
					request_id: 6000 + i,
					session_id: `01J9X4QF7H8K2N3P5R6T7V8W9Y-loadmore-${i % 4}`,
					category: (['avoiding_work', 'deflecting_blame', 'justifying_shortcuts'] as const)[
						i % 3
					]!,
					analyzer: (['overselling_analyzer', 'deflecting_blame_analyzer'] as const)[i % 2]!,
					advice:
						'Full advice text rendered — no truncation. The analyzer registry attaches complete remediation guidance for every match in the load-more page.',
					matched_text: `Finding #${i}: this could potentially be addressed by a follow-up refactor at some point, which arguably handles most of the edge cases in the load-more page.`
				})
			);
			stubAnalysisFetch({
				stats: buildStatsResponse({ request_count: 1280 }),
				diagnosticStats: POPULATED_STATS,
				findings: many
			});
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/analysis'),
					params: {}
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		__setRetryDelaysForTesting([0, 0, 0]);
		const canvas = within(canvasElement);
		const loadMoreBtn = await waitFor(
			() => {
				const wrap = canvasElement.querySelector('[data-slot="findings-load-more"]');
				if (!wrap) throw new Error('load-more block not yet rendered');
				const btn = wrap.querySelector('button');
				if (!btn) throw new Error('load-more button not yet rendered');
				return btn as HTMLButtonElement;
			},
			{ timeout: 4000 }
		);
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/diagnostics/findings')) {
				return new Response(JSON.stringify({ error: 'findings pagination unavailable' }), {
					status: 503,
					statusText: 'Service Unavailable',
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
		await userEvent.click(loadMoreBtn);
		const err = await waitFor(
			() => {
				const wrap = canvasElement.querySelector('[data-slot="findings-load-more"]');
				const alert = wrap?.querySelector('[role="alert"]');
				if (!alert) throw new Error('load-more error not yet rendered');
				return alert as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(err).toBeInTheDocument();
		await expect(err).toHaveTextContent(/failed: 503/i);
		void canvas;
		__resetRetryDelaysForTesting();
	}}
>
	{#snippet template()}
		<div class="h-[800px] w-[960px] overflow-y-auto bg-[var(--surface-bg)] p-6">
			<Page />
		</div>
	{/snippet}
</Story>
