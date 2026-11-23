<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { MemoryPageData } from './+page';
	import { stubConsolidationFetch } from '$lib/test-support/__fixtures__/knowledgeConsolidation';
	import type { LessonOutcomeSummary, LessonRelation } from '$lib/api/memoryFeedback';
	import type { FleetProjectSummary } from '$lib/fleet/types';

	const { Story } = defineMeta({
		title: 'Routes/Memory',
		component: Page,
		tags: ['autodocs']
	});

	const PROJECTS_ALL: FleetProjectSummary[] = [
		{
			project_path: '/Users/dev/temet-proxy',
			project_id: 3,
			session_count: 41,
			request_count: 612,
			first_seen: '2026-05-02T09:00:00Z',
			last_seen: '2026-07-29T13:40:04Z',
			pinned: true,
			custom_name: null
		},
		{
			project_path: '/Users/dev/other-repo',
			project_id: 4,
			session_count: 6,
			request_count: 58,
			first_seen: '2026-06-11T10:00:00Z',
			last_seen: '2026-07-20T08:00:00Z',
			pinned: false,
			custom_name: null
		}
	];

	const RELATION_FROM_71_TEXT =
		'FIXED 2026-07-24: sccache + lld are required for the shared CARGO_TARGET_DIR — preflight-toolchain.sh now auto-installs both via Homebrew.';
	const RELATION_TO_42_TEXT =
		'The macOS linker needs lld installed manually before a shared target dir build will link in reasonable time.';

	const RELATIONS_ALL: LessonRelation[] = [
		{
			id: 9,
			uuid: '018f7c00-0000-7000-8000-000000000009',
			kind: 'supersedes',
			status: 'proposed',
			method: 'adjudicated',
			from_lesson_id: 71,
			from_lesson_text: RELATION_FROM_71_TEXT,
			from_lesson_created_at: '2026-07-24T11:00:00Z',
			to_lesson_id: 42,
			to_lesson_text: RELATION_TO_42_TEXT,
			to_lesson_created_at: '2026-06-17T08:00:00Z',
			candidate_distance: 0.0421,
			evidence_json: { quoted_span: 'preflight-toolchain.sh now auto-installs both via Homebrew' },
			proposer_model: 'claude-sonnet-5',
			proposer_rationale:
				"Lesson #71 restates #42's linker requirement but adds the auto-install fix, making #42 obsolete guidance.",
			adjudicator_model: 'claude-opus-5',
			adjudicator_agrees: true,
			adjudicator_rationale:
				'Confirmed #71 is a strict superset of #42 — the manual-install step is now wrong.',
			detector_version: 'lesson-relations-v1',
			proposed_at: '2026-07-29T02:10:00Z',
			decided_at: null,
			decided_by: null,
			created_at: '2026-07-29T02:10:00Z'
		},
		{
			id: 5,
			uuid: '018f7c00-0000-7000-8000-000000000005',
			kind: 'contradicts',
			status: 'contested',
			method: 'adjudicated',
			from_lesson_id: 30,
			from_lesson_text:
				'Never start colima to pass a gate — the capture DB is remote and reachable without it.',
			from_lesson_created_at: '2026-07-20T10:00:00Z',
			to_lesson_id: 12,
			to_lesson_text:
				'Start colima before running the DB-backed test tier, or the connection will refuse.',
			to_lesson_created_at: '2026-05-02T09:00:00Z',
			candidate_distance: 0.091,
			evidence_json: {},
			proposer_model: 'claude-sonnet-5',
			proposer_rationale:
				'Both lessons address the same colima-startup step but reach opposite conclusions.',
			adjudicator_model: 'claude-opus-5',
			adjudicator_agrees: false,
			adjudicator_rationale:
				'These are not the same claim — #12 predates the remote-DB migration and was correct for a local-only setup at the time.',
			detector_version: 'lesson-relations-v1',
			proposed_at: '2026-07-28T09:00:00Z',
			decided_at: null,
			decided_by: null,
			created_at: '2026-07-28T09:00:00Z'
		}
	];

	const OUTCOMES_ALL: LessonOutcomeSummary[] = [
		{
			lesson_id: 71,
			kind: 'long_term',
			lesson_text: RELATION_FROM_71_TEXT,
			injection_count: 14,
			distinct_session_count: 9,
			outcomes_observed_after_injection: [
				{ outcome: 'success', count: 6, avg_quality_score: 0.82 },
				{ outcome: 'partial', count: 2, avg_quality_score: 0.51 },
				{ outcome: 'failure', count: 1, avg_quality_score: 0.12 },
				{ outcome: 'unknown', count: 0, avg_quality_score: null }
			]
		},
		{
			lesson_id: 42,
			kind: 'daily',
			injection_count: 3,
			distinct_session_count: 3,
			outcomes_observed_after_injection: [
				{ outcome: 'success', count: 1, avg_quality_score: 0.6 },
				{ outcome: 'unknown', count: 2, avg_quality_score: null }
			]
		}
	];

	function buildData(): MemoryPageData {
		return {
			relations: { items: RELATIONS_ALL, total_count: RELATIONS_ALL.length, limit: 50, offset: 0 },
			relationsLoadError: null,
			lessonOutcomes: OUTCOMES_ALL,
			lessonOutcomesLoadError: null,
			projects: PROJECTS_ALL,
			projectsLoadError: null
		};
	}

	function jsonResponse(body: unknown): Promise<Response> {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function stubMemoryFetch(): void {
		stubConsolidationFetch('steady');
		const consolidationFetch = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url =
				typeof input === 'string'
					? input
					: input instanceof URL
						? input.href
						: (input as Request).url;

			if (url.includes('/api/memory/relations/')) {
				const match = url.match(/\/api\/memory\/relations\/(\d+)\/(confirm|reject)/);
				const id = match ? Number(match[1]) : null;
				const action = match ? match[2] : null;
				const relation = RELATIONS_ALL.find((r) => r.id === id);
				if (relation && action) {
					return jsonResponse({
						...relation,
						status: action === 'confirm' ? 'confirmed' : 'rejected',
						decided_at: '2026-07-29T14:00:00Z'
					});
				}
				return jsonResponse({ error: 'relation not found' });
			}
			if (url.includes('/api/memory/relations')) {
				return jsonResponse({
					items: RELATIONS_ALL,
					total_count: RELATIONS_ALL.length,
					limit: 50,
					offset: 0
				});
			}
			if (url.includes('/api/memory/lesson-outcomes')) {
				return jsonResponse(OUTCOMES_ALL);
			}
			if (url.includes('/api/projects')) {
				return jsonResponse(PROJECTS_ALL);
			}
			return consolidationFetch(input, init);
		}) as typeof fetch;
	}
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			stubMemoryFetch();
			return {};
		}
	]}
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/memory'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText(/your files are not altered/i)).toBeInTheDocument();
		const section = await canvas.findByRole('region', { name: 'Consolidation' });
		await expect(await within(section).findByText('Open conflicts')).toBeInTheDocument();
		await expect(within(section).getByText('Recent changes')).toBeInTheDocument();
		await expect(canvas.queryByText('Last nightly run')).not.toBeInTheDocument();
		await expect(
			canvas.queryByRole('button', { name: /load more decisions/i })
		).not.toBeInTheDocument();
		await expect(
			section.compareDocumentPosition(canvas.getByText(/your files are not altered/i)) &
				Node.DOCUMENT_POSITION_FOLLOWING
		).toBeTruthy();

		await expect(await canvas.findByText('From — lesson #71')).toBeInTheDocument();
		await expect(canvas.getByText('To — lesson #42')).toBeInTheDocument();
		await expect(
			canvas.getAllByText(RELATIONS_ALL[0]!.from_lesson_text).length
		).toBeGreaterThanOrEqual(2);
		await expect(canvas.getByText(RELATIONS_ALL[1]!.to_lesson_text)).toBeInTheDocument();
		await expect(canvas.getByText('contested')).toBeInTheDocument();
		await expect(canvas.getByText(RELATIONS_ALL[0]!.proposer_rationale!)).toBeInTheDocument();

		const confirmButtons = canvas.getAllByRole('button', { name: /^confirm$/i });
		await userEvent.click(confirmButtons[0]!);
		await waitFor(() => expect(canvas.getByText('confirmed')).toBeInTheDocument());
		await expect(canvas.getAllByRole('button', { name: /^confirm$/i }).length).toBe(
			confirmButtons.length - 1
		);

		await expect(canvas.getByText(/not proof the lesson caused it/i)).toBeInTheDocument();
		await expect(canvas.getByText('success 6 (avg quality 0.82)')).toBeInTheDocument();
		await expect(
			canvas.getByText(/full lesson text not returned by this endpoint yet/i)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1100px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page data={buildData()} />
		</div>
	{/snippet}
</Story>
