<!--
@component
One labelling accuracy run, described in plain words: which judge and configuration it measured, on
which turns, when, how often it got the label right on each question, how many turns still differ
and wait for review, and whether it passed the accuracy bar set for being put in use. A run is
always named by what it is (date, configuration, model) and linked to its page; its id appears only
in the URL. Used on Labelling accuracy (one card per run) and at the top of a run's own page.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import type { BundleDto, EvalGateDto, EvalRunDto } from '$lib/api/evalRuns';
	import { Button } from '$lib/components/ui/button';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { formatWhen } from '$lib/components/eval/evalReview';
	import {
		JUDGE_NAME,
		LABELLING_QUERY,
		QUESTION_NAME,
		configurationLine,
		gateBarLines,
		questionScore,
		runName,
		runRole,
		sessionScore,
		statusWords,
		turnSetWords
	} from './judges';
	import type { QuestionCounts, QuestionKey } from './judges';

	interface Props {
		run: EvalRunDto;
		bundle: BundleDto | null;
		liveBundleId: number | null;
		gate: EvalGateDto | null;
		counts: Partial<Record<QuestionKey, QuestionCounts>> | null;
		gateOpen?: boolean;
		others?: EvalRunDto[];
		titleLevel?: 'h2' | 'h3';
	}

	let {
		run,
		bundle,
		liveBundleId,
		gate,
		counts,
		gateOpen = false,
		others = [],
		titleLevel = 'h2'
	}: Props = $props();

	// svelte-ignore state_referenced_locally
	let open = $state(gateOpen);

	const role = $derived(runRole(run));
	const inUse = $derived(liveBundleId !== null && run.bundle_id === liveBundleId);
	const status = $derived(statusWords(run.status));
	const config = $derived(configurationLine(bundle) || run.model);
	const session = $derived(role === 'session_judge' ? sessionScore(run) : null);
	const questions: QuestionKey[] = ['reaction', 'delivery'];

	function runHref(id: number): string {
		return resolve(`sessions/labelling/accuracy/${id}` as `sessions/labelling/accuracy/${string}`);
	}

	function reviewHref(key: QuestionKey): string {
		return `${resolve(`sessions/labelling/accuracy/${run.id}/review` as `sessions/labelling/accuracy/${string}/review`)}?mode=${key}`;
	}

	function nameOf(id: number): string {
		const found = others.find((r) => r.id === id);
		return found ? runName(found) : 'an earlier accuracy run';
	}

	const labellingHref = $derived(`${resolve('/sessions/labelling')}${LABELLING_QUERY[role]}`);

	const regression = $derived(gate?.report?.regression?.corrected ?? null);
</script>

