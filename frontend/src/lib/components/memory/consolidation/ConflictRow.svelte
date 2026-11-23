<script lang="ts">
	import { CircleCheck, TriangleAlert } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import type { DecisionRow } from '$lib/api/knowledgeConsolidation';
	import CarrierRef from './CarrierRef.svelte';
	import SubjectLink from './SubjectLink.svelte';
	import { KEEP_TOOLTIP, formatDayMonth, formatFull, settlesLine } from './labels';

	interface Props {
		row: DecisionRow;
		project: string;
		/** Set once settled; the string names the carrier that was kept, null when settled elsewhere. */
		settled?: string | null | undefined;
		/** Any action in flight disables the buttons; `busy` marks this row's own. */
		busy?: boolean;
		disabled?: boolean;
		error?: string | null;
		onKeep: (statementId: number) => void;
	}

	const {
		row,
		project,
		settled,
		busy = false,
		disabled = false,
		error = null,
		onKeep
	}: Props = $props();

	const isSettled = $derived(settled !== undefined);
	const sides = $derived([
		{
			statementId: row.statement_id,
			text: row.text_before,
			carrier: row.carrier,
			reference: row.carrier_reference
		},
		{
			statementId: row.counterpart_statement_id,
			text: row.counterpart_text ?? '',
			carrier: row.counterpart,
			reference: row.counterpart_reference
		}
	]);
</script>

<div class="cr" data-slot="conflict-row" data-settled={isSettled ? '' : undefined}>
	<div class="cr__head">
		{#if isSettled}
			<StatusMark icon={CircleCheck} tone="quiet" label="Settled" />
		{:else}
			<StatusMark icon={TriangleAlert} tone="warning" label="Conflict" />
		{/if}
		<SubjectLink id={row.subject_id} title={row.subject_title} {project} />
		<Tooltip text={formatFull(row.created_at)}>
			<span class="cr__found">found {formatDayMonth(row.created_at)}</span>
		</Tooltip>
	</div>

	<div class="cr__pair">
		{#each sides as side, i (i)}
			<div class="cr__cell">
				<p class="cr__text">{side.text}</p>
				<span class="cr__meta">
					<CarrierRef carrier={side.carrier} reference={side.reference} />
					{#if side.carrier?.stated_at}
						<span> · {formatDayMonth(side.carrier.stated_at)}</span>
					{/if}
				</span>
				{#if !isSettled && side.statementId !== null}
					<span class="cr__keep">
						<Tooltip text={KEEP_TOOLTIP}>
							{#snippet child({ props })}
								<Button
									{...props}
									aria-label="Keep this one"
									variant="outline"
									size="sm"
									disabled={disabled || busy}
									onclick={() => onKeep(side.statementId!)}
								>
									Keep this one
								</Button>
							{/snippet}
						</Tooltip>
					</span>
				{/if}
			</div>
		{/each}
	</div>

	<p class="cr__line">
		<span class="machine cr__tag">Why</span>
		<span class="cr__reason">{row.reason}</span>
	</p>
	<p class="cr__settles">
		{#if typeof settled === 'string'}
			Kept {settled}. The other is superseded.
		{:else if !isSettled}
			{settlesLine(row)}
		{/if}
	</p>
	{#if error}<p class="cr__error" role="alert">{error}</p>{/if}
</div>

<style>
	.cr {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		container-type: inline-size;
	}
	.cr:hover {
		background: var(--interactive-hover-surface);
	}
	.cr__head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.cr__found {
		margin-left: auto;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.cr__pair {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--hairline);
		background: var(--line);
	}
	@container (max-width: 40rem) {
		.cr__pair {
			grid-template-columns: 1fr;
		}
	}
	.cr__cell {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		background: var(--surface-2);
		min-width: 0;
	}
	.cr__text {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.cr__meta {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.cr__keep {
		display: flex;
	}
	.cr__line {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin: 0;
	}
	.cr__tag {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.cr__reason {
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.cr__settles {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.cr__settles:empty {
		display: none;
	}
	.cr__error {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
