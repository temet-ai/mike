<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import Page from './+page.svelte';
	import type { MemoryPageData } from './+page';
	import type { ConsolidationAction, ConsolidationRun } from '$lib/api/memoryDecisions';
	import type { LessonDto } from '$lib/types';
	import type { LessonOutcomeSummary, LessonRelation } from '$lib/api/memoryFeedback';
	import type { FleetProjectSummary } from '$lib/fleet/types';

	const { Story } = defineMeta({
		title: 'Routes/Memory',
		component: Page,
		tags: ['autodocs']
	});

	const RUNS: ConsolidationRun[] = [
		{
			id: 3,
			uuid: '018f7000-0000-7000-8000-000000000003',
			kind: 'session',
			trigger: 'schedule',
			status: 'completed',
			started_at: '2026-07-29T13:40:00Z',
			finished_at: '2026-07-29T13:40:04Z',
			error: null,
			action_count: 2
		},
		{
			id: 2,
			uuid: '018f7000-0000-7000-8000-000000000002',
			kind: 'nightly',
			trigger: 'schedule',
			status: 'completed',
			started_at: '2026-07-29T03:00:00Z',
			finished_at: '2026-07-29T03:00:41Z',
			error: null,
			action_count: 5
		},
		{
			id: 1,
			uuid: '018f7000-0000-7000-8000-000000000001',
			kind: 'gc',
			trigger: 'schedule',
			status: 'completed',
			started_at: '2026-07-27T03:05:00Z',
			finished_at: '2026-07-27T03:05:12Z',
			error: null,
			action_count: 1
		}
	];

	const ARCHIVED_ACTION: ConsolidationAction = {
		id: 68,
		run_id: 12,
		run_kind: 'session',
		lesson_id: 68,
		lesson_uuid: '018f7b2a-0000-7000-8000-000000000068',
		project_id: 3,
		project_path: '/Users/dev/temet-proxy',
		session_id: 501,
		session_uuid: '01j9x4qf7h8k2n3p5r6t7v8w9a',
		source_turn_id: 812,
		files_touched: ['crates/mike/src/store/persistence/lessons.rs'],
		agent_id: 4,
		lesson_text:
			'FIXED 2026-07-19: the seeded DB credential was postgres:postgres — corrected and confirmed the migration runner connects.',
		action_kind: 'archived_duplicate',
		outcome: 'applied',
		error: null,
		before_stage: 'daily',
		after_stage: 'archived',
		decided_by: 'embedding_distance',
		distance: 0.0421,
		threshold: 0.85,
		embedding_model: 'text-embedding-3-small',
		counterpart_lesson_id: 66,
		counterpart_uuid: '018f7b2a-0000-7000-8000-000000000066',
		counterpart_text: 'DB credential appears broken in local dev — needs investigation.',
		runners_up: [{ lesson_id: 61, distance: 0.79 }],
		rank_detail: null,
		injection_count: 4,
		created_at: '2026-07-19T09:13:00Z'
	};

	const PROMOTED_ACTION: ConsolidationAction = {
		id: 70,
		run_id: 12,
		run_kind: 'session',
		lesson_id: 70,
		lesson_uuid: '018f7b2a-0000-7000-8000-000000000070',
		project_id: 3,
		project_path: '/Users/dev/temet-proxy',
		session_id: 502,
		session_uuid: '01j9x4qf7h8k2n3p5r6t7v8w9b',
		source_turn_id: 820,
		files_touched: null,
		agent_id: null,
		lesson_text:
			'Prefer `scripts/cx` over the manual `.env` source preamble for cargo invocations.',
		action_kind: 'promoted_to_daily',
		outcome: 'applied',
		error: null,
		before_stage: 'raw',
		after_stage: 'daily',
		decided_by: null,
		distance: null,
		threshold: null,
		embedding_model: null,
		counterpart_lesson_id: null,
		counterpart_uuid: null,
		counterpart_text: null,
		runners_up: [],
		rank_detail: null,
		injection_count: 0,
		created_at: '2026-07-28T02:00:00Z'
	};

	const SKIPPED_ACTION: ConsolidationAction = {
		id: 71,
		run_id: 11,
		run_kind: 'nightly',
		lesson_id: 55,
		lesson_uuid: '018f7b2a-0000-7000-8000-000000000055',
		project_id: 3,
		project_path: '/Users/dev/temet-proxy',
		session_id: null,
		session_uuid: null,
		source_turn_id: null,
		files_touched: null,
		agent_id: null,
		lesson_text:
			'A recurrence on a different worker with inverted direction — guards were necessary but incomplete.',
		action_kind: 'evaluated',
		outcome: 'skipped',
		error: 'embedding service unavailable — neighbour search skipped',
		before_stage: 'daily',
		after_stage: 'daily',
		decided_by: null,
		distance: null,
		threshold: null,
		embedding_model: null,
		counterpart_lesson_id: null,
		counterpart_uuid: null,
		counterpart_text: null,
		runners_up: [],
		rank_detail: { consolidation_count: 2 },
		injection_count: null,
		created_at: '2026-07-25T03:00:00Z'
	};

	const PROJECTS_ALL: FleetProjectSummary[] = [
		{
			project_path: '/Users/dev/temet-proxy',
			session_count: 41,
			request_count: 612,
			first_seen: '2026-05-02T09:00:00Z',
			last_seen: '2026-07-29T13:40:04Z',
			pinned: true,
			custom_name: null
		},
		{
			project_path: '/Users/dev/other-repo',
			session_count: 6,
			request_count: 58,
			first_seen: '2026-06-11T10:00:00Z',
			last_seen: '2026-07-20T08:00:00Z',
			pinned: false,
			custom_name: null
		}
	];

	const DECISIONS_ALL = [ARCHIVED_ACTION, PROMOTED_ACTION, SKIPPED_ACTION];
	const DECISIONS_PAGE_1_A = DECISIONS_ALL[0]!;
	const DECISIONS_PAGE_1_B = DECISIONS_ALL[1]!;

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
			runs: RUNS,
			runsLoadError: null,
			decisions: {
				items: [DECISIONS_PAGE_1_A, DECISIONS_PAGE_1_B],
				total_count: DECISIONS_ALL.length,
				limit: 50,
				offset: 0
			},
			decisionsLoadError: null,
			relations: { items: RELATIONS_ALL, total_count: RELATIONS_ALL.length, limit: 50, offset: 0 },
			relationsLoadError: null,
			lessonOutcomes: OUTCOMES_ALL,
			lessonOutcomesLoadError: null,
			projects: PROJECTS_ALL,
			projectsLoadError: null,
			nightlyHourUtc: 3
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
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url =
				typeof input === 'string'
					? input
					: input instanceof URL
						? input.href
						: (input as Request).url;

			if (url.includes('/api/memory/decisions')) {
				const search = new URL(url, 'http://localhost').searchParams;
				const offset = Number(search.get('offset') ?? '0');
				const limit = Number(search.get('limit') ?? '50');
				const actionKind = search.get('action_kind');
				const project = search.get('project');
				let filtered = DECISIONS_ALL;
				if (actionKind) filtered = filtered.filter((a) => a.action_kind === actionKind);
				if (project) filtered = filtered.filter((a) => a.project_path === project);
				return jsonResponse({
					items: filtered.slice(offset, offset + limit),
					total_count: filtered.length,
					limit,
					offset
				});
			}
			if (url.includes('/api/memory/consolidation/runs')) {
				return jsonResponse({ items: RUNS, total_count: RUNS.length, limit: 30, offset: 0 });
			}
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
			if (url.includes('/restore')) {
				const restored: LessonDto = {
					id: 68,
					project_id: 3,
					project_path: '/Users/dev/temet-proxy',
					kind: 'long_term',
					text: ARCHIVED_ACTION.lesson_text,
					source_turn_id: 812,
					created_at: '2026-07-19T09:13:00Z',
					expires_at: null,
					topic: null
				};
				return jsonResponse(restored);
			}
			return jsonResponse({});
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
		await expect(await canvas.findByText('Last nightly run')).toBeInTheDocument();
		await expect(await canvas.findByText(ARCHIVED_ACTION.lesson_text)).toBeInTheDocument();
		await expect(await canvas.findByText(/Showing 2 of 3/)).toBeInTheDocument();

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

		await userEvent.click(canvas.getByRole('button', { name: /load more decisions/i }));
		await waitFor(() => expect(canvas.getByText(SKIPPED_ACTION.error!)).toBeInTheDocument());

		const restoreButton = canvas.getByRole('button', { name: /restore this lesson/i });
		await userEvent.click(restoreButton);
		await waitFor(() =>
			expect(canvas.queryByRole('button', { name: /restore this lesson/i })).not.toBeInTheDocument()
		);

		const actionSelect = canvas.getByLabelText('Action') as HTMLSelectElement;
		await userEvent.selectOptions(actionSelect, 'promoted_to_daily');
		await userEvent.click(canvas.getByRole('button', { name: /^apply$/i }));
		await waitFor(() => expect(canvas.queryByText(/Showing/)).not.toBeInTheDocument());
		await expect(canvas.getByText(PROMOTED_ACTION.lesson_text)).toBeInTheDocument();

		const projectSelect = canvas.getByLabelText('Project') as HTMLSelectElement;
		await expect(
			within(projectSelect).getByRole('option', { name: '/Users/dev/other-repo' })
		).toBeInTheDocument();

		await userEvent.selectOptions(projectSelect, '/Users/dev/other-repo');
		await userEvent.click(canvas.getByRole('button', { name: /^apply$/i }));
		await waitFor(() =>
			expect(
				canvas.getByText(/No decisions match project \/Users\/dev\/other-repo/i)
			).toBeInTheDocument()
		);
		await expect(canvas.getByText(/require an exact match/i)).toBeInTheDocument();

		const knownProjectButton = canvas.getByRole('button', { name: '/Users/dev/temet-proxy' });
		await userEvent.click(knownProjectButton);
		await waitFor(() => expect(canvas.getByText(PROMOTED_ACTION.lesson_text)).toBeInTheDocument());

		await userEvent.selectOptions(projectSelect, '__other__');
		const otherProjectInput = canvas.getByPlaceholderText('/exact/project/path') as HTMLInputElement;
		await expect(otherProjectInput).toHaveValue('');

		await userEvent.type(otherProjectInput, '/mnt/data/projects/temet/temet-proxy');
		await userEvent.click(canvas.getByRole('button', { name: /^apply$/i }));
		await waitFor(() =>
			expect(
				canvas.getByText(/No decisions match project \/mnt\/data\/projects\/temet\/temet-proxy/i)
			).toBeInTheDocument()
		);
	}}
>
	{#snippet template()}
		<div class="h-[900px] w-[1100px] overflow-y-auto bg-[var(--surface-bg)]">
			<Page data={buildData()} />
		</div>
	{/snippet}
</Story>
