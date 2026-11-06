<!--
@component
The summary strip under the "What the judge did" header: what is waiting on you (each count
selects its tab below), what changed in the last 7 days, and the corrected share last week.
Full bleed across the page's scroll container (`100cqi`, past the page's 1600px cap), hairline top
and bottom, cyan-dim wash, as the Sessions orientation strip.
-->
<script lang="ts">
	import type { ImprovementSummary, TrendPoint } from '$lib/api/improvement';
	import { sharePct } from './correctionTrend';
	import type { WaitingTab } from './WaitingForYou.svelte';

	interface Props {
		summary: ImprovementSummary | null;
		summaryError: string | null;
		latest: TrendPoint | null;
		onPick: (tab: WaitingTab) => void;
	}

	const { summary, summaryError, latest, onPick }: Props = $props();

	const waiting = $derived<{ tab: WaitingTab; label: string; count: number }[]>([
		{ tab: 'lessons', label: 'Lessons', count: summary?.lessonCandidateCount ?? 0 },
		{ tab: 'nudges', label: 'Nudges to answer', count: summary?.unlabelledWakeCount ?? 0 },
		{ tab: 'directives', label: 'Directives', count: summary?.openProposalCount ?? 0 }
	]);

	function plural(n: number, one: string, many: string): string {
		return `${n} ${n === 1 ? one : many}`;
	}
</script>

<div class="ims" data-slot="improvement-strip">
	{#if summaryError}
		<p class="ims-error" role="alert">Could not load this week's numbers: {summaryError}</p>
	{:else}
		<div class="ims-group">
			<p class="machine ims-cap" id="ims-waiting-cap">Waiting on you</p>
			<dl class="ims-facts" aria-labelledby="ims-waiting-cap">
				{#each waiting as item (item.tab)}
					<div class="ims-fact ims-fact--pick">
						<dt>{item.label}</dt>
						<dd class="ims-num">
							<button
								type="button"
								class="ims-pick"
								class:ims-pick--live={item.count > 0}
								aria-label={`${item.label}: ${item.count}. Show them`}
								onclick={() => onPick(item.tab)}>{item.count}</button
							>
						</dd>
					</div>
				{/each}
			</dl>
		</div>
		<div class="ims-group ims-group--week">
			<p class="machine ims-cap" id="ims-week-cap">Last 7 days</p>
			<dl class="ims-facts" aria-labelledby="ims-week-cap">
				<div class="ims-fact">
					<dt>Lessons approved</dt>
					<dd class="ims-num">{summary?.week.lessonsApproved ?? 0}</dd>
					<dd class="ims-sub">{summary?.week.lessonsRejected ?? 0} rejected</dd>
				</div>
				<div class="ims-fact">
					<dt>Nudges sent</dt>
					<dd class="ims-num">{(summary?.week.wakes ?? 0) + (summary?.week.appends ?? 0)}</dd>
					<dd class="ims-sub">
						{plural(summary?.week.wakes ?? 0, 'wake', 'wakes')} · {plural(
							summary?.week.appends ?? 0,
							'note',
							'notes'
						)}
					</dd>
				</div>
				<div class="ims-fact">
					<dt>Corrected turns, last week</dt>
					<dd class="ims-num">{latest ? sharePct(latest) : '—'}</dd>
					<dd class="ims-sub">
						{#if latest}{`of ${latest.n} judged turns · `}{/if}<a
							class="ims-link"
							href="#imp-better">See the trend</a
						>
					</dd>
				</div>
			</dl>
		</div>
	{/if}
</div>

<style>
	.ims {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4) var(--space-7);
		box-sizing: border-box;
		width: 100cqi;
		max-width: none;
		margin-inline: calc(-1 * var(--space-6)) 0;
		padding: var(--space-4) var(--space-6);
		border-block: var(--hairline) solid var(--border-subtle);
		background: linear-gradient(100deg, var(--funk-cyan-dim), transparent 65%);
	}
	.ims-group {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.ims-group--week {
		padding-left: var(--space-7);
		border-left: var(--hairline) solid var(--border-subtle);
	}
	.ims-cap {
		margin: 0;
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.ims-facts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-6);
		margin: 0;
	}
	.ims-fact {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.ims-fact dt {
		order: 2;
		color: var(--text-3);
		font-size: var(--text-xs);
	}
	.ims-num {
		order: 1;
		margin: 0;
		color: var(--text-1);
		font: var(--weight-display) var(--text-xl) / var(--leading-tight) var(--font-display);
		font-variant-numeric: tabular-nums;
	}
	.ims-sub {
		order: 3;
		margin: 0;
		color: var(--text-3);
		font-size: var(--text-xs);
	}
	.ims-pick {
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}
	.ims-pick::after {
		content: '';
		position: absolute;
		inset: 0;
	}
	.ims-pick--live {
		color: var(--signal-active);
	}
	.ims-fact--pick:hover .ims-pick {
		color: var(--text-strong);
	}
	.ims-pick:focus-visible {
		outline: none;
	}
	.ims-pick:focus-visible::after {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ims-link {
		color: var(--interactive-active-fg);
		text-underline-offset: var(--space-1);
	}
	.ims-link:hover {
		text-decoration: underline;
	}
	.ims-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ims-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	@container (max-width: 719.98px) {
		.ims {
			flex-direction: column;
			margin-inline: calc(-1 * var(--space-3)) 0;
			padding-inline: var(--space-3);
		}
		.ims-group--week {
			padding-left: 0;
			padding-top: var(--space-3);
			border-left: 0;
			border-top: var(--hairline) solid var(--border-subtle);
		}
	}
</style>
