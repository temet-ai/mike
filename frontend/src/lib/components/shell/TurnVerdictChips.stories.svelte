<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import TurnVerdictChips from './TurnVerdictChips.svelte';
	import type { OperatorTurnDto } from '$lib/types';

	const { Story } = defineMeta({
		title: 'Shell/TurnVerdictChips',
		component: TurnVerdictChips,
		tags: ['autodocs']
	});

	function turn(id: number, extra: Partial<OperatorTurnDto>): OperatorTurnDto {
		return {
			turn_id: id,
			session_id: '00000000-0000-4000-8000-000000000001',
			project_id: 1,
			start_request_id: id,
			end_request_id: id,
			start_request_number: 1,
			end_request_number: 1,
			user_prompt: 'add the retry',
			final_assistant_text: 'Retry added.',
			tool_call_count: 0,
			tool_error_count: 0,
			files_touched: [],
			tools_used: [],
			agents_dispatched: [],
			weasel_findings_count: 0,
			total_input_tokens: 0,
			total_output_tokens: 0,
			duration_ms: 0,
			outcome: 'unknown',
			outcome_evidence: null,
			classified_at: null,
			created_at: '2026-09-24T10:00:00Z',
			updated_at: '2026-09-24T10:00:00Z',
			...extra
		};
	}

	const TURNS = [
		turn(1, {
			outcome: 'corrected',
			outcome_source: 'judge',
			reaction: 'corrected',
			delivery: 'partial'
		}),
		turn(2, {
			outcome: 'shipped',
			outcome_source: 'operator',
			reaction: 'accepted',
			delivery: 'shipped'
		}),
		turn(3, { outcome: 'unknown', outcome_source: 'no_verdict' }),
		turn(4, { outcome: 'abandoned', outcome_source: null, legacy: true }),
		turn(5, { outcome: 'unknown', outcome_source: null, pending: true }),
		turn(6, {
			outcome: 'accepted',
			outcome_source: 'judge',
			reaction: 'redirected',
			delivery: 'not_applicable'
		})
	];
</script>

<Story
	name="TurnVerdictChips"
	play={async ({ canvasElement }) => {
		const sources = [...canvasElement.querySelectorAll('[data-verdict-source]')].map((el) =>
			el.getAttribute('data-verdict-source')
		);
		await expect(sources).toEqual([
			'judge',
			'operator',
			'no verdict',
			'legacy',
			'pending',
			'judge'
		]);
		const first = canvasElement.querySelector('[data-verdict-source="judge"]');
		await expect(first?.querySelector('[data-verdict-axis="reaction"]')?.textContent?.trim()).toBe(
			'corrected'
		);
		const legacy = canvasElement.querySelector('[data-verdict-source="legacy"]');
		await expect(legacy?.querySelector('[data-verdict-axis="reaction"]')).toBeNull();
		await expect(canvasElement.textContent).toContain('not applicable');
		const link = canvasElement.querySelector('a[href="/api/operator-turns/1/verdicts"]');
		await expect(link).not.toBeNull();
	}}
>
	{#snippet template()}
		<div class="flex flex-col gap-2 bg-[var(--surface-bg)] p-4">
			{#each TURNS as t (t.turn_id)}
				<TurnVerdictChips turn={t} />
			{/each}
		</div>
	{/snippet}
</Story>
