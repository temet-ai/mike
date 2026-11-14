<script lang="ts">
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { ShareBar } from '$lib/components/ui/share-bar';
	import { wholePercents, type SeatDecisionEntry, type SeatDecisionFigures } from './seatDecisions';
	import type { FleetDecisionModes } from './types';

	interface Props {
		seat: SeatDecisionFigures | null;
		modes?: Partial<FleetDecisionModes> | null;
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
	const STOP_TIP =
		"Each time this seat tried to stop: whether the stop went through or was pushed back once. The score is Jev's, when it was asked.";
	const TRIAGE_TIP =
		'The retry, fix-code or fix-environment line printed in the receipt of a failed check this seat asked for.';
	const COMMIT_TIP =
		'Whether Jev warned that a fleet commit message did not match its diff. It warns and never refuses.';

	const effort = $derived(seat?.effort ?? { work: 0, machinery: 0, waiting: 0 });
	const effortTotal = $derived(effort.work + effort.machinery + effort.waiting);
	const percents = $derived(wholePercents([effort.work, effort.machinery, effort.waiting]));
	const mail = $derived(seat?.mail ?? { held: 0, woken: 0 });
	const stopChecks = $derived(seat?.stopChecks ?? []);
	const triage = $derived(seat?.triage ?? []);
	const commitMessages = $derived(seat?.commitMessages ?? []);

	function clock(iso: string | null): string {
		if (iso === null) return '—';
		const parsed = new Date(iso);
		if (Number.isNaN(parsed.getTime())) return '—';
		return parsed.toLocaleTimeString(undefined, {
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			hour12: false
		});
	}

	function scoreText(entry: SeatDecisionEntry): string {
		return entry.score === null ? '' : entry.score.toFixed(2);
	}

	function modeWord(key: keyof FleetDecisionModes): string | null {
		const mode = modes?.[key];
		return mode !== undefined && mode !== 'on' ? mode : null;
	}
</script>

{#snippet head(label: string, tip: string, mode: string | null, count?: number)}
	<div class="sd-head">
		<Tooltip text={tip}>
			{#snippet child({ props })}
				<span class="machine sd-label" {...props}
					>{label}{count === undefined ? '' : ` ${count}`}</span
				>
			{/snippet}
		</Tooltip>
		{#if mode !== null}<span class="sd-mode">{mode}</span>{/if}
	</div>
{/snippet}

{#snippet decisionList(entries: SeatDecisionEntry[], name: string)}
	<!-- A scrolling list must be reachable from the keyboard. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="sd-list" role="list" aria-label={name} tabindex="0" data-list={name}>
		{#each entries as entry (entry.id)}
			<div class="sd-entry" role="listitem">
				<span class="sd-time">{clock(entry.at)}</span>
				<span class="sd-result" data-pushed={entry.result === 'pushed back' ? '' : undefined}
					>{entry.result}</span
				>
				<span class="sd-score">{scoreText(entry)}</span>
			</div>
		{:else}
			<div class="sd-entry sd-entry--none" role="listitem">—</div>
		{/each}
	</div>
{/snippet}

{#if loading}
	<div class="sd-skeleton" data-motion="wait" aria-hidden="true"></div>
{:else if error !== null}
	<LoadErrorBanner title="Couldn't load decisions" detail={error} {onRetry} />
{:else if unavailableReason !== null}
	<p class="sd-none">{unavailableReason}</p>
{:else}
	<div class="sd" data-slot="seat-decisions">
		<div class="sd-block" data-block="effort">
			{@render head('Effort', EFFORT_TIP, modeWord('effort'))}
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
			{@render head('Mail', MAIL_TIP, modeWord('wake_check'))}
			<dl class="sd-kv">
				<dt>held</dt>
				<dd>{mail.held}</dd>
				<dt>woken</dt>
				<dd>{mail.woken}</dd>
			</dl>
		</div>

		<div class="sd-block" data-block="stop-checks">
			{@render head('Stop checks', STOP_TIP, modeWord('stop_check'), stopChecks.length)}
			{@render decisionList(stopChecks, 'Stop checks')}
		</div>

		<div class="sd-block" data-block="triage">
			{@render head('Triage', TRIAGE_TIP, modeWord('triage'), triage.length)}
			<!-- A scrolling list must be reachable from the keyboard. -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div class="sd-list" role="list" aria-label="Triage" tabindex="0" data-list="Triage">
				{#each triage as entry (entry.id)}
					<div class="sd-entry sd-entry--triage" role="listitem">
						<span class="sd-time">{clock(entry.at)}</span>
						<span class="sd-result">{entry.exitCode === null ? '—' : `exit ${entry.exitCode}`}</span
						>
						<span class="sd-text">{entry.line}</span>
					</div>
				{:else}
					<div class="sd-entry sd-entry--none" role="listitem">—</div>
				{/each}
			</div>
		</div>

		<div class="sd-block" data-block="commit-messages">
			{@render head(
				'Commit messages',
				COMMIT_TIP,
				modeWord('commit_message'),
				commitMessages.length
			)}
			{@render decisionList(commitMessages, 'Commit messages')}
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
	.sd-list {
		display: flex;
		flex-direction: column;
		max-height: calc(8 * 20px);
		overflow-y: auto;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
	}
	.sd-list:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.sd-entry {
		display: grid;
		grid-template-columns: 8ch minmax(0, 1fr) 4ch;
		gap: var(--space-2);
		min-height: 20px;
		align-items: baseline;
		color: var(--text-2);
	}
	.sd-entry--triage {
		grid-template-columns: 8ch 7ch minmax(0, 1fr);
	}
	.sd-entry--none {
		display: block;
		color: var(--text-3);
	}
	.sd-time {
		color: var(--text-3);
	}
	.sd-result[data-pushed] {
		color: var(--text-1);
	}
	.sd-score {
		text-align: right;
		color: var(--text-3);
	}
	.sd-text {
		overflow-wrap: anywhere;
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
