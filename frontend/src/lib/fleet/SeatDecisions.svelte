<script lang="ts">
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { ShareBar } from '$lib/components/ui/share-bar';
	import { wholePercents, type SeatDecisionFigures } from './seatDecisions';
	import type { FleetDecisionMode } from './types';

	interface Props {
		seat: SeatDecisionFigures | null;
		modes?: { effort: FleetDecisionMode; wake_check: FleetDecisionMode } | null;
		loading?: boolean;
		error?: string | null;
		onRetry?: () => void;
		unavailableReason?: string | null;
	}

	const {
		seat,
		modes = null,
		loading = false,
		error = null,
		onRetry,
		unavailableReason = null
	}: Props = $props();

	const EFFORT_TIP =
		"Share of this seat's tokens, by what each request was doing. Requests with no label yet are not counted.";
	const MAIL_TIP =
		"Held mail goes out with the seat's next turn. Mail from the operator and replies to the seat's own asks always wake it.";

	const effort = $derived(seat?.effort ?? { work: 0, machinery: 0, waiting: 0 });
	const effortTotal = $derived(effort.work + effort.machinery + effort.waiting);
	const percents = $derived(wholePercents([effort.work, effort.machinery, effort.waiting]));
	const mail = $derived(seat?.mail ?? { held: 0, woken: 0 });
</script>

{#if loading}
	<div class="sd-skeleton" data-motion="wait" aria-hidden="true"></div>
{:else if error !== null}
	<LoadErrorBanner title="Couldn't load decisions" detail={error} {onRetry} />
{:else if unavailableReason !== null}
	<p class="sd-none">{unavailableReason}</p>
{:else}
	<div class="sd" data-slot="seat-decisions">
		<div class="sd-block" data-block="effort">
			<div class="sd-head">
				<Tooltip text={EFFORT_TIP}>
					{#snippet child({ props })}
						<span class="machine sd-label" {...props}>Effort</span>
					{/snippet}
				</Tooltip>
				{#if modes && modes.effort !== 'on'}<span class="sd-mode">{modes.effort}</span>{/if}
			</div>
			{#if effortTotal > 0}
				<ShareBar
					size="split"
					max={effortTotal}
					segments={[
						{ value: effort.work, tone: 'info' },
						{ value: effort.machinery, tone: 'active' }
					]}
				/>
				<p class="sd-line">
					work {percents[0]}% · machinery {percents[1]}% · waiting {percents[2]}%
				</p>
			{:else}
				<div class="sd-empty-bar"></div>
				<p class="sd-line">—</p>
			{/if}
		</div>

		<div class="sd-block" data-block="mail">
			<div class="sd-head">
				<Tooltip text={MAIL_TIP}>
					{#snippet child({ props })}
						<span class="machine sd-label" {...props}>Mail</span>
					{/snippet}
				</Tooltip>
				{#if modes && modes.wake_check !== 'on'}<span class="sd-mode">{modes.wake_check}</span>{/if}
			</div>
			<dl class="sd-kv">
				<dt>held</dt>
				<dd>{mail.held}</dd>
				<dt>woken</dt>
				<dd>{mail.woken}</dd>
			</dl>
		</div>
	</div>
{/if}

<style>
	.sd {
		display: flex;
		flex-direction: column;
	}
	.sd-block {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-2) 0;
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.sd-block:first-child {
		border-top: 0;
		padding-top: 0;
	}
	.sd-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}
	.sd-label {
		font-size: var(--text-xs);
		color: var(--text-3);
		cursor: help;
	}
	.sd-mode {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sd-line {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		color: var(--text-2);
	}
	.sd-empty-bar {
		height: 12px;
		background: var(--surface-2);
	}
	.sd-kv {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: 0 var(--space-3);
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
	}
	.sd-kv dt {
		color: var(--text-3);
	}
	.sd-kv dd {
		margin: 0;
		color: var(--text-1);
	}
	.sd-none {
		margin: 0;
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sd-skeleton {
		height: 64px;
		background: var(--surface-2);
	}
</style>
