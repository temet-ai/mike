<!--
@component
One lesson candidate as a compact band of the Lessons list: the rule in full (click it to expand),
Approve and Reject beside it, and a source line under it. Expanding reveals what you asked and your
correction, both in full, plus links to the full lesson and the turn. A decided lesson stays where
it is and shows "Approved" or "Rejected" in the footprint the buttons held.
-->
<script module lang="ts">
	export type CandidateState = 'waiting' | 'approved' | 'rejected';
</script>

<script lang="ts">
	import { ArrowUpRight } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import EvalQuote from '$lib/components/eval/EvalQuote.svelte';
	import { formatDay } from '$lib/components/eval/evalReview';
	import { disclose } from '$lib/components/ui/motion';
	import type { LessonCandidate, LessonKind, RejectReason } from '$lib/api/improvement';

	interface Props {
		candidate: LessonCandidate;
		detailHref: string;
		showSession?: boolean;
		decision?: CandidateState;
		busy: boolean;
		onApprove: () => Promise<void>;
		onReject: (reason: RejectReason, note?: string) => Promise<void>;
	}

	const {
		candidate,
		detailHref,
		showSession = true,
		decision = 'waiting',
		busy,
		onApprove,
		onReject
	}: Props = $props();

	const KIND_WORD: Record<LessonKind, string> = {
		preference: 'Preference',
		correction: 'Correction',
		warning: 'Warning',
		convention: 'Convention'
	};

	const REASONS: { reason: RejectReason; text: string }[] = [
		{ reason: 'not_a_correction', text: "This wasn't something I was correcting" },
		{ reason: 'too_specific', text: 'Too specific to be a general rule' },
		{ reason: 'wrong', text: 'Just wrong' },
		{ reason: 'other', text: 'Something else' }
	];

	const bodyId = $derived(`lcb-body-${candidate.id}`);
	const day = $derived(formatDay(candidate.createdAt) ?? candidate.createdAt);
	const earlier = $derived(candidate.evidenceCount - 1);

	let open = $state(false);
	let rejecting = $state(false);
	let otherNote = $state('');
	let showOtherNote = $state(false);
	let saving = $state<'approve' | RejectReason | null>(null);
	let rowError = $state<string | null>(null);

	const locked = $derived(busy || saving !== null);

	function toggle(): void {
		open = !open;
	}

	function toggleReject(): void {
		rejecting = !rejecting;
		if (rejecting) open = true;
		else showOtherNote = false;
	}

	async function approve(): Promise<void> {
		saving = 'approve';
		rowError = null;
		try {
			await onApprove();
			open = false;
			rejecting = false;
		} catch (e) {
			rowError = e instanceof Error ? e.message : String(e);
		} finally {
			saving = null;
		}
	}

	async function reject(reason: RejectReason): Promise<void> {
		if (reason === 'other' && !showOtherNote) {
			showOtherNote = true;
			return;
		}
		saving = reason;
		rowError = null;
		try {
			await onReject(reason, reason === 'other' ? otherNote : undefined);
			rejecting = false;
			showOtherNote = false;
			otherNote = '';
			open = false;
		} catch (e) {
			rowError = e instanceof Error ? e.message : String(e);
		} finally {
			saving = null;
		}
	}
</script>

<article
	class="lcb"
	data-slot="lesson-candidate-card"
	data-state={decision}
	aria-label="Lesson waiting on you"
