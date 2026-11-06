<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import WorkflowRunBand from './WorkflowRunBand.svelte';
	import type { WorkflowRunDto, WorkflowAgentDto } from '$lib/types';

	const STORY_SESSION_ID = 'storybook-wf-session-1';
	const STORY_RUN_ID = 1;
	const STORY_RUN_UID = 'wf_abc123def456';
	const STORY_PARENT_REQUEST_ID = 201;
	const STATIC_NOW = new Date('2026-05-20T14:00:00Z').getTime();

	function buildAgent(
		overrides: Partial<WorkflowAgentDto> & { agent_id: string; status: 'started' | 'result' }
	): WorkflowAgentDto {
		return {
			agent_type: null,
			summary: null,
			gaps_count: 0,
			result_json: null,
			request_ids: [],
			...overrides
		};
	}

	const COMPLETE_RUN: WorkflowRunDto = {
		run_id: STORY_RUN_ID,
		run_uid: STORY_RUN_UID,
		name: 'Auth flow refactor',
		description: 'End-to-end JWT refresh flow rewrite and test coverage pass.',
		phases: ['research', 'implement', 'verify'],
		parent_request_id: 201,
		agent_count: 2,
		result_count: 2,
		agents: [
			buildAgent({
				agent_id: 'agt_01alpha',
				agent_type: 'rust-developer',
				status: 'result',
				summary:
					'Rewrote the JWT middleware to use HMAC-SHA256 with a 1-hour sliding window. Updated the `AuthGuard` extractor to short-circuit on bearer-token absence and return 401 with a structured JSON error body. All 14 integration tests pass.',
				gaps_count: 0,
				result_json: {
					files_modified: ['crates/mike/src/auth/jwt.rs', 'crates/mike/src/auth/guard.rs'],
					tests_added: 3,
					verdict: 'complete'
				},
				request_ids: [2010, 2011]
			}),
			buildAgent({
				agent_id: 'agt_02beta',
				agent_type: 'frontend-developer',
				status: 'result',
				summary:
					'Added the refresh-token flow to `authStore.svelte.ts`. The store now auto-refreshes 60 seconds before expiry using a `$effect`-based timer. The `useAuthGuard` hook is wired into `+layout.svelte`. Manual QA: sign-in, wait, token refreshes silently.',
				gaps_count: 1,
				result_json: {
					files_modified: [
						'frontend/src/lib/stores/authStore.svelte.ts',
						'frontend/src/routes/+layout.svelte'
					],
					gaps: ['No automated test for the 60-second refresh window — requires fake timers.'],
					verdict: 'complete_with_gaps'
				},
				request_ids: [2020, 2021, 2022]
			})
		]
	};

	const PARTIAL_RUN: WorkflowRunDto = {
		run_id: 2,
		run_uid: 'wf_partial001',
		name: 'API migration',
		description: 'Migrates the legacy REST calls to the new inspect API contract.',
		phases: ['audit', 'migrate'],
		parent_request_id: 301,
		agent_count: 3,
		result_count: 2,
		agents: [
			buildAgent({
				agent_id: 'agt_p1',
				agent_type: 'researcher',
				status: 'result',
				summary: 'Audited 47 API call sites across 12 frontend files. 6 use deprecated endpoints.',
				gaps_count: 0,
				result_json: { deprecated_count: 6, sites_audited: 47 },
				request_ids: [3010]
			}),
			buildAgent({
				agent_id: 'agt_p2',
				agent_type: 'frontend-developer',
				status: 'result',
				summary:
					'Migrated 4 of 6 deprecated call sites. `api.ts` and `sessionDetail.svelte.ts` updated.',
				gaps_count: 2,
				result_json: { migrated: 4, remaining: 2 },
				request_ids: [3020, 3021]
			}),
			buildAgent({
				agent_id: 'agt_p3',
				agent_type: 'frontend-developer',
				status: 'started',
				summary: null,
				gaps_count: 0,
				result_json: null,
				request_ids: []
			})
		]
	};

	const FAILED_RUN: WorkflowRunDto = {
		run_id: 3,
		run_uid: null,
		name: null,
		description: null,
		phases: [],
		parent_request_id: 401,
		agent_count: 2,
		result_count: 0,
		agents: [
			buildAgent({ agent_id: 'agt_f1', agent_type: 'rust-developer', status: 'started' }),
			buildAgent({ agent_id: 'agt_f2', agent_type: 'db-dev', status: 'started' })
		]
	};

	function stubFetch(runs: WorkflowRunDto[]): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = async (_url: RequestInfo | URL) => {
			return new Response(JSON.stringify(runs), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		};
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/WorkflowRunBand',
		component: WorkflowRunBand,
		tags: ['autodocs']
	});
</script>

<Story
	name="Loading"
	loaders={[
		async () => {
			const orig = globalThis.fetch;
			globalThis.fetch = async () => new Promise<Response>(() => {});
			return {
				restore: () => {
					globalThis.fetch = orig;
				}
			};
		}
	]}
