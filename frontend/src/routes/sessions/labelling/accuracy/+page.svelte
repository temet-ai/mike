<script lang="ts">
	import { untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import * as Tabs from '$lib/components/ui/tabs';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { disclose } from '$lib/components/ui/motion';
	import TestRunCard from '$lib/components/judges/TestRunCard.svelte';
	import { resolve } from '$app/paths';
	import {
		QUESTION_NAME,
		gateFor,
		modelOf,
		newestFirst,
		questionScore
	} from '$lib/components/judges/judges';
	import type { QuestionCounts, QuestionKey } from '$lib/components/judges/judges';
	import { disagreementCounts } from '$lib/components/judges/judgePageData';
	import type { EvalRunsPageData } from './+page';

	let { data }: { data: EvalRunsPageData } = $props();

	let tab = $state<'turn_judge' | 'session_judge'>(untrack(() => data.tab));

	function pick(value: string) {
		const next = value === 'session_judge' ? 'session_judge' : 'turn_judge';
		if (next === tab) return;
		tab = next;
		const url = new URL(page.url.href);
		if (next === 'session_judge') url.searchParams.set('for', 'sessions');
		else url.searchParams.delete('for');
		try {
			replaceState(`${url.pathname}${url.search}`, {});
		} catch {
			return;
		}
	}
	let howOpen = $state(false);

	const counts = new SvelteMap<number, Partial<Record<QuestionKey, QuestionCounts>>>();

	const turnRuns = $derived(newestFirst(data.turnRuns));
	const sessionRuns = $derived(newestFirst(data.sessionRuns));
	const shown = $derived(tab === 'turn_judge' ? turnRuns : sessionRuns);
	const liveId = $derived(tab === 'turn_judge' ? data.liveTurn : data.liveSession);

	const inUse = $derived(
		[
			{ label: 'Turns', id: data.liveTurn },
			{ label: 'Sessions', id: data.liveSession }
		].map((c) => ({
			...c,
			model: c.id > 0 ? (modelOf(data.bundles.find((b) => b.id === c.id)) ?? 'model not recorded') : 'none set'
		}))
	);
	const latestTurn = $derived(turnRuns.find((r) => r.status === 'complete') ?? null);
	const latestHref = $derived(
		latestTurn
			? resolve(
					`sessions/labelling/accuracy/${latestTurn.id}` as `sessions/labelling/accuracy/${string}`
				)
			: null
	);
	const latestScores = $derived(
		(['reaction', 'delivery'] as const).map((key) => {
			const score = latestTurn ? questionScore(latestTurn, key) : null;
			return {
				key,
				label: `Agrees on ${QUESTION_NAME[key].toLowerCase()}`,
				percent: score?.percent ?? '—',
				of: score ? `${score.agreed} of ${score.n} turns` : 'not answered'
			};
		})
	);

	$effect(() => {
		for (const run of turnRuns) {
			untrack(() => {
				if (counts.has(run.id) || !run.report?.reaction) return;
				counts.set(run.id, {});
				disagreementCounts(run.id)
					.then((c) => counts.set(run.id, c))
					.catch(() => counts.delete(run.id));
			});
		}
	});
</script>

<svelte:head><title>Judge accuracy — Sessions — mike</title></svelte:head>

<div class="er-scroll">
	<div class="er-page">
		<div class="er-head">
			<PageIntro
				level="h1"
				title="Judge accuracy"
				description="How often the judge's answer matches the label, run by run."
			/>
		</div>

		<div class="page-strip" data-slot="accuracy-strip" data-testid="accuracy-strip">
			<div class="page-strip__group">
				<p class="machine page-strip__cap" id="er-strip-runs">Accuracy runs</p>
				<dl class="page-strip__facts" aria-labelledby="er-strip-runs">
					<div class="page-strip__fact">
						<dt>Turns</dt>
						<dd class="page-strip__num">{turnRuns.length}</dd>
					</div>
					<div class="page-strip__fact">
						<dt>Sessions</dt>
						<dd class="page-strip__num">{sessionRuns.length}</dd>
					</div>
				</dl>
			</div>
			<div class="page-strip__group">
				<p class="machine page-strip__cap" id="er-strip-live">Configuration in use</p>
				<dl class="page-strip__facts" aria-labelledby="er-strip-live">
					{#each inUse as c (c.label)}
						<div class="page-strip__fact" data-testid="strip-config">
							<dt>{c.label}</dt>
							<dd class="page-strip__num">{c.id > 0 ? c.id : 'None'}</dd>
							<dd class="page-strip__sub">{c.model}</dd>
						</div>
					{/each}
				</dl>
			</div>
			{#if latestTurn}
				<div class="page-strip__group">
					<p class="machine page-strip__cap" id="er-strip-latest">Latest turn run</p>
					<dl class="page-strip__facts" aria-labelledby="er-strip-latest">
						{#each latestScores as s (s.key)}
							<div class="page-strip__fact" data-testid="strip-agreement">
								<dt>{s.label}</dt>
								<dd class="page-strip__num">{s.percent}</dd>
								<dd class="page-strip__sub">
									{s.of}{#if latestHref}{' · '}<a href={latestHref}>See the run</a>{/if}
								</dd>
							</div>
						{/each}
					</dl>
				</div>
			{/if}
		</div>

		<div class="er-how">
			<button
				type="button"
				class="er-how-toggle"
				aria-expanded={howOpen}
				aria-controls="er-how-body"
				onclick={() => (howOpen = !howOpen)}
			>
				<DisclosureMarker open={howOpen} size={10} /><span>How accuracy is measured</span>
			</button>
			{#if howOpen}
				<div class="er-how-body" id="er-how-body" transition:disclose>
					<div class="er-how-inner">
						<p class="er-how-p">
							How often the judge's answer matches the label. In each accuracy run the judge labels a
							fixed set of turns that were labelled beforehand, and every answer is compared with that
							label. Most of those labels were made by AI too: two AI models labelled each turn separately
							and a third settled where they differed. The rest are labels you chose. A score here is
							agreement with those labels, not proof the judge is right.
						</p>
						<p class="er-how-p">
							Use this page to compare configurations, and open Review labels on a run to go through the
							turns where the judge's answer and the label differ and say which is right. Your answer
							becomes the turn's label, so it counts in every accuracy run that includes that turn.
						</p>
						<p class="er-how-p">
							Accuracy runs are started from the command line with <code class="er-cmd"
								>mike eval run</code
							>.
						</p>
					</div>
				</div>
			{/if}
		</div>

		{#if data.error}
			<p class="er-error" role="alert">The accuracy runs could not be loaded: {data.error}</p>
		{/if}

		<Tabs.Root value={tab} onValueChange={pick}>
			<Tabs.List aria-label="What is labelled">
				<Tabs.Trigger value="turn_judge"
					>Turns<span class="er-count">{turnRuns.length}</span></Tabs.Trigger
				>
				<Tabs.Trigger value="session_judge"
					>Sessions<span class="er-count">{sessionRuns.length}</span></Tabs.Trigger
				>
			</Tabs.List>
		</Tabs.Root>

		{#if shown.length === 0}
			<p class="er-sub">
				No accuracy runs yet. Start one from the command line: <code class="er-cmd"
					>{tab === 'session_judge' ? 'mike eval run --role session_judge' : 'mike eval run'}</code
				>
			</p>
		{:else}
			<ol class="er-list">
				{#each shown as run (run.id)}
					<li>
						<TestRunCard
							{run}
							bundle={data.bundles.find((b) => b.id === run.bundle_id) ?? null}
							liveBundleId={liveId}
							gate={gateFor(data.gates, run.id)}
							counts={counts.get(run.id) ?? null}
							others={shown}
						/>
					</li>
				{/each}
			</ol>
		{/if}
	</div>
</div>

<style>
	.er-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		background: var(--surface-bg);
		container-type: inline-size;
	}
	.er-page {
		padding: var(--space-5) var(--space-6) var(--space-8);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: 1600px;
	}
	.er-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.er-how {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.er-how-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-2);
		text-align: left;
		cursor: pointer;
	}
	.er-how-toggle:hover {
		color: var(--text-1);
	}
	.er-how-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.er-how-body {
		display: grid;
	}
	.er-how-inner {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-height: 0;
		overflow: hidden;
	}
	.er-how-p {
		margin: 0;
		max-width: 80ch;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}
	.er-cmd {
		font-family: var(--font-mono);
		color: var(--text-1);
	}
	.er-count {
		margin-left: var(--space-2);
		color: var(--text-2);
	}
	.er-sub {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.er-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.er-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	@container (max-width: 719.98px) {
		.er-page {
			padding: var(--space-4) var(--space-3) var(--space-6);
		}
	}
</style>
