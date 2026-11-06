<!--
@component
One answer to one question on the Review labels page, as a band of the rail's card. Its
heading names what it is ("This turn's label", "The judge's answer"); the answer word and its
definition follow. The label says who labelled the turn and, behind a disclosure, why. The judge
says which model answered, its reason, whether it saw the assistant's last message, and behind a
disclosure the text it was given, in sections. A `label` of null is the judge's answer on a turn
the judge did not label.
-->
<script lang="ts">
	import type { EvalVerdictDto } from '$lib/api/evalRuns';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { cardHeading, judgeInputSections, judgeMissedLast, notGradedReason } from './evalReview';
	import type { Axis, Holder, WhyItem } from './evalReview';
	import { wordFor } from './labelControl';

	interface Props {
		axis: Axis;
		label: string | null;
		holders: Holder[];
		definition: string | null;
		labelledBy: string | null;
		why: WhyItem[];
		verdict: EvalVerdictDto | null;
		answeredBy: string | null;
		judgeSaw: string | null;
		notGraded: string | null;
	}

	let {
		axis,
		label,
		holders,
		definition,
		labelledBy,
		why,
		verdict,
		answeredBy,
		judgeSaw,
		notGraded
	}: Props = $props();

	let whyOpen = $state(false);
	let inputOpen = $state(false);

	const isLabel = $derived(holders.includes('label') || holders.includes('earlier'));
	const isJudge = $derived(holders.includes('judge'));
	const word = $derived(label ? wordFor(label) : null);
	const input = $derived(
		typeof verdict?.input_text === 'string' && verdict.input_text ? verdict.input_text : null
	);
	const sections = $derived(judgeInputSections(input));
	const missed = $derived(judgeMissedLast(judgeSaw));
	const stillWanted = $derived(
		axis === 'delivery' && verdict?.unmet_ask ? verdict.unmet_ask : null
	);
</script>

<article class="eac" data-testid="answer-card" data-label={label ?? 'none'}>
	<div class="band-hd band-hd--static">
		<h3 class="machine eac-heading">{cardHeading(holders)}</h3>
	</div>
	<div class="band-body eac-body">
		<p class="eac-word">{word ?? '—'}</p>
		{#if definition}<p class="eac-def">{definition}</p>{/if}

		{#if isLabel && labelledBy}
			<div class="eac-field">
				<span class="machine eac-k">Labelled by</span>
				<p class="eac-v" data-testid="labelled-by">{labelledBy}</p>
			</div>
		{/if}
		{#if isLabel && why.length > 0 && word}
			<button
				type="button"
				class="eac-toggle"
				aria-expanded={whyOpen}
				onclick={() => (whyOpen = !whyOpen)}
			>
				<DisclosureMarker open={whyOpen} size={10} /><span>Why it is labelled {word}</span>
			</button>
			{#if whyOpen}
				<div class="eac-why" data-testid="why-labelled">
					{#each why as item, i (i)}
						{#if item.kind === 'said'}
							<div class="eac-field">
								<span class="machine eac-k">{item.who}</span>
								<p class="eac-v">{item.text}</p>
							</div>
						{:else if item.kind === 'text'}
							<p class="eac-v">{item.text}</p>
						{:else if item.kind === 'rules'}
							<details class="eac-rules" open={item.open} data-testid="labelling-rules">
								<summary class="eac-toggle">How mixed cases are labelled</summary>
								<ul class="eac-list">
									{#each item.items as n (n)}<li>{n}</li>{/each}
								</ul>
							</details>
						{:else}
							<div class="eac-field">
								<span class="machine eac-k">The labelling also noted</span>
								<ul class="eac-list">
									{#each item.items as n (n)}<li>{n}</li>{/each}
								</ul>
							</div>
						{/if}
					{/each}
				</div>
			{/if}
		{/if}

		{#if isJudge}
			{#if notGraded}
				<p class="eac-v">The judge did not label this turn: {notGradedReason(notGraded)}.</p>
			{:else if verdict}
				{#if answeredBy}
					<div class="eac-field">
						<span class="machine eac-k">Answered by</span>
						<p class="eac-v" data-testid="answered-by">{answeredBy}</p>
					</div>
				{/if}
				<div class="eac-field">
					<span class="machine eac-k">Its reason</span>
					<p class="eac-v">{verdict.rationale ? verdict.rationale : 'none given.'}</p>
				</div>
				{#if stillWanted}
					<div class="eac-field">
						<span class="machine eac-k">Still wanted, per the judge</span>
						<p class="eac-v">{stillWanted}</p>
					</div>
				{/if}
				{#if judgeSaw}<p class="eac-saw" class:eac-saw--warn={missed} data-testid="judge-saw">
						{judgeSaw}{#if missed}{' '}Its answer rests on incomplete input, so it says little about
							how good the judge is.{/if}
					</p>{/if}
				{#if input}
					<button
						type="button"
						class="eac-toggle"
						aria-expanded={inputOpen}
						onclick={() => (inputOpen = !inputOpen)}
					>
						<DisclosureMarker open={inputOpen} size={10} /><span>Show what the judge saw</span>
					</button>
					{#if inputOpen}
						<div class="eac-why" data-testid="judge-input">
							{#if sections}
								{#each sections as s (s.title)}
									<div class="eac-field">
										<span class="machine eac-k">{s.title}</span>
										<p class="eac-pre">{s.text}</p>
									</div>
								{/each}
							{:else}
								<p class="eac-pre">{input}</p>
							{/if}
						</div>
					{/if}
				{/if}
			{/if}
		{/if}
	</div>
</article>

<style>
	.eac {
		min-width: 0;
	}
	.eac-heading {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.eac-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.eac-word {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-xl);
		font-weight: var(--weight-display);
		line-height: var(--leading-tight);
		color: var(--text-1);
	}
	.eac-def {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.eac-field {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
	}
	.eac-k {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eac-v {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.eac-pre {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.eac-saw {
		margin: 0;
		font-size: var(--text-sm);
		font-weight: var(--weight-display);
		color: var(--text-1);
	}
	.eac-rules {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.eac-rules > .eac-list {
		margin-top: var(--space-1);
	}
	.eac-saw--warn {
		padding: var(--space-2) var(--space-3);
		border: var(--hairline) solid var(--status-warning-border);
		background: var(--status-warning-bg);
		color: var(--status-warning-fg);
	}
	.eac-list {
		margin: 0;
		padding-left: var(--space-4);
		list-style: square;
		font-size: var(--text-sm);
		color: var(--text-1);
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.eac-why {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-1);
	}
	.eac-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-xs);
		color: var(--text-2);
		text-align: left;
		cursor: pointer;
	}
	.eac-toggle:hover {
		color: var(--text-1);
	}
	.eac-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