>
	<div class="band-hd-wrap lcb-hd">
		<button
			type="button"
			class="band-hd band-hd--top"
			aria-expanded={open}
			aria-controls={bodyId}
			onclick={toggle}
		>
			<DisclosureMarker {open} size={10} />
			<span class="lcb-rule">{candidate.text}</span>
		</button>
		<div class="lcb-act">
			{#if decision === 'waiting'}
				<Button
					size="sm"
					variant="default"
					disabled={locked}
					aria-busy={saving === 'approve'}
					onclick={() => void approve()}>Approve</Button
				>
				<Button
					size="sm"
					variant="outline"
					disabled={locked}
					aria-expanded={rejecting}
					aria-controls={bodyId}
					onclick={toggleReject}>Reject</Button
				>
			{:else}
				<span class="lcb-done" data-state={decision}
					>{decision === 'approved' ? 'Approved' : 'Rejected'}</span
				>
			{/if}
		</div>
	</div>
	<p class="lcb-src">
		<span class="lcb-kind">{KIND_WORD[candidate.kind]}</span>
		{#if candidate.source === 'directive'}
			<span>· from a directive you approved</span>
		{:else if showSession}
			<span
				>· from your reply in <a class="lcb-link" href={candidate.sessionHref}
					>{candidate.sessionTitle}</a
				></span
			>
		{:else}
			<span>· from your reply</span>
		{/if}
		<span>· {day}</span>
		<span class="lcb-count"
			>{candidate.evidenceCount}
			{candidate.evidenceCount === 1 ? 'correction' : 'corrections'}</span
		>
	</p>
	{#if open}
		<div class="lcb-body" id={bodyId} transition:disclose>
			<div class="band-body lcb-inner">
				{#if rejecting && decision === 'waiting'}
					<fieldset class="lcb-reasons">
						<legend class="lcb-legend">Why reject it?</legend>
						<div class="lcb-reason-row">
							{#each REASONS as r (r.reason)}
								<Button
									size="sm"
									variant="outline"
									disabled={locked}
									aria-busy={saving === r.reason}
									aria-expanded={r.reason === 'other' ? showOtherNote : undefined}
									onclick={() => void reject(r.reason)}>{r.text}</Button
								>
							{/each}
						</div>
						{#if showOtherNote}
							<div class="lcb-reason-row">
								<input
									class="lcb-note"
									type="text"
									placeholder="Say why (optional)"
									aria-label="Why you're rejecting this lesson"
									bind:value={otherNote}
								/>
								<Button
									size="sm"
									variant="default"
									disabled={locked}
									aria-busy={saving === 'other'}
									onclick={() => void reject('other')}>Reject</Button
								>
							</div>
						{/if}
					</fieldset>
				{/if}
				{#if candidate.askText !== ''}
					<EvalQuote who="you" heading="You asked">
						<p class="lcb-quote">{candidate.askText}</p>
					</EvalQuote>
				{/if}
				<EvalQuote
					who="you"
					key
					heading={earlier > 0 ? 'Your latest correction' : 'Your correction'}
				>
					<p class="lcb-quote">{candidate.correctingQuote}</p>
				</EvalQuote>
				{#if earlier > 0}
					<p class="lcb-earlier">
						{earlier}
						{earlier === 1 ? 'earlier correction backs' : 'earlier corrections back'} this rule — see
						them on the full lesson page.
					</p>
				{/if}
				<div class="lcb-links">
					<a class="lcb-link" href={detailHref}
						>Open the full lesson<ArrowUpRight size={12} aria-hidden="true" /></a
					>
					<a class="lcb-link" href={candidate.turnHref}
						>Open the turn<ArrowUpRight size={12} aria-hidden="true" /></a
					>
				</div>
			</div>
		</div>
	{/if}
	{#if rowError}
		<p class="lcb-error" role="alert">{rowError}</p>
	{/if}
</article>

<style>
	.lcb {
		min-width: 0;
	}
	.lcb-rule {
		font-size: var(--text-md);
		font-weight: var(--weight-medium);
		color: var(--text-1);
		line-height: var(--leading-body);
		overflow-wrap: anywhere;
		text-align: left;
	}
	.lcb[data-state='approved'] .lcb-rule,
	.lcb[data-state='rejected'] .lcb-rule {
		color: var(--text-2);
	}
	.lcb-act {
		display: flex;
		flex: 0 0 auto;
		gap: var(--space-2);
		align-items: center;
		justify-content: flex-end;
		min-width: 11rem;
		padding-right: var(--space-4);
		/* the height of a size="sm" Button (h-7), held when a word replaces the buttons */
		min-height: 1.75rem;
	}
	.lcb-done {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.lcb-done[data-state='approved'] {
		color: var(--status-success-fg);
	}
	.lcb-src {
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-2);
		margin: 0;
		padding: 0 var(--space-4) var(--space-3) calc(var(--space-4) + 10px + var(--space-2));
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.lcb-count {
		margin-left: auto;
		font-size: var(--text-caption);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
	.lcb-body {
		display: grid;
	}
	.lcb-inner {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-height: 0;
		overflow: hidden;
		padding-left: calc(var(--space-4) + 10px + var(--space-2));
	}
	.lcb-reasons {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: 0;
	}
	.lcb-legend {
		padding: 0;
		margin-bottom: var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.lcb-reason-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.lcb-note {
		min-width: 14rem;
		padding: var(--space-1) var(--space-2);
		border: var(--hairline) solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-xs);
	}
	.lcb-note:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.lcb-quote {
		margin: 0;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.lcb-earlier {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.lcb-links {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		font-size: var(--text-sm);
	}
	.lcb-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--interactive-active-fg);
		text-underline-offset: var(--space-1);
	}
	.lcb-src .lcb-link {
		display: inline;
	}
	.lcb-link:hover {
		text-decoration: underline;
	}
	.lcb-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.lcb-error {
		margin: 0;
		padding: 0 var(--space-4) var(--space-3) calc(var(--space-4) + 10px + var(--space-2));
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
	}
	@container (max-width: 719.98px) {
		.lcb-hd {
			flex-wrap: wrap;
		}
		.lcb-act {
			flex-basis: 100%;
			justify-content: flex-start;
			min-width: 0;
			padding: 0 var(--space-4) var(--space-2) calc(var(--space-4) + 10px + var(--space-2));
		}
		.lcb-count {
			margin-left: 0;
		}
	}
</style>
