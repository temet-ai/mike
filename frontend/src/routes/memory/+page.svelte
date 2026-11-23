<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import WhatThisTouches from '$lib/components/memory/WhatThisTouches.svelte';
	import ConsolidationSection from '$lib/components/memory/consolidation/ConsolidationSection.svelte';
	import RelationsSection from '$lib/components/memory/RelationsSection.svelte';
	import OutcomesSection from '$lib/components/memory/OutcomesSection.svelte';
	import DirectiveProposalsSection from '$lib/components/memory/DirectiveProposalsSection.svelte';
	import PatternCandidatesPanel from '$lib/components/memory/PatternCandidatesPanel.svelte';
	import { ApiError } from '$lib/api';
	import {
		getLessonRelations,
		getLessonOutcomes,
		confirmLessonRelation,
		rejectLessonRelation,
		type LessonRelation,
		type LessonOutcomeSummary
	} from '$lib/api/memoryFeedback';
	import { getFleetProjects } from '$lib/fleet/api';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import type { MemoryPageData } from './+page';

	const { data }: { data: MemoryPageData } = $props();

	const RELATIONS_PAGE_SIZE = 50;

	let relationItems = $state<LessonRelation[]>([]);
	let relationsTotal = $state(0);
	let relationsError = $state<string | null>(null);
	let relationsLoading = $state(false);
	let relationsLoadingMore = $state(false);
	let relationsLoadMoreError = $state<string | null>(null);

	async function retryRelations(): Promise<void> {
		relationsLoading = true;
		relationsError = null;
		try {
			const result = await getLessonRelations({ limit: RELATIONS_PAGE_SIZE });
			relationItems = result.items;
			relationsTotal = result.total_count;
		} catch (err) {
			relationsError = err instanceof ApiError ? err.message : String(err);
		} finally {
			relationsLoading = false;
		}
	}

	async function loadMoreRelations(): Promise<void> {
		relationsLoadingMore = true;
		relationsLoadMoreError = null;
		try {
			const result = await getLessonRelations({
				offset: relationItems.length,
				limit: RELATIONS_PAGE_SIZE
			});
			relationItems = [...relationItems, ...result.items];
			relationsTotal = result.total_count;
		} catch (err) {
			relationsLoadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			relationsLoadingMore = false;
		}
	}

	const relationsRemaining = $derived(Math.max(0, relationsTotal - relationItems.length));

	let decidingRelationId = $state<number | null>(null);
	let relationDecideErrors = $state<Record<number, string>>({});
	let relationDecisionAnnouncement = $state('');

	async function decideRelation(
		relation: LessonRelation,
		action: (id: number) => Promise<LessonRelation>,
		verb: 'confirmed' | 'rejected'
	): Promise<void> {
		decidingRelationId = relation.id;
		const nextErrors = { ...relationDecideErrors };
		delete nextErrors[relation.id];
		relationDecideErrors = nextErrors;
		try {
			const updated = await action(relation.id);
			relationItems = relationItems.map((r) => (r.id === updated.id ? updated : r));
			relationDecisionAnnouncement = `Relation #${relation.id} ${verb}.`;
		} catch (err) {
			relationDecideErrors = {
				...relationDecideErrors,
				[relation.id]: err instanceof ApiError ? err.message : String(err)
			};
			relationDecisionAnnouncement = `Couldn't record a decision for relation #${relation.id}.`;
		} finally {
			decidingRelationId = null;
		}
	}

	function handleConfirmRelation(relation: LessonRelation): void {
		void decideRelation(relation, confirmLessonRelation, 'confirmed');
	}
	function handleRejectRelation(relation: LessonRelation): void {
		void decideRelation(relation, rejectLessonRelation, 'rejected');
	}

	let lessonOutcomes = $state<LessonOutcomeSummary[]>([]);
	let outcomesError = $state<string | null>(null);
	let outcomesLoading = $state(false);

	async function retryOutcomes(): Promise<void> {
		outcomesLoading = true;
		outcomesError = null;
		try {
			lessonOutcomes = await getLessonOutcomes();
		} catch (err) {
			outcomesError = err instanceof ApiError ? err.message : String(err);
		} finally {
			outcomesLoading = false;
		}
	}

	let projects = $state<FleetProjectSummary[]>([]);
	let projectsError = $state<string | null>(null);
	let initialProject = $state<string | null>(null);

	async function retryProjects(): Promise<void> {
		projectsError = null;
		try {
			projects = await getFleetProjects();
		} catch (err) {
			projectsError = err instanceof Error ? err.message : String(err);
		}
	}

	function rememberProject(project: FleetProjectSummary): void {
		const url = new URL(page.url.href);
		url.searchParams.set('project', project.project_path);
		replaceState(`${url.pathname}${url.search}${url.hash}`, {});
	}

	onMount(() => {
		relationItems = data.relations?.items ?? [];
		relationsTotal = data.relations?.total_count ?? 0;
		relationsError = data.relationsLoadError;
		lessonOutcomes = data.lessonOutcomes ?? [];
		outcomesError = data.lessonOutcomesLoadError;
		projects = data.projects ?? [];
		projectsError = data.projectsLoadError;
		initialProject = page.url.searchParams.get('project');

		if (data.relationsLoadError) void retryRelations();
		if (data.lessonOutcomesLoadError) void retryOutcomes();
		if (data.projectsLoadError) void retryProjects();
	});
</script>

<svelte:head>
	<title>Memory — mike</title>
</svelte:head>

<section class="mem-page">
	<header class="mem-page__head">
		<PageIntro title="Memory" level="h1">
			What consolidation has done to your project's lessons
			<HelpHint slug="lesson" />
			and guidance files, with the evidence behind it.
		</PageIntro>
	</header>

	{#if projectsError}
		<LoadErrorBanner
			title="Couldn't load the project list"
			detail={projectsError}
			onRetry={retryProjects}
		/>
	{:else if projects.length > 0}
		<ConsolidationSection {projects} {initialProject} onProjectChange={rememberProject} />
	{/if}

	<WhatThisTouches />

	{#if relationsError}
		<LoadErrorBanner
			title="Couldn't load lesson relationships"
			detail={relationsError}
			hint="The memory-relations endpoint may not be reachable yet."
			onRetry={retryRelations}
		/>
	{:else if !relationsLoading}
		<RelationsSection
			relations={relationItems}
			total={relationsTotal}
			onConfirm={handleConfirmRelation}
			onReject={handleRejectRelation}
			decidingId={decidingRelationId}
			decisionAnnouncement={relationDecisionAnnouncement}
			decideErrors={relationDecideErrors}
			remaining={relationsRemaining}
			loadingMore={relationsLoadingMore}
			loadMoreError={relationsLoadMoreError}
			onLoadMore={loadMoreRelations}
		/>
	{/if}

	{#if outcomesError}
		<LoadErrorBanner
			title="Couldn't load lesson outcomes"
			detail={outcomesError}
			hint="The lesson-outcomes endpoint may not be reachable yet."
			onRetry={retryOutcomes}
		/>
	{:else if !outcomesLoading}
		<OutcomesSection outcomes={lessonOutcomes} />
		<PatternCandidatesPanel />
		<DirectiveProposalsSection />
	{/if}
</section>

<style>
	.mem-page {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		box-sizing: border-box;
		width: 100%;
		min-width: 0;
		max-width: 1000px;
		margin: 0 auto;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}
</style>
