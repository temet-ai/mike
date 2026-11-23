<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import * as TooltipUI from '$lib/components/ui/tooltip';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import type { DecisionRow } from '$lib/api/knowledgeConsolidation';
	import CarrierRef from './CarrierRef.svelte';
	import SubjectLink from './SubjectLink.svelte';
	import {
		APPLICATIONS,
		PROPOSED_TOOLTIP,
		VERDICTS,
		actionFor,
		formatClock,
		formatDayMonth,
		formatFull,
		isToday
	} from './labels';

	interface Props {
		row: DecisionRow;
		project: string;
		/** Lists the evidence the judge read as full rows instead of a tooltip. */
		showEvidence?: boolean;
		/** Hides the subject link, on the page of that subject. */
		hideSubject?: boolean;
		busy?: boolean;
		disabled?: boolean;
		error?: string | null;
		onAction: (kind: 'undo' | 'apply') => void;
	}

	const {
		row,
		project,
		showEvidence = false,
		hideSubject = false,
		busy = false,
		disabled = false,
		error = null,
		onAction
	}: Props = $props();

	const verdict = $derived(VERDICTS[row.verdict]);
	const application = $derived(APPLICATIONS[row.application]);
	const action = $derived(actionFor(row));
	const hasAfter = $derived(row.text_after !== null && row.text_after !== '');
	const plain = $derived(
		row.verdict === 'kept' || row.verdict === 'misgrouped' || row.verdict === 'conflict_open'
	);
	const withCounterpart = $derived(
		row.verdict === 'covered' || row.verdict === 'superseded' || row.verdict === 'merged'
	);
	const today = $derived(isToday(row.created_at));
	const evidenceCount = $derived(row.evidence.length);
</script>