>
	{#snippet template()}
		<div style="width: 560px; background: var(--surface-bg); padding: 12px;">
			<WorkflowRunBand sessionId={STORY_SESSION_ID} parentRequestId={201} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="SingleRunComplete"
	loaders={[
		async () => {
			const restore = stubFetch([COMPLETE_RUN]);
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		void within(canvasElement);

		const sectionBtn = await waitFor(
			() => {
				const el = canvasElement.querySelector('button.wf-header');
				if (!el) throw new Error('wf-header button not yet rendered');
				return el as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		if (sectionBtn.getAttribute('aria-expanded') !== 'true') {
			await userEvent.click(sectionBtn);
		}

		await waitFor(
			() => {
				const rows = canvasElement.querySelectorAll('.wf-agent-row');
				if (rows.length < 2) throw new Error('agent rows not yet rendered');
			},
			{ timeout: 3000 }
		);

		const firstToggle = await waitFor(
			() => {
				const el = canvasElement.querySelector('button[aria-label*="Expand agent agt_01alpha"]');
				if (!el) throw new Error('first agent toggle not yet rendered');
				return el as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(firstToggle);

		const fullSummaryTail = 'All 14 integration tests pass.';
		await waitFor(
			() => {
				const text = canvasElement.textContent ?? '';
				if (!text.includes(fullSummaryTail)) {
					throw new Error(`Summary text was truncated — expected to find: "${fullSummaryTail}"`);
				}
			},
			{ timeout: 3000 }
		);

		const resultJsonKey = '"verdict"';
		await waitFor(
			() => {
				const text = canvasElement.textContent ?? '';
				if (!text.includes(resultJsonKey)) {
					throw new Error(`result_json was truncated — expected to find key: ${resultJsonKey}`);
				}
			},
			{ timeout: 3000 }
		);

		const secondToggle = await waitFor(
			() => {
				const el = canvasElement.querySelector('button[aria-label*="Expand agent agt_02beta"]');
				if (!el) throw new Error('second agent toggle not yet rendered');
				return el as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		await userEvent.click(secondToggle);

		const gapText = 'No automated test for the 60-second refresh window';
		await waitFor(
			() => {
				const text = canvasElement.textContent ?? '';
				if (!text.includes(gapText)) {
					throw new Error(`result_json gap text was truncated — expected: "${gapText}"`);
				}
			},
			{ timeout: 3000 }
		);

		await waitFor(
			() => {
				const link = canvasElement.querySelector('a.wf-req-link');
				if (!link) throw new Error('request-id link not rendered');
			},
			{ timeout: 3000 }
		);
	}}
>
	{#snippet template()}
		<div style="width: 560px; background: var(--surface-bg); padding: 12px;">
			<WorkflowRunBand
				sessionId={STORY_SESSION_ID}
				parentRequestId={STORY_PARENT_REQUEST_ID}
				now={STATIC_NOW}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="PartialFailure"
	loaders={[
		async () => {
			const restore = stubFetch([PARTIAL_RUN]);
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		const sectionBtn = await waitFor(
			() => {
				const el = canvasElement.querySelector('button.wf-header');
				if (!el) throw new Error('wf-header not yet rendered');
				return el as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		if (sectionBtn.getAttribute('aria-expanded') !== 'true') {
			await userEvent.click(sectionBtn);
		}

		const badge = await waitFor(
			() => {
				const el = canvasElement.querySelector('.wf-badge--partial');
				if (!el) throw new Error('partial badge not rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(badge).toBeInTheDocument();

		await waitFor(
			() => {
				const el = canvasElement.querySelector('.wf-status-icon--pending');
				if (!el) throw new Error('pending status icon not rendered');
			},
			{ timeout: 3000 }
		);
	}}
>
	{#snippet template()}
		<div style="width: 560px; background: var(--surface-bg); padding: 12px;">
			<WorkflowRunBand sessionId={STORY_SESSION_ID} parentRequestId={301} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>

<Story
	name="FailedRun"
	loaders={[
		async () => {
			const restore = stubFetch([FAILED_RUN]);
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		const sectionBtn = await waitFor(
			() => {
				const el = canvasElement.querySelector('button.wf-header');
				if (!el) throw new Error('wf-header not yet rendered');
				return el as HTMLButtonElement;
			},
			{ timeout: 3000 }
		);
		if (sectionBtn.getAttribute('aria-expanded') !== 'true') {
			await userEvent.click(sectionBtn);
		}

		const badge = await waitFor(
			() => {
				const el = canvasElement.querySelector('.wf-badge--fail');
				if (!el) throw new Error('fail badge not rendered');
				return el as HTMLElement;
			},
			{ timeout: 3000 }
		);
		await expect(badge).toBeInTheDocument();

		await waitFor(
			() => {
				const icons = canvasElement.querySelectorAll('.wf-status-icon--pending');
				if (icons.length < 2) throw new Error(`expected 2 pending icons, got ${icons.length}`);
			},
			{ timeout: 3000 }
		);

		await waitFor(
			() => {
				const text = canvasElement.textContent ?? '';
				if (!text.includes('run #3')) {
					throw new Error('Expected fallback run #3 chrome label not found');
				}
			},
			{ timeout: 3000 }
		);
	}}
>
	{#snippet template()}
		<div style="width: 560px; background: var(--surface-bg); padding: 12px;">
			<WorkflowRunBand sessionId={STORY_SESSION_ID} parentRequestId={401} now={STATIC_NOW} />
		</div>
	{/snippet}
</Story>
