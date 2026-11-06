<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, fn } from 'storybook/test';
	import SessionAgent from './SessionAgent.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	type DispatchOutcome = 'completed' | 'abandoned' | 'error' | null;

	interface DispatchEvent {
		requestId: number;
		createdAt: string;
		outcome: DispatchOutcome;
		description: string | null;
	}

	interface AgentData {
		subagentType: string;
		model: string | null;
		description: string;
		events: DispatchEvent[];
	}

	const OUTCOME_LABELS: Record<Exclude<DispatchOutcome, null>, string> = {
		completed: 'Completed',
		abandoned: 'Abandoned',
		error: 'Errored'
	};

	const OUTCOME_TOOLTIPS: Record<Exclude<DispatchOutcome, null> | 'inflight', string> = {
		completed: 'The agent sub-task finished and reported back.',
		abandoned: 'The agent sub-task was cancelled or did not report back.',
		error: 'The agent sub-task terminated with an error.',
		inflight: 'The agent sub-task is still running — no outcome yet.'
	};

	const MODEL_TOOLTIPS: Record<string, string> = {
		'claude-sonnet-4-6':
			'claude-sonnet-4-6 — Anthropic Sonnet 4.6. Balanced performance and speed.',
		'claude-haiku-4-5-20251001':
			'claude-haiku-4-5-20251001 — Anthropic Haiku 4.5. Fast and cost-effective.',
		'claude-opus-4-8': 'claude-opus-4-8 — Anthropic Opus 4.8. Highest capability.'
	};

	function modelTooltip(model: string): string {
		return MODEL_TOOLTIPS[model] ?? model;
	}

	const AGENT_FIXTURE: AgentData = {
		subagentType: 'rust-developer',
		model: 'claude-sonnet-4-6',
		description:
			'Implement the JWT middleware refactor: rewrite `crates/mike/src/auth/jwt.rs` ' +
			'to use HMAC-SHA256 with a 1-hour sliding window. Update the `AuthGuard` extractor ' +
			'to short-circuit on bearer-token absence and return 401 with a structured JSON error body. ' +
			'Add integration tests covering the new guard path.',
		events: [
			{
				requestId: 2089,
				createdAt: '2026-06-25T09:52:18Z',
				outcome: 'completed',
				description:
					'Re-run: add integration tests covering the new AuthGuard extractor path. ' +
					'All 14 tests must pass before reporting complete.'
			},
			{
				requestId: 2042,
				createdAt: '2026-06-25T09:38:44Z',
				outcome: 'abandoned',
				description:
					'Implement the JWT middleware refactor and add integration tests covering the new AuthGuard extractor path.'
			},
			{
				requestId: 2011,
				createdAt: '2026-06-25T09:21:05Z',
				outcome: 'completed',
				description: 'Rewrite `crates/mike/src/auth/jwt.rs` to use HMAC-SHA256.'
			}
		]
	};

	const AGENT_SLOT: Record<string, string> = {
		'rust-developer': '1',
		'frontend-developer': '2',
		'ui-designer': '3',
		'db-dev': '4',
		watchdog: '5'
	};

	function agentFg(type: string): string {
		const slot = AGENT_SLOT[type];
		return slot ? `var(--agent-${slot}-fg)` : 'var(--text-faint)';
	}

	function outcomeStyle(outcome: DispatchOutcome): string {
		switch (outcome) {
			case 'completed':
				return 'color: var(--status-success-fg); background: var(--status-success-bg); border-color: var(--status-success-border);';
			case 'abandoned':
				return 'color: var(--text-2); background: var(--surface-2); border-color: var(--border-subtle);';
			case 'error':
				return 'color: var(--status-danger-fg); background: var(--status-danger-bg); border-color: var(--status-danger-border);';
			case null:
				return 'color: var(--status-info-fg); background: var(--surface-2); border-color: var(--border-subtle);';
		}
	}

	function formatTimestamp(iso: string): string {
		try {
			return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
		} catch {
			return iso;
		}
	}

	const onJumpTo = fn();

	const { Story } = defineMeta({
		title: 'Shell/SessionAgent',
		component: SessionAgent,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'One sub-agent entry with an expandable mini-timeline of all dispatch events. ' +
						'Outcome chips have tooltips. Model chip has a tooltip. ' +
						'Design brief: artifacts/2026-06-25/graders-ui-design.md §2 v3.'
				}
			}
		}
	});
</script>

<Story
	name="RustDeveloperAgent"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const typeEl = canvasElement.querySelector('.sa-type');
		await expect(typeEl).not.toBeNull();
		await expect(typeEl?.textContent?.trim()).toContain('rust-developer');

		const modelEl = canvasElement.querySelector('.sa-model');
		await expect(modelEl).not.toBeNull();
		await expect(modelEl?.getAttribute('title')).toContain('claude-sonnet-4-6');

		const descEl = canvasElement.querySelector('.sa-desc');
		await expect(descEl).not.toBeNull();
		await expect(descEl?.textContent).toContain('integration tests covering');

		const expandBtn = canvas.getByRole('button', { name: /show dispatches|hide dispatches/i });
		await userEvent.click(expandBtn);

		const completedRows = canvasElement.querySelectorAll('.sa-event--completed');
		await expect(completedRows.length).toBeGreaterThan(0);
		const outcomeLabel = completedRows[0]!.querySelector('.sa-event-outcome');
		await expect(outcomeLabel?.textContent?.trim()).toBe('Completed');

		await expect(outcomeLabel?.getAttribute('title')).toContain('finished');

		const jumpBtns = canvas.getAllByRole('button', { name: /jump to/i });
		await userEvent.click(jumpBtns[0]!);
		await expect(onJumpTo).toHaveBeenCalledWith(AGENT_FIXTURE.events[0]!.requestId);
	}}
>
	{#snippet template()}
		<div style="background: var(--surface-bg); padding: 16px; width: 100%;">
			<SessionAgent
				agent={AGENT_FIXTURE}
				agentColor={agentFg(AGENT_FIXTURE.subagentType)}
				{onJumpTo}
			/>
		</div>
	{/snippet}
</Story>
