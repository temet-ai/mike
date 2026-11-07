<!--
@component
Sessions › Guard nudges: a summary strip, a vertical list of the guard's questions by short name
(`?question=` in the URL), and for the open question its full wording, a card with its mode,
holdback readouts and honest effect verdict, then its delivered nudges. Mark any nudge right or
wrong in place.
See artifacts/2026-09-29/judge-pages-redesign/SPEC.md §2.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import GuardQuestionPanel from '$lib/components/improvement/GuardQuestionPanel.svelte';
	import GuardWakeList from '$lib/components/improvement/GuardWakeList.svelte';
	import {
		evaluationUuidsOf,
		getDeliveredWakes,
		getGuardEffect,
		groupWakesByQuestion,
		guardQuestionLabel,
		guardQuestionShortLabel,
		isJudged,
		labelGuardEvaluation
	} from '$lib/api/guardEffect';
	import type { GuardWake } from '$lib/api/guardEffect';
	import type { GuardMode } from '$lib/api/guard';
	import { getGuard } from '$lib/api/guard';
	import type { GuardPageData, GuardPageQuestion } from './+page';

	const { data }: { data: GuardPageData } = $props();

	let questions = $state<GuardPageQuestion[] | null>(untrack(() => data.questions));
	let questionsLoadError = $state<string | null>(untrack(() => data.questionsLoadError));
	let wakesByQuestion = $state<Record<string, GuardWake[]>>(untrack(() => data.wakesByQuestion));
	let wakesLoadError = $state<string | null>(untrack(() => data.wakesLoadError));
	const readPointHoldback = untrack(() => data.readPointHoldback);
	const weekNudges = untrack(() => data.weekNudges);

	// Only questions with an effect estimate get a tab and a rail card.
	const listed = $derived((questions ?? []).filter((q) => q.effect !== null));
	const liveCount = $derived(listed.filter((q) => q.mode === 'append' || q.mode === 'wake').length);
	// Same rule as the landing page's Nudges count: only a nudge whose next turn has been judged
	// can be answered now.
	const waitingCount = $derived(
		Object.values(wakesByQuestion)
			.flat()
			.filter((w) => isJudged(w) && w.operatorLabel === null).length
	);

	// The open tab lives in `?question=`. With no (or an unknown) param the first question in the
	// order the API returns them is open.
	let tabParam = $state<string | null>(untrack(() => page.url.searchParams.get('question')));
	$effect(() => {
		const param = page.url.searchParams.get('question');
		if (param !== null && param !== untrack(() => tabParam)) tabParam = param;
	});
	const activeKey = $derived(listed.find((q) => q.key === tabParam)?.key ?? listed[0]?.key ?? '');

	const MODE_WORD: Record<GuardMode, string> = {
		off: 'Off',
		shadow: 'Tried quietly',
		append: 'Live, adds a note',
		wake: 'Live, starts a new turn'
	};

	function setQuestionTab(key: string): void {
		tabParam = key;
		if (page.url.searchParams.get('question') === key) return;
		void goto(`${resolve('/sessions/improvement/guard')}?question=${encodeURIComponent(key)}`, {
			replace: true,
			reset: false
		});
	}

	async function retryQuestions(): Promise<void> {
		questionsLoadError = null;
		try {
			const [summary, effect] = await Promise.all([getGuard(), getGuardEffect()]);
			const effectByKey = new Map(effect.questions.map((q) => [q.questionKey, q]));
			questions = summary.questions
				.filter((q) => q.mode !== 'off')
				.map((q) => ({
					key: q.question,
					mode: q.mode,
					counts24h: { rows: q.counts_24h.rows, wouldFire: q.counts_24h.would_fire },
					effect: effectByKey.get(q.question) ?? null
				}));
		} catch (e) {
			questionsLoadError = e instanceof Error ? e.message : String(e);
		}
	}

	async function retryWakes(): Promise<void> {
		wakesLoadError = null;
		try {
			wakesByQuestion = groupWakesByQuestion(await getDeliveredWakes());
		} catch (e) {
			wakesLoadError = e instanceof Error ? e.message : String(e);
		}
	}

	async function labelWake(
		questionKey: string,
		evaluationUuid: string,
		label: 'right' | 'wrong'
	): Promise<void> {
		const wake = (wakesByQuestion[questionKey] ?? []).find(
			(w) => w.evaluationUuid === evaluationUuid
		);
		const uuids = wake ? evaluationUuidsOf(wake) : [evaluationUuid];
		await Promise.all(uuids.map((uuid) => labelGuardEvaluation(uuid, label)));
		wakesByQuestion = {
			...wakesByQuestion,
			[questionKey]: (wakesByQuestion[questionKey] ?? []).map((w) =>
				w.evaluationUuid === evaluationUuid ? { ...w, operatorLabel: label } : w
			)
		};
	}
</script>

<svelte:head><title>Guard nudges — Sessions — mike</title></svelte:head>

