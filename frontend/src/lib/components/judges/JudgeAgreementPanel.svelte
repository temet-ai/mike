<!--
@component
The latest labelling accuracy of the configuration in use: how often the judge gave the right label
on each question in its newest finished accuracy run, how many turns still differ and wait for
review, and a way to review them. The run is named by what it is and linked to its page. With no
such run it says so and how to start one.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import type { EvalRunDto } from '$lib/api/evalRuns';
	import { Button } from '$lib/components/ui/button';
	import {
		LABELLING_QUERY,
		QUESTION_NAME,
		questionScore,
		runName,
		sessionScore,
		turnSetWords
	} from './judges';
	import type { JudgeRole, QuestionCounts, QuestionKey } from './judges';

	interface Props {
		role: JudgeRole;
		run: EvalRunDto | null;
		counts: Partial<Record<QuestionKey, QuestionCounts>> | null;
	}

	let { role, run, counts }: Props = $props();

	const keys: QuestionKey[] = ['reaction', 'delivery'];
	const session = $derived(run && role === 'session_judge' ? sessionScore(run) : null);
	const left = $derived.by(() => {
		if (!counts) return null;
		const total = (counts.reaction?.total ?? 0) + (counts.delivery?.total ?? 0);
		const reviewed = (counts.reaction?.reviewed ?? 0) + (counts.delivery?.reviewed ?? 0);
		return { total, reviewed };
	});
	const differWords = $derived(
		keys
			.map((k) => ({ k, n: counts?.[k]?.total ?? 0 }))
			.filter((x) => x.n > 0)
			.map((x) => `${x.n} turns for ${QUESTION_NAME[x.k].toLowerCase()}`)
			.join(' and ') || 'no turns'
	);
	const command = $derived(
		role === 'session_judge' ? 'mike eval run --role session_judge' : 'mike eval run'
	);

	function runHref(id: number): string {
		return resolve(`sessions/labelling/accuracy/${id}` as `sessions/labelling/accuracy/${string}`);
	}
</script>

<section class="jap edge mech-group" aria-label="Latest labelling accuracy" data-testid="agreement">
	<div class="band-hd band-hd--static">
		<h2 class="machine jap-title">Latest labelling accuracy</h2>
	</div>
	<div class="band-body jap-body">
		{#if !run}
			<p class="jap-text">
				The accuracy of this configuration has not been measured yet. Start an accuracy run from the
				command line:
				<code class="jap-cmd">{command}</code>
			</p>
		{:else}
			<p class="jap-text">
				<a class="jap-link" href={runHref(run.id)}>{runName(run)}</a> · {turnSetWords(
					run
				)}{#if run.repeat_of_run_id !== null}{' '}· a repeat of an earlier run on the same turns, to
					see how much the result moves by chance{/if}
			</p>
			<div class="jap-scores">
				{#if role === 'session_judge'}
					<span class="machine jap-q">How the session ended</span>
					{#if session}
						<span class="jap-num">{session.agreed} of {session.n}</span>
						<span class="jap-num jap-pct">{session.percent}</span>
					{:else}
						<span class="jap-text">no score yet</span>
					{/if}
				{:else}
					{#each keys as key (key)}
						{@const score = questionScore(run, key)}
						<span class="machine jap-q">{QUESTION_NAME[key]}</span>
						{#if score}
							<span class="jap-num">{score.agreed} of {score.n}</span>
							<span class="jap-num jap-pct">{score.percent}</span>
						{:else}
							<span class="jap-text jap-span">not answered by this configuration</span>
						{/if}
					{/each}
				{/if}
			</div>
			{#if role === 'session_judge'}
				{#if session && session.n < 30}
					<p class="jap-text">
						Too few to tell. Start a larger accuracy run from the command line:
						<code class="jap-cmd">{command}</code>
					</p>
				{/if}
			{:else if left}
				<div class="jap-decide">
					<p class="jap-text" data-testid="differ">
						The judge's answer and the label differ on {differWords}. You have reviewed {left.reviewed}
						of {left.total}. Your answer becomes the turn's label, so a turn you review counts as
						reviewed in every accuracy run that includes it.
					</p>
					{#if left.total > left.reviewed}
						<Button
							variant="default"
							size="sm"
							href={resolve(
								`sessions/labelling/accuracy/${run.id}/review` as `sessions/labelling/accuracy/${string}/review`
							)}>Review labels</Button
						>
					{/if}
				</div>
			{/if}
			<a
				class="jap-link jap-all"
				href={`${resolve('/sessions/labelling/accuracy')}${LABELLING_QUERY[role]}`}
				>All labelling accuracy runs</a
			>
		{/if}
	</div>
</section>

<style>
	.jap {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.jap-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.jap-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.jap-text {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.jap-scores {
		display: grid;
		grid-template-columns: auto auto auto;
		justify-content: start;
		align-items: baseline;
		gap: var(--space-2) var(--space-5);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.jap-q {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.jap-num {
		font-family: var(--font-mono);
		font-variant-numeric: tabular-nums;
	}
	.jap-pct {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
	}
	.jap-span {
		grid-column: 2 / -1;
		color: var(--text-2);
	}
	.jap-decide {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-4);
	}
	.jap-cmd {
		font-family: var(--font-mono);
		color: var(--text-1);
	}
	.jap-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
		font-size: var(--text-sm);
	}
	.jap-all {
		align-self: flex-start;
	}
	.jap-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
