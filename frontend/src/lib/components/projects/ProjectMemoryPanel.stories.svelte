<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import ProjectMemoryPanel from './ProjectMemoryPanel.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	import type { RecallProjectContextDto, LessonDto } from '$lib/types';
	import type { ArchivedLesson } from '$lib/api/memoryDecisions';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import { __resetRetryDelaysForTesting, __setRetryDelaysForTesting } from '$lib/api';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';

	const PROJECT_PATH = 'projects/temet/temet-proxy';

	const RECALL: RecallProjectContextDto = {
		project_path: PROJECT_PATH,
		lessons: [
			{
				id: 501,
				project_id: 7,
				project_path: PROJECT_PATH,
				kind: 'correction',
				text: "Never `git reset --hard` in a shared worktree — it destroyed a peer session's uncommitted work on 2026-06-04. Scope destructive git ops to files this session actually owns.",
				source_turn_id: 9001,
				created_at: '2026-07-15T10:00:00Z',
				expires_at: null,
				topic: 'git-safety'
			},
			{
				id: 502,
				project_id: 7,
				project_path: PROJECT_PATH,
				kind: 'preference',
				text: 'Prefer scoped `cargo check -p mike` over the full 66-binary nextest compile in the inner loop; save the full gate for the end.',
				source_turn_id: null,
				created_at: '2026-07-10T09:30:00Z',
				expires_at: null,
				topic: null
			}
		],
		recent_corrected: [
			{
				turn_id: 9001,
				session_id: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
				project_id: 7,
				start_request_id: 4001,
				end_request_id: 4004,
				start_request_number: 12,
				end_request_number: 15,
				user_prompt: 'Delete the stale worktree at .claude/worktrees/old-thing please.',
				final_assistant_text: 'Ran `rm -rf .claude/worktrees/old-thing` directly.',
				tool_call_count: 2,
				tool_error_count: 0,
				files_touched: [],
				tools_used: ['Bash'],
				agents_dispatched: [],
				weasel_findings_count: 0,
				total_input_tokens: 4200,
				total_output_tokens: 380,
				duration_ms: 4200,
				outcome: 'corrected',
				outcome_evidence:
					'No — use `git worktree remove` so the branch registration is cleaned up too, not a raw rm -rf.',
				classified_at: '2026-07-15T10:05:00Z',
				created_at: '2026-07-15T10:00:00Z',
				updated_at: '2026-07-15T10:05:00Z',
				stop_response: 'Ran `rm -rf .claude/worktrees/old-thing` directly.',
				stop_reason: 'end_turn',
				stop_request_id: 4004,
				interrupted: false
			}
		],
		recent_shipped: [
			{
				turn_id: 8801,
				session_id: 'b2c3d4e5-f6a7-8901-bcde-f12345678901',
				project_id: 7,
				start_request_id: 3801,
				end_request_id: 3809,
				start_request_number: 40,
				end_request_number: 48,
				user_prompt:
					'Add the /api/recall/project-context endpoint bundling lessons + recent corrected + recent shipped turns.',
				final_assistant_text:
					'Shipped `recall_project_context` in handlers.rs — bundles 5 most-recent corrected turns, 3 most-recent shipped turns, and active lessons for the project into one response.',
				tool_call_count: 14,
				tool_error_count: 0,
				files_touched: ['crates/mike/src/api/handlers.rs'],
				tools_used: ['Read', 'Edit', 'Bash'],
				agents_dispatched: [],
				weasel_findings_count: 0,
				total_input_tokens: 18000,
				total_output_tokens: 2100,
				duration_ms: 61000,
				outcome: 'shipped',
				outcome_evidence: null,
				classified_at: '2026-07-14T16:00:00Z',
				created_at: '2026-07-14T15:40:00Z',
				updated_at: '2026-07-14T16:00:00Z',
				stop_response:
					'Shipped `recall_project_context` in handlers.rs — bundles 5 most-recent corrected turns, 3 most-recent shipped turns, and active lessons for the project into one response.',
				stop_reason: 'end_turn',
				stop_request_id: 3809,
				interrupted: false
			}
		]
	};

	const ARCHIVED_LESSON: ArchivedLesson = {
		id: 68,
		uuid: '018f7b2a-0000-7000-8000-000000000068',
		project_id: 7,
		project_path: PROJECT_PATH,
		kind: 'correction',
		text: 'FIXED 2026-07-19: the seeded DB credential was postgres:postgres — corrected and confirmed the migration runner connects.',
		topic: 'db-safety',
		session_id: 501,
		session_uuid: 'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
		source_turn_id: 812,
		agent_id: 4,
		created_at: '2026-07-19T09:13:00Z',
		archived_at: '2026-07-19T09:13:00Z',
		archived_reason: 'duplicate',
		duplicate_of_id: 66
	};

	let recallCalls = 0;
	let archivedCalls = 0;

	const { Story } = defineMeta({
		title: 'Projects/ProjectMemoryPanel',
		component: ProjectMemoryPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	args={{ projectPath: PROJECT_PATH }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			stubFetchJsonRouter((method, url) => {
				if (url.includes('/api/recall/project-context')) return typed<RecallProjectContextDto>(RECALL);
				if (url.includes('/api/lessons/archived')) return typed<ArchivedLesson[]>([ARCHIVED_LESSON]);
				if (url.includes('/restore')) {
					const restored: LessonDto = {
						id: 68,
						project_id: 7,
						project_path: PROJECT_PATH,
						kind: 'long_term',
						text: ARCHIVED_LESSON.text,
						source_turn_id: 812,
						created_at: '2026-07-19T09:13:00Z',
						expires_at: null,
						topic: null
					};
					return typed<LessonDto>(restored);
				}
				return typed<Record<string, never>>({});
			});
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const toggle = canvas.getByRole('button', { expanded: false });
		await expect(canvasElement.querySelector('.dsc-body')).toBeNull();

		const helpIcon = canvasElement.querySelector('.dsc-title-help') as HTMLElement;
		await expect(helpIcon).toBeInTheDocument();
		await expect(toggle.querySelector('.dsc-title-help')).toBeNull();

		await userEvent.click(toggle);
		await waitFor(() => expect(toggle).toHaveAttribute('aria-expanded', 'true'));

		await waitFor(() =>
			expect(canvas.getByText('2 lessons · 1 corrected · 1 shipped')).toBeInTheDocument()
		);

		await expect(
			canvas.getByText(/never `git reset --hard` in a shared worktree/i)
		).toBeInTheDocument();
		await expect(canvas.getByText('git-safety')).toBeInTheDocument();
		await expect(canvas.getByText('correction')).toBeInTheDocument();
		await expect(canvas.getByText('preference')).toBeInTheDocument();

		const correctedPrompt = canvas.getByText(
			'Delete the stale worktree at .claude/worktrees/old-thing please.'
		);
		const correctedTurn = correctedPrompt.closest(
			'[data-slot="project-memory-turn"]'
		) as HTMLElement;
		await expect(within(correctedTurn).getByText('click turn to expand')).toBeInTheDocument();
		await expect(
			within(correctedTurn).queryByText(/rm -rf \.claude\/worktrees\/old-thing directly/)
		).toBeNull();

		await userEvent.click(correctedTurn);
		await waitFor(() => expect(correctedTurn).toHaveAttribute('aria-expanded', 'true'));
		await expect(
			within(correctedTurn).getByText(
				'Delete the stale worktree at .claude/worktrees/old-thing please.'
			)
		).toBeInTheDocument();
		await expect(
			within(correctedTurn).getByText('Ran `rm -rf .claude/worktrees/old-thing` directly.')
		).toBeInTheDocument();

		const shippedPrompt = await canvas.findByText(
			/Add the \/api\/recall\/project-context endpoint bundling lessons/
		);
		const shippedTurn = shippedPrompt.closest('[data-slot="project-memory-turn"]') as HTMLElement;
		await userEvent.click(shippedTurn);
		await waitFor(() => expect(shippedTurn).toHaveAttribute('aria-expanded', 'true'));
		await expect(
			within(shippedTurn).getByText(/Shipped `recall_project_context` in handlers\.rs/)
		).toBeInTheDocument();

		await waitFor(() => expect(canvas.getByText(ARCHIVED_LESSON.text)).toBeInTheDocument());
		await expect(canvas.getByText(/merged as a duplicate/i)).toBeInTheDocument();
		await expect(
			canvas.getByText(`duplicate of lesson #${ARCHIVED_LESSON.duplicate_of_id}`)
		).toBeInTheDocument();
		const restoreButton = canvas.getByRole('button', { name: /restore this lesson/i });
		await userEvent.click(restoreButton);
		await waitFor(() =>
			expect(canvas.queryByRole('button', { name: /restore this lesson/i })).not.toBeInTheDocument()
		);
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			{#snippet children()}
				<div class="w-[560px] bg-[var(--surface-bg)] p-6">
					<ProjectMemoryPanel {...args} />
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>

<Story
	name="Error"
	args={{ projectPath: PROJECT_PATH }}
	loaders={[
		async () => {
			localStorage.clear();
			__resetApiCacheForTests();
			__setRetryDelaysForTesting([]);
			recallCalls = 0;
			archivedCalls = 0;
			globalThis.fetch = (async (input: RequestInfo | URL) => {
				const url =
					typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
				if (url.includes('/api/recall/project-context')) recallCalls += 1;
				if (url.includes('/api/lessons/archived')) archivedCalls += 1;
				return new Response('down', { status: 503, statusText: 'Service Unavailable' });
			}) as typeof fetch;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { expanded: false }));

		await waitFor(() => expect(canvas.getByText("Couldn't load project memory")).toBeInTheDocument());
		await waitFor(() =>
			expect(canvas.getByText("Couldn't load archived lessons")).toBeInTheDocument()
		);
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(recallCalls).toBe(1);
		await expect(archivedCalls).toBe(1);

		const [retryMemory, retryArchived] = canvas.getAllByRole('button', { name: /retry/i });
		await userEvent.click(retryMemory!);
		await waitFor(() => expect(recallCalls).toBe(2));
		await userEvent.click(retryArchived!);
		await waitFor(() => expect(archivedCalls).toBe(2));
		await new Promise((resolve) => setTimeout(resolve, 1200));
		await expect(recallCalls).toBe(2);
		await expect(archivedCalls).toBe(2);
		__resetRetryDelaysForTesting();
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			<ProjectMemoryPanel {...args} />
		</CollapseStoreProvider>
	{/snippet}
</Story>