<div class="gp-scroll">
	<div class="gp">
		<PageIntro
			level="h1"
			title="Guard nudges"
			description="The guard can wake the model or add a note to its reply when one of its questions fires. Mark each nudge right or wrong."
		/>

		<div class="page-strip" data-slot="guard-strip">
			<div class="page-strip__group">
				<p class="machine page-strip__cap" id="gps-guard-cap">The guard</p>
				<dl class="page-strip__facts" aria-labelledby="gps-guard-cap">
					<div class="page-strip__fact">
						<dt>Questions</dt>
						<dd class="page-strip__num">{questions ? listed.length : '—'}</dd>
					</div>
					<div class="page-strip__fact">
						<dt>Live</dt>
						<dd class="page-strip__num">{questions ? liveCount : '—'}</dd>
					</div>
					<div class="page-strip__fact">
						<dt>Nudges sent, last 7 days</dt>
						<dd class="page-strip__num">
							{weekNudges ? weekNudges.wakes + weekNudges.appends : '—'}
						</dd>
						{#if weekNudges}
							<dd class="page-strip__sub">
								{weekNudges.wakes}
								{weekNudges.wakes === 1 ? 'wake' : 'wakes'} · {weekNudges.appends}
								{weekNudges.appends === 1 ? 'note' : 'notes'}
							</dd>
						{/if}
					</div>
				</dl>
			</div>
			<div class="page-strip__group">
				<p class="machine page-strip__cap" id="gps-waiting-cap">Waiting on you</p>
				<dl class="page-strip__facts" aria-labelledby="gps-waiting-cap">
					<div class="page-strip__fact gps-fact--pick">
						<dt>Waiting for your answer</dt>
						<dd class="page-strip__num">
							<a
								class="gps-pick"
								class:gps-pick--live={waitingCount > 0}
								href={`${resolve('/sessions/improvement')}?waiting=nudges`}
								aria-label={`Waiting for your answer: ${waitingCount}. Open What the judge did, Nudges tab`}
								>{waitingCount}</a
							>
						</dd>
					</div>
				</dl>
			</div>
		</div>

		{#if questionsLoadError}
			<LoadErrorBanner
				title="Could not load the guard's questions."
				detail={questionsLoadError}
				onRetry={retryQuestions}
			/>
		{:else if questions}
			{#if listed.length === 0}
				<p class="gp-empty">No guard question is running yet.</p>
			{:else}
				<Tabs.Root
					bind:value={() => activeKey, setQuestionTab}
					orientation="vertical"
					class="gp-work"
				>
					<Tabs.List aria-label="Guard questions" class="gp-list">
						{#each listed as question (question.key)}
							<Tabs.Trigger value={question.key} class="gp-q">
								<span class="gp-q-name">{guardQuestionShortLabel(question.key)}</span>
								<span class="gp-count">{(wakesByQuestion[question.key] ?? []).length}</span>
								<span class="gp-q-mode">{MODE_WORD[question.mode]}</span>
							</Tabs.Trigger>
						{/each}
					</Tabs.List>
					{#each listed as question (question.key)}
						<Tabs.Content value={question.key} class="gp-main">
							<h2 class="gp-h2">{guardQuestionLabel(question.key)}</h2>
							{#if question.effect}
								<GuardQuestionPanel
									questionKey={question.key}
									mode={question.mode}
									counts24h={question.counts24h}
									effect={question.effect}
									{readPointHoldback}
								/>
							{/if}
							<GuardWakeList
								wakes={wakesByQuestion[question.key] ?? []}
								lead={false}
								error={wakesLoadError}
								emptyText="This question has not sent a nudge yet."
								onLabel={(evaluationUuid, label) => labelWake(question.key, evaluationUuid, label)}
								onRetry={retryWakes}
							/>
						</Tabs.Content>
					{/each}
				</Tabs.Root>
			{/if}
		{/if}
	</div>
</div>

<style>
	.gp-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		background: var(--surface-bg);
		container-type: inline-size;
	}
	.gp {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	.gp-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}

	.gps-pick {
		color: inherit;
		text-decoration: none;
	}
	.gps-pick::after {
		content: '';
		position: absolute;
		inset: 0;
	}
	.gps-pick--live {
		color: var(--signal-active);
	}
	.gps-fact--pick:hover .gps-pick {
		color: var(--text-strong);
	}
	.gps-pick:focus-visible {
		outline: none;
	}
	.gps-pick:focus-visible::after {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	:global(.gp-work) {
		display: grid;
		grid-template-columns: 18rem minmax(0, 1fr);
		gap: var(--space-5);
		align-items: start;
	}
	:global(.gp-list.gp-list) {
		position: sticky;
		top: var(--space-4);
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: var(--space-1);
		width: auto;
		height: auto;
		overflow: visible;
		border-bottom: 0;
	}
	:global(.gp-q.gp-q) {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: baseline;
		justify-items: start;
		gap: var(--space-1) var(--space-3);
		height: auto;
		padding: var(--space-2) var(--space-3);
		white-space: normal;
		text-align: left;
	}
	.gp-q-name {
		color: inherit;
		overflow-wrap: anywhere;
	}
	.gp-q-mode {
		grid-column: 1 / -1;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-3);
	}
	.gp-count {
		display: inline-block;
		min-width: 2ch;
		justify-self: end;
		color: var(--text-2);
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	:global(.gp-main) {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}
	.gp-h2 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	@container (max-width: 719.98px) {
		.gp {
			padding: var(--space-4) var(--space-3) var(--space-6);
		}
		:global(.gp-work) {
			grid-template-columns: minmax(0, 1fr);
		}
		:global(.gp-list.gp-list) {
			position: static;
		}
	}
</style>