<article class="trc edge mech-group" data-testid="test-run-card" data-run={run.id}>
	<div class="band-hd band-hd--static">
		<svelte:element this={titleLevel} class="machine trc-title"
			><a class="trc-link" href={runHref(run.id)}>{runName(run)}</a></svelte:element
		>
		{#if status}<span class="band-hd__meta">{status}</span>{/if}
	</div>
	<div class="band-body trc-body">
		<dl class="trc-facts">
			<dt class="machine">Judge</dt>
			<dd><a class="trc-link" href={labellingHref}>{JUDGE_NAME[role]}</a></dd>
			<dt class="machine">Configuration</dt>
			<dd>
				<a class="trc-link" href={labellingHref}>Configuration {run.bundle_id}</a
				>{#if inUse}&nbsp;(in use){/if} · {config}
			</dd>
			<dt class="machine">Measured on</dt>
			<dd>{turnSetWords(run)}</dd>
			<dt class="machine">Ran</dt>
			<dd>
				{[formatWhen(run.created_at), formatWhen(run.finished_at)].filter(Boolean).join(' to ')}
			</dd>
			{#if run.repeat_of_run_id !== null}
				<dt class="machine">Repeat of</dt>
				<dd>
					<a class="trc-link" href={runHref(run.repeat_of_run_id)}>{nameOf(run.repeat_of_run_id)}</a
					>, to see how much the result moves by chance
				</dd>
			{/if}
		</dl>

		<div class="trc-scores" data-testid="run-scores">
			{#if role === 'session_judge'}
				<span class="machine trc-q">How the session ended</span>
				{#if session}
					<span class="trc-num">agreed on {session.agreed} of {session.n}</span>
					<span class="trc-num trc-pct">{session.percent}</span>
				{:else}
					<span class="trc-none">no score yet</span>
				{/if}
			{:else}
				{#each questions as key (key)}
					{@const score = questionScore(run, key)}
					{@const left = counts?.[key]}
					<span class="machine trc-q">{QUESTION_NAME[key]}</span>
					{#if score}
						<span class="trc-num">agreed on {score.agreed} of {score.n}</span>
						<span class="trc-num trc-pct">{score.percent}</span>
						<span class="trc-left">
							{#if left}
								{#if left.total === 0}
									<span class="trc-none">nothing to review</span>
								{:else}
									{left.total} differ from the label · {left.reviewed} reviewed
								{/if}
							{:else}
								{score.n - score.agreed} differ from the label
							{/if}
						</span>
						<span class="trc-act">
							{#if !left || left.total > 0}
								<Button variant="outline" size="sm" href={reviewHref(key)}>Review labels</Button>
							{/if}
						</span>
					{:else}
						<span class="trc-none trc-span">this configuration does not answer this question</span>
					{/if}
				{/each}
			{/if}
		</div>

		{#if gate}
			<button type="button" class="trc-toggle" aria-expanded={open} onclick={() => (open = !open)}>
				<DisclosureMarker {open} size={10} /><span
					>{gate.verdict === 'pass' ? 'Passed' : 'Did not pass'} the accuracy bar set for being put in
					use{#if gate.candidate_run_id !== run.id}, checked on the run this one repeats{/if}</span
				>
			</button>
			{#if open}
				<ul class="trc-gate" data-testid="accuracy-bar">
					{#each gateBarLines(gate) as line (line)}<li>{line}</li>{/each}
					{#if regression && gate.baseline_run_id}
						<li>
							Against <a class="trc-link" href={runHref(gate.baseline_run_id)}
								>{nameOf(gate.baseline_run_id)}</a
							>
							it gained {regression.gained ?? 0} turns labelled Corrected and lost {regression.lost ??
								0}.
						</li>
					{/if}
				</ul>
			{/if}
		{/if}
	</div>
</article>

<style>
	.trc {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.trc-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.trc-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.trc-link:focus-visible,
	.trc-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.trc-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.trc-facts {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: var(--space-1) var(--space-4);
		margin: 0;
		font-size: var(--text-sm);
	}
	.trc-facts dt {
		font-size: var(--text-xs);
		color: var(--text-2);
		align-self: baseline;
	}
	.trc-facts dd {
		margin: 0;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.trc-scores {
		display: grid;
		grid-template-columns: auto auto auto minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-2) var(--space-4);
		padding-top: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.trc-q {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.trc-num {
		font-family: var(--font-mono);
		font-variant-numeric: tabular-nums;
	}
	.trc-pct {
		font-weight: var(--weight-display);
	}
	.trc-left {
		color: var(--text-2);
	}
	.trc-none {
		color: var(--text-2);
	}
	.trc-span {
		grid-column: 2 / -1;
	}
	.trc-act {
		justify-self: end;
		min-block-size: var(--control-h);
		display: flex;
		align-items: center;
	}
	.trc-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-1);
		cursor: pointer;
	}
	.trc-toggle:hover {
		color: var(--interactive-active-fg);
	}
	.trc-gate {
		margin: 0;
		padding-left: var(--space-5);
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	@container (max-width: 640px) {
		.trc-scores {
			grid-template-columns: auto auto minmax(0, 1fr);
		}
		.trc-left {
			grid-column: 1 / 3;
		}
	}
</style>
