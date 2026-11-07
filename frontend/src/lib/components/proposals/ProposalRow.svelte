<!--
@component
One lesson mike placed as a rule or an agent statement: the lesson and the prompt it came from, why
it was placed there, the file it belongs in and the text it would add, and the user's decision.
Every button is always present and is disabled when it does not apply, so the row never moves.
-->
<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import { Button } from '$lib/components/ui/button';
	import { formatDay } from '$lib/components/eval/evalReview';
	import type { Proposal } from '$lib/api/proposals';
	import { proposalOutcome, proposalTarget } from './proposalTarget';

	let {
		proposal,
		busy = false,
		error = null,
		onDecide
	}: {
		proposal: Proposal;
		busy?: boolean;
		error?: string | null;
		onDecide: (decision: 'accept' | 'decline' | 'clear') => void;
	} = $props();

	const target = $derived(proposalTarget(proposal));
	const kindWord = $derived(proposal.placement === 'rule' ? 'Rule' : 'Agent');
	const stateText = $derived(
		proposal.decision === 'accepted'
			? 'Accepted'
			: proposal.decision === 'declined'
				? 'Declined'
				: 'Waiting on you'
	);
	const stateVariant = $derived(
		proposal.decision === 'accepted'
			? 'tag-green'
			: proposal.decision === 'declined'
				? 'chip-muted'
				: 'chip-warning'
	);
	const learned = $derived(formatDay(proposal.created_at) ?? proposal.created_at);
</script>

<article class="pr" data-slot="proposal-row" data-decision={proposal.decision ?? 'none'}>
	<header class="pr-head">
		<Badge variant="tag-default">{kindWord}</Badge>
		<h3 class="pr-title">
			Lesson {proposal.lesson_id}{proposal.topic ? `, ${proposal.topic}` : ''}
		</h3>
		<Badge variant={stateVariant}>{stateText}</Badge>
	</header>

	<p class="pr-text">{proposal.text}</p>

	<dl class="pr-facts">
		<div>
			<dt>Learned</dt>
			<dd>{learned}, a {proposal.kind}</dd>
		</div>
		{#if proposal.agent_name}
			<div>
				<dt>Agent</dt>
				<dd>{proposal.agent_name}</dd>
			</div>
		{/if}
		{#if proposal.reason}
			<div class="pr-wide">
				<dt>Why it belongs in a {proposal.placement}</dt>
				<dd>{proposal.reason}</dd>
			</div>
		{/if}
		{#if proposal.user_prompt}
			<div class="pr-wide">
				<dt>What you said when it was learned</dt>
				<dd>{proposal.user_prompt}</dd>
			</div>
		{/if}
	</dl>

	<section class="pr-change" aria-label={`Proposed change to ${target.path}`}>
		<p class="pr-target">
			{target.creates ? 'Creates' : 'Adds to'} <code>{target.path}</code>
		</p>
		<pre class="pr-diff">{target.change}</pre>
	</section>

	<div class="pr-buttons" role="group" aria-label={`Your decision on lesson ${proposal.lesson_id}`}>
		<Button
			variant="default"
			disabled={busy || proposal.decision === 'accepted'}
			onclick={() => onDecide('accept')}>Accept</Button
		>
		<Button
			variant="outline"
			disabled={busy || proposal.decision === 'declined'}
			onclick={() => onDecide('decline')}>Decline</Button
		>
		<Button
			variant="ghost"
			disabled={busy || proposal.decision === null}
			onclick={() => onDecide('clear')}>Undo decision</Button
		>
	</div>
	<p class="pr-outcome" aria-live="polite" data-error={error !== null}>
		{error ?? proposalOutcome(proposal, target)}
	</p>
</article>

<style>
	.pr {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		background: var(--surface-card);
		border: 1px solid var(--border-default);
	}
	.pr-head {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex-wrap: wrap;
	}
	.pr-title {
		margin: 0;
		font-size: var(--text-base);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.pr-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.pr-facts {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2) var(--space-5);
		margin: 0;
		font-size: var(--text-xs);
	}
	.pr-wide {
		flex-basis: 100%;
	}
	.pr-facts dt {
		color: var(--text-3);
	}
	.pr-facts dd {
		margin: 0;
		color: var(--text-1);
	}
	.pr-change {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-xs);
	}
	.pr-target {
		margin: 0;
		color: var(--text-3);
	}
	.pr-target code {
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.pr-diff {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		font-family: var(--font-mono);
		color: var(--status-success-fg);
		background: var(--surface-2);
		border: 1px solid var(--border-default);
	}
	.pr-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.pr-outcome {
		margin: 0;
		min-height: 2lh;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.pr-outcome[data-error='true'] {
		color: var(--status-danger-fg);
	}
</style>
