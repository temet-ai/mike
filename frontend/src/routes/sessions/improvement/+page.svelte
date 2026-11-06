<!--
@component
Sessions › What the judge did: a summary strip, what is waiting on you as tabs with counts
(`?waiting=` in the URL), and beside it a sticky rail answering whether corrections are falling.
See artifacts/2026-09-29/judge-pages-redesign/SPEC.md §1.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import ImprovementStrip from '$lib/components/improvement/ImprovementStrip.svelte';
	import WaitingForYou, { isWaitingTab } from '$lib/components/improvement/WaitingForYou.svelte';
	import type { WaitingTab } from '$lib/components/improvement/WaitingForYou.svelte';
	import LessonCandidateList from '$lib/components/improvement/LessonCandidateList.svelte';
	import CorrectionTrend from '$lib/components/improvement/CorrectionTrend.svelte';
	import RepeatedAsks from '$lib/components/improvement/RepeatedAsks.svelte';
	import ActionEffectTable from '$lib/components/improvement/ActionEffectTable.svelte';
	import GuardWakeList from '$lib/components/improvement/GuardWakeList.svelte';
	import DirectiveProposalsSection from '$lib/components/memory/DirectiveProposalsSection.svelte';
	import {
		approveLessonCandidate,
		getCorrectionTrend,
		getImprovementSummary,
		getRepeats,
		listActionEffects,
		listLessonCandidates,
		rejectLessonCandidate
	} from '$lib/api/improvement';
	import type {
		ActionEffectItem,
		GuardWake,
		ImprovementSummary,
		LessonCandidate,
		RejectReason,
		RepeatsResponse,
		TrendResponse
	} from '$lib/api/improvement';
	import {
		evaluationUuidsOf,
		getDeliveredWakes,
		isJudged,
		labelGuardEvaluation
	} from '$lib/api/guardEffect';
	import { listProposals } from '$lib/api/directiveProposals';
	import type { DirectiveProposal } from '$lib/api/directiveProposals';
	import type { ImprovementPageData } from './+page';

	const { data }: { data: ImprovementPageData } = $props();

	// Local mirrors so each section can retry itself without reloading the whole route — same
	// pattern as routes/memory/+page.svelte. Named distinctly from the `lessons`/`nudges`/
	// `directives` snippet props WaitingForYou expects below: a `{#snippet lessons()}` block would
	// otherwise shadow a same-named page variable inside its own body.
	//
	// Each field below deliberately snapshots `data` once: `+page.ts` already resolved every fetch
	// before this component rendered (design-spec.md §9), and each field then diverges
	// independently via its own retry/mutate call (approve/reject/label). Re-deriving from `data`
	// would overwrite an in-flight local edit the moment `data` was read again, so each line is a
	// one-time snapshot, not a bug.
	// svelte-ignore state_referenced_locally -- see note above
	let lessonCandidates = $state<LessonCandidate[] | null>(data.candidates);
	// svelte-ignore state_referenced_locally -- see note above
	let candidatesError = $state<string | null>(data.candidatesError);
	// svelte-ignore state_referenced_locally -- see note above
	let directiveProposals = $state<DirectiveProposal[] | null>(data.proposals);
	// svelte-ignore state_referenced_locally -- see note above
	let proposalsError = $state<string | null>(data.proposalsError);
	// svelte-ignore state_referenced_locally -- see note above
	let guardWakes = $state<GuardWake[] | null>(data.wakes);
	// svelte-ignore state_referenced_locally -- see note above
	let wakesError = $state<string | null>(data.wakesError);
	// svelte-ignore state_referenced_locally -- see note above
	let summary = $state<ImprovementSummary | null>(data.summary);
	// svelte-ignore state_referenced_locally -- see note above
	let summaryError = $state<string | null>(data.summaryError);
	// svelte-ignore state_referenced_locally -- see note above
	let actions = $state<ActionEffectItem[] | null>(data.actions);
	// svelte-ignore state_referenced_locally -- see note above
	let actionsError = $state<string | null>(data.actionsError);
	// svelte-ignore state_referenced_locally -- see note above
	let trend = $state<TrendResponse | null>(data.trend);
	// svelte-ignore state_referenced_locally -- see note above
	let trendError = $state<string | null>(data.trendError);
	// svelte-ignore state_referenced_locally -- see note above
	let repeats = $state<RepeatsResponse | null>(data.repeats);
	// svelte-ignore state_referenced_locally -- see note above
	let repeatsError = $state<string | null>(data.repeatsError);

	// A decided lesson keeps its row until the next page load (stable-list-ordering); this records
	// how each was decided so its band can say so and the counts leave it out.
	const decided = new SvelteMap<number, 'approved' | 'rejected'>();

	const lessonsWaiting = $derived(
		lessonCandidates === null
			? (summary?.lessonCandidateCount ?? 0)
			: lessonCandidates.filter((c) => !decided.has(c.id)).length
	);
	// Only nudges whose next turn has been judged can be answered now, which the summary's
	// guard_wakes_unlabelled matches (summary.rs).
	const nudgesWaiting = $derived(
		guardWakes === null
			? (summary?.unlabelledWakeCount ?? 0)
			: guardWakes.filter((w) => isJudged(w) && w.operatorLabel === null).length
	);
	const directivesWaiting = $derived(
		directiveProposals === null ? (summary?.openProposalCount ?? 0) : directiveProposals.length
	);
	const counts = $derived({
		lessons: lessonsWaiting,
		nudges: nudgesWaiting,
		directives: directivesWaiting
	});
	const stripSummary = $derived<ImprovementSummary | null>(
		summary === null
			? null
			: {
					...summary,
					lessonCandidateCount: lessonsWaiting,
					unlabelledWakeCount: nudgesWaiting,
					openProposalCount: directivesWaiting
				}
	);
	const latestTrendPoint = $derived(trend?.weekly.at(-1) ?? null);

	// The open tab lives in `?waiting=`. With no param the page opens on the first tab with
	// something waiting, chosen once here; a count reaching 0 later never switches the tab.
	function defaultTab(): WaitingTab {
		const initial = untrack(() => counts);
		if (initial.lessons > 0) return 'lessons';
		if (initial.nudges > 0) return 'nudges';
		if (initial.directives > 0) return 'directives';
		return 'lessons';
	}

	const initialParam = untrack(() => page.url.searchParams.get('waiting'));
	let waitingTab = $state<WaitingTab>(isWaitingTab(initialParam) ? initialParam : defaultTab());

	$effect(() => {
		const param = page.url.searchParams.get('waiting');
		if (isWaitingTab(param) && param !== untrack(() => waitingTab)) waitingTab = param;
	});

	function setWaitingTab(tab: WaitingTab): void {
		waitingTab = tab;
		if (page.url.searchParams.get('waiting') === tab) return;
		void goto(`${resolve('/sessions/improvement')}?waiting=${tab}`, {
			replaceState: true,
			noScroll: true,
			keepFocus: true
		});
	}

	function pickFromStrip(tab: WaitingTab): void {
		setWaitingTab(tab);
		document.getElementById('imp-waiting-hd')?.scrollIntoView({ block: 'start' });
	}

	function detailHrefFor(id: number): string {
		return resolve(
			`/sessions/improvement/lessons/${id}` as `/sessions/improvement/lessons/${string}`
		);
	}

	async function retryCandidates(): Promise<void> {
		candidatesError = null;
		try {
			lessonCandidates = (await listLessonCandidates()).candidates;
		} catch (e) {
			candidatesError = e instanceof Error ? e.message : String(e);
		}
	}

	async function retryProposals(): Promise<void> {
		proposalsError = null;
		try {
			directiveProposals = (await listProposals('open')).proposals;
		} catch (e) {
			proposalsError = e instanceof Error ? e.message : String(e);
		}
	}

	function proposalDecided(): void {
		void retryProposals();
	}

	async function retryWakes(): Promise<void> {
		wakesError = null;
		try {
			guardWakes = (await getDeliveredWakes()).filter((w) => w.operatorLabel === null);
		} catch (e) {
			wakesError = e instanceof Error ? e.message : String(e);
		}
	}

	async function retryActions(): Promise<void> {
		actionsError = null;
		try {
			actions = (await listActionEffects()).actions;
		} catch (e) {
			actionsError = e instanceof Error ? e.message : String(e);
		}
	}

	async function retryTrend(): Promise<void> {
		trendError = null;
		try {
			trend = await getCorrectionTrend({ weeks: 8 });
		} catch (e) {
			trendError = e instanceof Error ? e.message : String(e);
		}
	}

	async function retryRepeats(): Promise<void> {
		repeatsError = null;
		try {
			repeats = await getRepeats({ weeks: 12 });
		} catch (e) {
			repeatsError = e instanceof Error ? e.message : String(e);
		}
	}

	async function approveCandidate(id: number): Promise<void> {
		await approveLessonCandidate(id);
		decided.set(id, 'approved');
	}

	async function rejectCandidate(id: number, reason: RejectReason, note?: string): Promise<void> {
		await rejectLessonCandidate(id, reason, note);
		decided.set(id, 'rejected');
	}

	async function labelWake(evaluationUuid: string, label: 'right' | 'wrong'): Promise<void> {
		const wake = (guardWakes ?? []).find((w) => w.evaluationUuid === evaluationUuid);
		const uuids = wake ? evaluationUuidsOf(wake) : [evaluationUuid];
		await Promise.all(uuids.map((uuid) => labelGuardEvaluation(uuid, label)));
		guardWakes = (guardWakes ?? []).map((w) =>
			w.evaluationUuid === evaluationUuid ? { ...w, operatorLabel: label } : w
		);
	}
