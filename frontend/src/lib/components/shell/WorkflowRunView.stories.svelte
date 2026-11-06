<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent } from 'storybook/test';
	import WorkflowRunView, { type WorkflowRunModel } from './WorkflowRunView.svelte';

	const RUN: WorkflowRunModel = {
		name: 'turtles-implement',
		description:
			'Execute one wave of the turtles implementation plan: implement → independently verify each unit, escalation ladder on failure, deterministic wave gate at the end',
		elapsedMs: 42 * 60_000 + 29_000,
		phases: [
			{
				name: 'Extract',
				status: 'completed',
				agents: [
					{
						label: 'extract:wave-6',
						model: 'Haiku 4.5',
						status: 'completed',
						tokens: 41_200,
						toolCalls: 9,
						durationMs: 2 * 60_000 + 10_000,
						requestIds: [88950]
					}
				]
			},
			{
				name: 'Implement',
				status: 'active',
				agents: [
					{
						label: 'impl:T-12a',
						model: 'Sonnet 5',
						status: 'failed',
						tokens: 108_700,
						toolCalls: 28,
						durationMs: 5 * 60_000 + 50_000,
						failureReason:
							'API Error: Connection closed mid-response. The response above may be incomplete.',
						requestIds: [89010, 89044]
					},
					{
						label: 'impl:T-12b',
						model: 'Sonnet 5',
						status: 'completed',
						tokens: 172_800,
						toolCalls: 51,
						durationMs: 13 * 60_000 + 30_000,
						requestIds: [89060, 89112],
						prompt: {
							lines: 32,
							text: 'UNIT T-12b — Runtime machine — budgets + upward propagation (tier R3, implement)\n\nImplement the budget-enforcement section of the runtime machine so that every dispatch/re-dispatch/kill step checks the hard budgets before running, appends a budget-exhausted ledger entry on exceed, and propagates the exhaustion upward without resetting preserved context.'
						},
						activity: [
							'Write(/private/tmp/claude-501/…/lib/runtime/machine.ts)',
							'Bash(npx tsx "/private/tmp/claude-501/…/scripts/probe.ts")',
							'Bash(npx vitest run lib/runtime/machine.budgets.test.ts 2>&1)'
						],
						outcome:
							"Summary (≤200 words): Implemented T-12b in lib/runtime/machine.ts's budget section: checkHardBudgets runs before every dispatch/re-dispatch/kill-step in runInner's loop, checking maxReinstructs/maxGoalAttempts (new goalAttempts derived count) and, when the target state is verification, maxVerifications. On exceed, appendBudgetExhausted appends a budget-exhausted LedgerEntry whose payload bubbles the full accumulated EvidenceLedger (plus limit/count/max), also surfaced on RunResult.budgetExhausted. Added handleAlignmentCorrectionEntry and handleKilledEntry. Test gate output (npx vitest run lib/runtime/machine.budgets.test.ts): 14 passed."
					},
					{
						label: 'impl:T-12c',
						model: 'Sonnet 5',
						status: 'completed',
						tokens: 195_800,
						toolCalls: 56,
						durationMs: 17 * 60_000 + 47_000,
						requestIds: [89070, 89130]
					},
					{
						label: 'verify:T-12b-r1',
						model: 'Sonnet 5',
						status: 'completed',
						tokens: 78_900,
						toolCalls: 28,
						durationMs: 3 * 60_000 + 24_000,
						requestIds: [89200]
					},
					{
						label: 'verify:T-12c-r1',
						model: 'Sonnet 5',
						status: 'completed',
						tokens: 83_500,
						toolCalls: 23,
						durationMs: 4 * 60_000 + 20_000,
						requestIds: [89210]
					},
					{
						label: 'fix:T-12c-r1',
						model: 'Sonnet 5',
						status: 'running',
						tokens: 152_100,
						toolCalls: 36,
						durationMs: null,
						requestIds: [89320]
					}
				]
			},
			{
				name: 'Gate',
				status: 'pending',
				agents: []
			}
		]
	};

	const { Story } = defineMeta({
		title: 'Shell/WorkflowRunView',
		component: WorkflowRunView,
		tags: ['autodocs'],
		args: { run: RUN }
	});
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('turtles-implement')).toBeInTheDocument();
		await expect(canvas.getByText(/5\/7 agents/)).toBeInTheDocument();
		await expect(canvas.getByRole('group', { name: 'Phases' })).toBeInTheDocument();

		await expect(canvas.getByText('impl:T-12b')).toBeInTheDocument();
		await expect(canvas.getByText(/failed: API Error/i)).toBeInTheDocument();

		await userEvent.click(canvas.getByText('impl:T-12b'));
		await expect(canvas.getByText('Completed')).toBeInTheDocument();
		await expect(canvas.getByText('Outcome')).toBeInTheDocument();
		await expect(canvas.getByText(/Implemented T-12b/i)).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: /back to agents/i }));
		await expect(canvas.getByText('verify:T-12c-r1')).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<div style="width: 1000px; background: var(--surface-sidebar); padding: 16px;">
			<WorkflowRunView {...args} />
		</div>
	{/snippet}
</Story>
