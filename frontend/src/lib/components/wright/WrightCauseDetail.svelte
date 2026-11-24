<!--
@component
What an opened cause adds to its row: where it belongs, how its score is made, whether the engineer
will take it and why, every open problem it explains, and the causes merged into it. Separate sits on
the merges the operator made; one the engineer made says so and has no button. Separate asks to
confirm first.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { AbsorbedCause, WrightCause, WrightProblem } from '$lib/api/wright';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';
	import type { Feed } from '$lib/wright/feed';
	import { causeArea, engineerLine, problemsText, scoreBreakdown } from '$lib/wright/causes';
	import { dayAndTime } from '$lib/wright/present';
	import WrightNote from './WrightNote.svelte';
	import WrightProblemRow from './WrightProblemRow.svelte';
	import WrightSeparateDialog from './WrightSeparateDialog.svelte';

	interface Props {
		cause: WrightCause;
		/** `wright.followup_limits.max_attempts`, for "used all 3 of its attempts". */
		maxAttempts: number | null;
		/** The cause's open problems; undefined before the first read. */
		problems?: Feed<WrightProblem>;
		/** Why Separate is off (this browser is not paired); null when it is on. */
		disabledReason?: string | null;
		busy?: boolean;
		/** The refusal of the last queue action, shown in the Separate dialog. */
		error?: string | null;
		onRetry?: () => void;
		/** The Separate dialog opens: the page drops the last refusal so it is not shown there. */
		onClearError?: () => void;
		onShowHeld?: () => void;
		/** Called when one problem row opens, so the page can load its detail. */
		onOpenProblem?: (id: number) => void;
		/** Separates one merge; true when it worked. */
		onSeparate: (mergeId: number) => Promise<boolean>;
		/** The opened problem's own detail. */
		problem?: Snippet<[WrightProblem]>;
	}

	const {
		cause,
		maxAttempts,
		problems,
		disabledReason = null,
		busy = false,
		error = null,
		onRetry,
		onClearError,
		onShowHeld,
		onOpenProblem,
		onSeparate,
		problem
	}: Props = $props();

	const area = $derived(causeArea(cause));
	const score = $derived(scoreBreakdown(cause));
	const engineer = $derived(engineerLine(cause, maxAttempts));

	let separating = $state<AbsorbedCause | null>(null);
	let separateOpen = $state(false);

	function askSeparate(merge: AbsorbedCause): void {
		onClearError?.();
		separating = merge;
		separateOpen = true;
	}
</script>

<div class="cd" data-slot="wright-cause-detail">
	<div class="cd__section">
		<h3 class="machine cd__h">Area</h3>
		<p class="cd__line">{area.label}</p>
		{#if area.detail.length > 0}
			<p class="cd__note">{area.detail.join(', ')}</p>
		{/if}
	</div>

	<div class="cd__section">
		<h3 class="machine cd__h">How it is ranked</h3>
		<dl class="cd__score">
			{#each score as line (line.label)}
				<dt>{line.label}</dt>
				<dd>{line.value}</dd>
			{/each}
		</dl>
		{#if engineer !== null}
			<p class="cd__line" data-slot="wright-engineer-line">{engineer}</p>
		{/if}
	</div>

	{#if cause.absorbed.length > 0}
		<div class="cd__section">
			<h3 class="machine cd__h">Merged into this cause</h3>
			<ul class="cd__merges" role="list">
				{#each cause.absorbed as merge (merge.merge_id)}
					<li class="cd__merge">
						<span class="cd__merge-text">
							<span class="cd__line">{merge.check_name}</span>
							<span class="cd__note">{merge.key}</span>
							<span class="cd__note">
								{problemsText(merge.problems)} · merged by {merge.by === 'operator'
									? 'you'
									: "Wright's engineer"}
								{dayAndTime(merge.created_at)}
							</span>
						</span>
						<span class="cd__merge-slot">
							{#if merge.can_separate}
								<Button
									variant="outline"
									size="sm"
									aria-label="Separate {merge.check_name}"
									disabled={disabledReason !== null || busy}
									title={disabledReason ?? undefined}
									onclick={() => askSeparate(merge)}
								>
									Separate
								</Button>
							{/if}
						</span>
					</li>
				{/each}
			</ul>
		</div>
	{/if}

	<div class="cd__section">
		<div class="cd__title-row">
			<h3 class="machine cd__h">Open problems</h3>
			<span class="cd__held">
				{#if problems !== undefined && problems.held.length > 0 && onShowHeld}
					<Button variant="ghost" size="sm" onclick={onShowHeld}>{problems.held.length} new</Button>
				{/if}
			</span>
		</div>
		{#if problems === undefined || problems.status === 'loading'}
			<WrightNote>Loading.</WrightNote>
		{:else if problems.status === 'missing'}
			<WrightNote>Not in the running mike yet.</WrightNote>
		{:else if problems.status === 'error'}
			<LoadErrorBanner
				title="Could not load the problems"
				detail={problems.error ?? ''}
				{onRetry}
			/>
		{:else if problems.rows.length === 0}
			<WrightNote>No open problems.</WrightNote>
		{:else}
			<ul class="cd__problems" role="list" aria-label="Open problems of cause {cause.cause_id}">
				{#each problems.rows as row (row.id)}
					<WrightProblemRow problem={row} onOpen={onOpenProblem}>
						{@render problem?.(row)}
					</WrightProblemRow>
				{/each}
			</ul>
		{/if}
	</div>
</div>

<WrightSeparateDialog
	bind:open={separateOpen}
	merge={separating}
	keptBy={cause.check_name}
	{busy}
	{error}
	onConfirm={onSeparate}
/>

<style>
	.cd {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.cd__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.cd__h {
		margin: 0;
		font-size: var(--text-micro);
		color: var(--text-3);
	}

	.cd__line {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.cd__note {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.cd__score {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: var(--space-1) var(--space-4);
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
	}

	.cd__score dt {
		color: var(--text-3);
	}

	.cd__score dd {
		margin: 0;
		color: var(--text-1);
	}

	.cd__merges,
	.cd__problems {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.cd__merge {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		padding-block: var(--space-2);
	}

	.cd__merge:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.cd__merge-text {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		flex: 1;
		min-width: 0;
	}

	/* Keeps the button's width on a merge that has none, so the rows line up. */
	.cd__merge-slot {
		flex: none;
		inline-size: 6rem;
		display: inline-flex;
		justify-content: flex-end;
	}

	.cd__title-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		min-block-size: 1.75rem;
	}

	.cd__held {
		inline-size: 5rem;
		display: inline-flex;
		justify-content: flex-end;
	}
</style>
