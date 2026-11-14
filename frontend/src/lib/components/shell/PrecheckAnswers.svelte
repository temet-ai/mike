<script lang="ts" module>
	export interface PrecheckQuestionCopy {
		key: string;
		label: string;
		direction: string;
	}

	export const PRECHECK_QUESTIONS: readonly PrecheckQuestionCopy[] = [
		{
			key: 'already_disclosed',
			label: 'Says what was not checked',
			direction: 'A high value favours sending the nudge.'
		},
		{
			key: 'claims_checked',
			label: 'Claims it was checked',
			direction: 'A high value favours holding the nudge.'
		},
		{
			key: 'answer_not_change',
			label: 'Is an answer, not a change',
			direction: 'A high value favours holding the nudge.'
		},
		{
			key: 'invites_trial',
			label: 'Asks the operator to try it',
			direction: 'A high value favours sending the nudge.'
		}
	];
</script>

<script lang="ts">
	import type { Precheck } from '$lib/api/steering';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { ShareBar } from '$lib/components/ui/share-bar';

	interface Props {
		precheck: Precheck | null;
		applies: boolean;
	}

	const { precheck, applies }: Props = $props();

	const SCORE_TIP =
		'The average of the four answers after flipping the two that argue against sending. A score under the hold threshold holds the nudge.';

	function valueOf(key: string): number | null {
		const v = precheck?.answers[key];
		return typeof v === 'number' ? v : null;
	}
</script>

{#if applies}
	<div class="pca" data-slot="precheck-answers">
		<div class="pca__head">
			<span class="machine pca__label">Pre-check</span>
			{#if precheck !== null}
				<Tooltip text={SCORE_TIP}>
					{#snippet child({ props })}
						<span
							class="pca__verdict"
							data-send={precheck.verdict.send ? '' : undefined}
							{...props}
						>
							{precheck.verdict.send ? 'SENT' : 'HELD'}
							{precheck.verdict.score.toFixed(2)}
						</span>
					{/snippet}
				</Tooltip>
			{/if}
		</div>

		{#if precheck === null}
			<p class="pca__none">Pre-check not run</p>
		{:else}
			{#if precheck.fail_open}
				<p class="pca__failed" role="status">
					<span class="machine">Failed open</span>
					{#if precheck.error}<span>{precheck.error}</span>{/if}
				</p>
			{/if}
			<div class="pca__rows">
				{#each PRECHECK_QUESTIONS as q (q.key)}
					{@const value = valueOf(q.key)}
					<div class="pca__row" data-answer={q.key}>
						<Tooltip text={q.direction}>
							{#snippet child({ props })}
								<span class="pca__q" {...props}>{q.label}</span>
							{/snippet}
						</Tooltip>
						<ShareBar segments={[{ value: value ?? 0, tone: 'info' }]} max={1} />
						<span class="pca__v">{value === null ? '—' : value.toFixed(2)}</span>
					</div>
				{/each}
			</div>
			<p class="pca__why">
				{precheck.verdict.why}{precheck.refusals > 0 ? ` · ${precheck.refusals} refused` : ''}
			</p>
		{/if}
	</div>
{/if}

<style>
	.pca {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.pca__head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
		padding-bottom: var(--space-1);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.pca__label {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.pca__verdict {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		color: var(--text-1);
		cursor: help;
	}
	.pca__verdict[data-send] {
		color: var(--text-2);
	}
	.pca__none {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.pca__failed {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-xs);
		color: var(--status-warning-fg);
		overflow-wrap: anywhere;
	}
	.pca__rows {
		display: flex;
		flex-direction: column;
	}
	.pca__row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 96px 4ch;
		align-items: center;
		gap: var(--space-2);
		height: 24px;
	}
	.pca__q {
		font-size: var(--text-sm);
		color: var(--text-2);
		cursor: help;
		overflow-wrap: anywhere;
	}
	.pca__v {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		text-align: right;
		color: var(--text-1);
	}
	.pca__why {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
</style>