</script>

<svelte:head><title>What the judge did — Sessions — mike</title></svelte:head>

<div class="imp-scroll">
	<div class="imp">
		<PageIntro
			level="h1"
			title="What the judge did"
			description="What mike did with the turns you corrected, what is waiting for you, and whether you are correcting Claude less."
		/>
		<ImprovementStrip
			summary={stripSummary}
			{summaryError}
			latest={latestTrendPoint}
			onPick={pickFromStrip}
		/>
		<div class="imp-work">
			<section class="imp-main" aria-labelledby="imp-waiting-hd">
				<h2 class="imp-h2" id="imp-waiting-hd">Waiting for you</h2>
				<WaitingForYou {counts} bind:tab={() => waitingTab, setWaitingTab}>
					{#snippet lessons()}
						<LessonCandidateList
							candidates={lessonCandidates ?? []}
							error={candidatesError}
							{decided}
							{detailHrefFor}
							onApprove={approveCandidate}
							onReject={rejectCandidate}
							onRetry={retryCandidates}
						/>
					{/snippet}
					{#snippet nudges()}
						<GuardWakeList
							wakes={guardWakes ?? []}
							error={wakesError}
							emptyText="No nudges are waiting for your answer."
							onLabel={labelWake}
							onRetry={retryWakes}
						/>
					{/snippet}
					{#snippet directives()}
						<DirectiveProposalsSection
							proposals={directiveProposals ?? []}
							error={proposalsError}
							emptyText="No standing directives are waiting on you."
							onDecided={proposalDecided}
						/>
					{/snippet}
				</WaitingForYou>
				<section class="imp-sub" aria-labelledby="imp-repeats-hd">
					<h2 class="imp-h2" id="imp-repeats-hd">What you keep saying</h2>
					<p class="imp-note">
						Things you have told Claude three or more times, across a week or more or in more than
						one project. If mike did each one itself, you would not have to say it again.
					</p>
					<RepeatedAsks {repeats} error={repeatsError} onRetry={retryRepeats} />
				</section>
			</section>
			<aside class="imp-rail" id="imp-better" aria-labelledby="imp-better-hd">
				<h2 class="imp-h2" id="imp-better-hd">Is it getting better</h2>
				<p class="imp-note">
					These numbers compare periods before and after each change. They are not a controlled
					experiment — other things can move at the same time.
				</p>
				<CorrectionTrend
					weekly={trend?.weekly ?? []}
					markers={trend?.markers ?? []}
					windowLabel="last 8 weeks"
					error={trendError}
					onRetry={retryTrend}
				/>
				<ActionEffectTable actions={actions ?? []} error={actionsError} onRetry={retryActions} />
			</aside>
		</div>
	</div>
</div>

<style>
	.imp-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		background: var(--surface-bg);
		container-type: inline-size;
	}
	.imp {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		max-width: 1600px;
		padding: var(--space-5) var(--space-6) var(--space-8);
	}
	.imp-work {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 24rem;
		gap: var(--space-5);
		align-items: start;
	}
	.imp-main,
	.imp-rail {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
	}
	.imp-sub {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin-top: var(--space-5);
		min-width: 0;
	}
	.imp-rail {
		position: sticky;
		top: var(--space-4);
		max-block-size: calc(100dvh - 44px - 2 * var(--space-4));
		overflow-y: auto;
	}
	.imp-h2 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
		color: var(--text-strong);
	}
	.imp-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		line-height: var(--leading-body);
	}
	@container (max-width: 1199.98px) {
		.imp-work {
			grid-template-columns: minmax(0, 1fr);
		}
		.imp-rail {
			position: static;
			max-block-size: none;
			overflow-y: visible;
		}
	}
	@container (max-width: 719.98px) {
		.imp {
			padding: var(--space-4) var(--space-3) var(--space-6);
		}
	}
</style>