<div class="chg" data-slot="change-row" data-verdict={row.verdict}>
	<div class="chg__grid">
		<div class="chg__time">
			<Tooltip text={formatFull(row.created_at)}>
				<span>{formatClock(row.created_at)}</span>
			</Tooltip>
			{#if !today}<span class="chg__day">{formatDayMonth(row.created_at)}</span>{/if}
		</div>

		<div class="chg__verdict">
			<Tooltip text={verdict.tooltip}><span class="machine">{verdict.word}</span></Tooltip>
		</div>

		<div class="chg__body">
			<div class="chg__who">
				{#if !hideSubject}
					<SubjectLink id={row.subject_id} title={row.subject_title} {project} />
				{/if}
				<span class="chg__carriers">
					<CarrierRef carrier={row.carrier} reference={row.carrier_reference} />
					{#if withCounterpart && (row.counterpart || row.counterpart_reference)}
						<span class="chg__by">· by</span>
						<CarrierRef carrier={row.counterpart} reference={row.counterpart_reference} />
					{/if}
				</span>
			</div>

			{#if plain}
				<p class="chg__text">{row.text_before}</p>
			{:else if hasAfter}
				<div class="chg__block">
					<span class="machine chg__label">After</span>
					<p class="chg__text">{row.text_after}</p>
				</div>
				<div class="chg__spine">
					<span class="machine chg__label">Before</span>
					<p class="chg__text chg__text--old">{row.text_before}</p>
				</div>
			{:else}
				<div class="chg__spine">
					<span class="machine chg__label">Retired</span>
					<p class="chg__text chg__text--old">{row.text_before}</p>
				</div>
			{/if}

			<p class="chg__why">
				<span class="machine chg__label">Why</span>
				<span class="chg__reason">{row.reason}</span>
			</p>
			{#if row.application === 'failed' && row.error}
				<p class="chg__failed">Not written: {row.error}</p>
			{/if}

			<p class="chg__meta">
				{row.model} ·
				{#if showEvidence || evidenceCount === 0}
					read {evidenceCount} {evidenceCount === 1 ? 'item' : 'items'}
				{:else}
					<TooltipUI.Root>
						<TooltipUI.Trigger class="chg__read">
							read {evidenceCount}
							{evidenceCount === 1 ? 'item' : 'items'}
						</TooltipUI.Trigger>
						<TooltipUI.Content side="top">
							<ul class="chg__ev-tip">
								{#each row.evidence as item, i (i)}
									<li>
										{item.label}{item.at ? ` · ${formatFull(item.at)}` : ''}{item.operator
											? ' · your words'
											: ''}{item.note ? ` · ${item.note}` : ''}
									</li>
								{/each}
							</ul>
						</TooltipUI.Content>
					</TooltipUI.Root>
				{/if}
			</p>
			{#if showEvidence && evidenceCount > 0}
				<ul class="chg__evidence" aria-label="What the judge read">
					{#each row.evidence as item, i (i)}
						<li class="chg__ev">
							<span class="chg__ev-head">
								<span class="chg__ev-label">{item.label}</span>
								{#if item.at}<span> · {formatFull(item.at)}</span>{/if}
								{#if item.operator}<span class="machine"> · your words</span>{/if}
							</span>
							{#if item.note}<span class="chg__ev-note">{item.note}</span>{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</div>

		<div class="chg__app">
			{#if row.application === 'proposed'}
				<Tooltip text={PROPOSED_TOOLTIP}>
					<StatusMark icon={application.icon} tone={application.tone} label={application.word} />
				</Tooltip>
			{:else}
				<StatusMark icon={application.icon} tone={application.tone} label={application.word} />
			{/if}
			{#if row.applied_at}<span class="chg__applied">{formatClock(row.applied_at)}</span>{/if}
		</div>

		<div class="chg__action">
			{#if action}
				<Tooltip text={action.tooltip}>
					{#snippet child({ props })}
						<Button
							{...props}
							aria-label={action.label}
							variant={action.variant}
							size="sm"
							disabled={disabled || busy}
							onclick={() => onAction(action.kind)}
						>
							{action.label}
						</Button>
					{/snippet}
				</Tooltip>
			{/if}
		</div>
	</div>
	{#if error}<p class="chg__error" role="alert">{error}</p>{/if}
</div>

<style>
	.chg {
		container-type: inline-size;
		padding: var(--space-3) var(--space-4);
	}
	.chg:hover {
		background: var(--interactive-hover-surface);
	}
	.chg__grid {
		display: grid;
		grid-template-columns: 5.5rem 6.5rem minmax(0, 1fr) 7.5rem 8.5rem;
		grid-template-areas: 'time verdict body app action';
		column-gap: var(--space-3);
		align-items: start;
	}
	@container (max-width: 51.99rem) {
		.chg__grid {
			grid-template-columns: auto auto minmax(0, 1fr) auto;
			grid-template-areas:
				'time verdict app action'
				'body body body body';
			row-gap: var(--space-2);
		}
	}
	.chg__time {
		grid-area: time;
		display: flex;
		flex-direction: column;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.chg__verdict {
		grid-area: verdict;
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.chg__body {
		grid-area: body;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.chg__app {
		grid-area: app;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-xs);
	}
	.chg__applied {
		color: var(--text-3);
		font-size: var(--text-xs);
	}
	.chg__action {
		grid-area: action;
		display: flex;
		justify-content: flex-end;
	}
	.chg__who {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-2);
	}
	.chg__carriers {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.chg__by {
		color: var(--text-2);
	}
	.chg__block,
	.chg__spine {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.chg__spine {
		padding-left: var(--space-3);
		border-left: var(--hairline) solid var(--line);
	}
	.chg__label {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.chg__text {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.chg__text--old {
		color: var(--text-3);
	}
	.chg__why {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: var(--space-2);
		margin: 0;
	}
	.chg__reason {
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.chg__failed,
	.chg__error {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
	.chg__error {
		margin-top: var(--space-2);
	}
	.chg__meta {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.chg__meta :global(.chg__read) {
		font: inherit;
		color: inherit;
		background: transparent;
		border: 0;
		padding: 0;
		text-decoration: underline dotted;
		cursor: help;
	}
	.chg__ev-tip {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.chg__evidence {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.chg__ev {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding-left: var(--space-3);
		border-left: var(--hairline) solid var(--line);
	}
	.chg__ev-head {
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.chg__ev-label {
		font-family: var(--font-mono, monospace);
		color: var(--text-2);
	}
	.chg__ev-note {
		font-size: var(--text-sm);
		color: var(--text-2);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
