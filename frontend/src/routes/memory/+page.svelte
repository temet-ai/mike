<script lang="ts">
	import { onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { History, HelpCircle } from '@lucide/svelte';
	import { page } from '$app/state';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import FilterScopeStatus from '$lib/components/shared/FilterScopeStatus.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import WhatThisTouches from '$lib/components/memory/WhatThisTouches.svelte';
	import RunsStrip from '$lib/components/memory/RunsStrip.svelte';
	import DecisionActionCard from '$lib/components/memory/DecisionActionCard.svelte';
	import RelationsSection from '$lib/components/memory/RelationsSection.svelte';
	import OutcomesSection from '$lib/components/memory/OutcomesSection.svelte';
	import DirectiveProposalsSection from '$lib/components/memory/DirectiveProposalsSection.svelte';
	import PatternCandidatesPanel from '$lib/components/memory/PatternCandidatesPanel.svelte';
	import { ApiError } from '$lib/api';
	import {
		getConsolidationRuns,
		getMemoryDecisions,
		restoreLesson,
		describeActionKind,
		type ActionKind,
		type ConsolidationAction,
		type ConsolidationRun
	} from '$lib/api/memoryDecisions';
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

	const ACTION_KINDS: ActionKind[] = [
		'promoted_to_daily',
		'promoted_to_long_term',
		'archived_duplicate',
		'archived_trimmed',
		'archived_expired',
		'deleted_expired',
		'evaluated'
	];

	let runs = $state<ConsolidationRun[]>([]);
	let runsError = $state<string | null>(null);

	async function retryRuns(): Promise<void> {
		runsError = null;
		try {
			runs = (await getConsolidationRuns({ limit: 30 })).items;
		} catch (err) {
			runsError = err instanceof ApiError ? err.message : String(err);
		}
	}

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

	const PAGE_SIZE = 50;

	let decisionItems = $state<ConsolidationAction[]>([]);
	let decisionsTotal = $state(0);
	let decisionsError = $state<string | null>(null);
	let decisionsLoading = $state(false);
	let loadingMore = $state(false);
	let loadMoreError = $state<string | null>(null);

	let projectFilter = $state('');
	let sessionFilter = $state('');
	let fileFilter = $state('');
	let actionKindFilter = $state<'' | ActionKind>('');

	let projects = $state<FleetProjectSummary[]>([]);
	let projectsError = $state<string | null>(null);
	let projectOtherMode = $state(false);

	async function retryProjects(): Promise<void> {
		projectsError = null;
		try {
			projects = await getFleetProjects();
		} catch (err) {
			projectsError = err instanceof Error ? err.message : String(err);
		}
	}

	const PROJECT_OTHER_VALUE = '__other__';

	const sortedProjects = $derived(
		[...projects].sort((a, b) => a.project_path.localeCompare(b.project_path))
	);
	const projectPathSet = $derived(new Set(projects.map((p) => p.project_path)));

	// When projectFilter is empty, only the explicit "Other" pick decides which control shows —
	// otherwise a typed value stood in the box would vanish the instant it's cleared. Once
	// projectFilter is non-empty, its own membership in the known list is authoritative, so a
	// programmatic assignment (URL param, empty-state suggestion) always displays correctly even
	// if projectOtherMode is stale.
	const projectIsOther = $derived(
		projectFilter === '' ? projectOtherMode : !projectPathSet.has(projectFilter)
	);
	const projectSelectValue = $derived(projectIsOther ? PROJECT_OTHER_VALUE : projectFilter);

	function handleProjectSelectChange(event: Event): void {
		const value = (event.currentTarget as HTMLSelectElement).value;
		if (value === PROJECT_OTHER_VALUE) {
			projectOtherMode = true;
			projectFilter = '';
		} else {
			projectOtherMode = false;
			projectFilter = value;
		}
	}

	function applyKnownProjectFromEmptyState(projectPath: string): void {
		projectOtherMode = false;
		projectFilter = projectPath;
		void loadDecisions();
	}

	interface AppliedDecisionFilters {
		project: string;
		session: string;
		file: string;
		actionKind: '' | ActionKind;
	}
	const EMPTY_APPLIED_FILTERS: AppliedDecisionFilters = {
		project: '',
		session: '',
		file: '',
		actionKind: ''
	};
	let appliedFilters = $state<AppliedDecisionFilters>({ ...EMPTY_APPLIED_FILTERS });

	const hasAppliedFilters = $derived(
		!!(
			appliedFilters.project ||
			appliedFilters.session ||
			appliedFilters.file ||
			appliedFilters.actionKind
		)
	);

	const filtersDirty = $derived(
		projectFilter !== appliedFilters.project ||
			sessionFilter !== appliedFilters.session ||
			fileFilter !== appliedFilters.file ||
			actionKindFilter !== appliedFilters.actionKind
	);

	function describeAppliedFilters(filters: AppliedDecisionFilters): string {
		const parts: string[] = [];
		if (filters.project) parts.push(`project ${filters.project}`);
		if (filters.session) parts.push(`session ${filters.session}`);
		if (filters.file) parts.push(`file ${filters.file}`);
		if (filters.actionKind) parts.push(describeActionKind(filters.actionKind).label.toLowerCase());
		return parts.length > 0 ? parts.join(', ') : 'every captured decision';
	}
	const scopeLabel = $derived(describeAppliedFilters(appliedFilters));

	const filteredEmptyText = $derived(`No decisions match ${scopeLabel}`);
	const filteredEmptySubtext = $derived(
		'Project, session, and file filters all require an exact match on the stored value — no partial, alias, or case-insensitive matching. Clear the filters, or pick a known project below.'
	);

	const showResultCounts = $derived(!decisionsLoading && !decisionsError);

	function filtersToQuery(offset: number, filters: AppliedDecisionFilters) {
		const query: {
			offset: number;
			limit: number;
			project?: string;
			session?: string;
			file?: string;
			action_kind?: ActionKind;
		} = { offset, limit: PAGE_SIZE };
		if (filters.project) query.project = filters.project;
		if (filters.session) query.session = filters.session;
		if (filters.file) query.file = filters.file;
		if (filters.actionKind) query.action_kind = filters.actionKind;
		return query;
	}

	async function loadDecisions(): Promise<void> {
		decisionsLoading = true;
		decisionsError = null;
		loadMoreError = null;
		const snapshot: AppliedDecisionFilters = {
			project: projectFilter,
			session: sessionFilter,
			file: fileFilter,
			actionKind: actionKindFilter
		};
		appliedFilters = snapshot;
		try {
			const result = await getMemoryDecisions(filtersToQuery(0, snapshot));
			decisionItems = result.items;
			decisionsTotal = result.total_count;
		} catch (err) {
			decisionsError = err instanceof ApiError ? err.message : String(err);
		} finally {
			decisionsLoading = false;
		}
	}

	async function loadMoreDecisions(): Promise<void> {
		loadingMore = true;
		loadMoreError = null;
		try {
			const result = await getMemoryDecisions(filtersToQuery(decisionItems.length, appliedFilters));
			decisionItems = [...decisionItems, ...result.items];
			decisionsTotal = result.total_count;
		} catch (err) {
			loadMoreError = err instanceof ApiError ? err.message : String(err);
		} finally {
			loadingMore = false;
		}
	}

	function applyFilters(event: SubmitEvent): void {
		event.preventDefault();
		void loadDecisions();
	}

	function clearFilters(): void {
		projectFilter = '';
		projectOtherMode = false;
		sessionFilter = '';
		fileFilter = '';
		actionKindFilter = '';
		void loadDecisions();
	}

	const remaining = $derived(Math.max(0, decisionsTotal - decisionItems.length));

	let restoringActionId = $state<number | null>(null);
	let restoreErrors = $state<Record<number, string>>({});
	const restoredActionIds = new SvelteSet<number>();
	let restoreAnnouncement = $state('');

	async function handleRestore(action: ConsolidationAction): Promise<void> {
		if (action.lesson_id === null) return;
		restoringActionId = action.id;
		const nextErrors = { ...restoreErrors };
		delete nextErrors[action.id];
		restoreErrors = nextErrors;
		try {
			await restoreLesson(action.lesson_id);
			restoredActionIds.add(action.id);
			restoreAnnouncement = `Lesson #${action.lesson_id} restored.`;
		} catch (err) {
			restoreErrors = {
				...restoreErrors,
				[action.id]: err instanceof ApiError ? err.message : String(err)
			};
			restoreAnnouncement = `Couldn't restore lesson #${action.lesson_id}.`;
		} finally {
			restoringActionId = null;
		}
	}

	onMount(() => {
		runs = data.runs ?? [];
		runsError = data.runsLoadError;
		decisionItems = data.decisions?.items ?? [];
		decisionsTotal = data.decisions?.total_count ?? 0;
		decisionsError = data.decisionsLoadError;
		relationItems = data.relations?.items ?? [];
		relationsTotal = data.relations?.total_count ?? 0;
		relationsError = data.relationsLoadError;
		lessonOutcomes = data.lessonOutcomes ?? [];
		outcomesError = data.lessonOutcomesLoadError;
		projects = data.projects ?? [];
		projectsError = data.projectsLoadError;

		const urlProject = page.url.searchParams.get('project');
		const urlSession = page.url.searchParams.get('session');
		if (urlProject) projectFilter = urlProject;
		if (urlSession) sessionFilter = urlSession;
		if (urlProject || urlSession) void loadDecisions();

		if (data.runsLoadError) void retryRuns();
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
			<span class="flex items-center gap-1.5 flex-wrap">
				<span>What consolidation has done to your project's</span>
				<span class="inline-flex items-center gap-1">
					<span>lessons</span>
					<HelpHint slug="lesson" />
				</span>
				<span>— every promotion, archive, and restore, with the evidence behind it.</span>
			</span>
		</PageIntro>
	</header>

	<WhatThisTouches />

	{#if runsError}
		<LoadErrorBanner
			title="Couldn't load run status"
			detail={runsError}
			hint="The consolidation-runs endpoint may not be reachable yet."
			onRetry={retryRuns}
		/>
	{:else}
		<RunsStrip {runs} nightlyHourUtc={data.nightlyHourUtc} now={Date.now()} />
	{/if}

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

	<section class="mem-feed-section" data-design="funk" aria-labelledby="mem-feed-heading">
		<h2 id="mem-feed-heading" class="mem-feed-heading">Decisions</h2>
		<span class="sr-only" role="status" aria-live="polite">{restoreAnnouncement}</span>
		<form class="mem-filters" data-augmented-ui="tl-clip br-clip border" onsubmit={applyFilters}>
			<div class="mem-filter">
				<span class="mem-filter__label">
					<span id="mem-filter-project-label">Project</span>
					<Tooltip
						text="Matches the stored project path exactly — no partial, alias, or case-insensitive matches. Pick a value from the list to guarantee a match, or choose “Other” to type a path this list doesn't have yet."
					>
						<HelpCircle size={12} aria-hidden="true" class="mem-filter__label-help" />
					</Tooltip>
				</span>
				<select
					id="mem-filter-project"
					class="mem-filter__select"
					aria-labelledby="mem-filter-project-label"
					value={projectSelectValue}
					onchange={handleProjectSelectChange}
				>
					<option value="">All projects</option>
					{#each sortedProjects as p (p.project_path)}
						<option value={p.project_path}>{p.project_path}</option>
					{/each}
					<option value={PROJECT_OTHER_VALUE}>Other — type a path</option>
				</select>
				{#if projectSelectValue === PROJECT_OTHER_VALUE}
					<input
						class="mem-filter__input mem-filter__input--other"
						type="text"
						bind:value={projectFilter}
						placeholder="/exact/project/path"
						aria-label="Exact project path"
					/>
				{/if}
				{#if projectsError}
					<span class="mem-filter__error" role="alert"
						>Couldn't load the project list: {projectsError}</span
					>
				{/if}
			</div>
			<label class="mem-filter">
				<span class="mem-filter__label">Session</span>
				<input
					class="mem-filter__input"
					type="text"
					bind:value={sessionFilter}
					placeholder="session uuid"
				/>
			</label>
			<label class="mem-filter">
				<span class="mem-filter__label">File</span>
				<input
					class="mem-filter__input"
					type="text"
					bind:value={fileFilter}
					placeholder="path/to/file.rs"
				/>
			</label>
			<label class="mem-filter">
				<span class="mem-filter__label">Action</span>
				<select class="mem-filter__select" bind:value={actionKindFilter}>
					<option value="">All actions</option>
					{#each ACTION_KINDS as kind (kind)}
						<option value={kind}>{describeActionKind(kind).label}</option>
					{/each}
				</select>
			</label>
			<div class="mem-filter__actions">
				<button type="submit" class="mem-filter__apply">Apply</button>
				<button type="button" class="mem-filter__clear" onclick={clearFilters}>Clear</button>
			</div>
			<FilterScopeStatus
				{scopeLabel}
				shownCount={showResultCounts ? decisionItems.length : undefined}
				totalCount={showResultCounts ? decisionsTotal : undefined}
				itemNoun="decision"
				dirty={filtersDirty}
			/>
		</form>

		{#if decisionsLoading}
			<div class="mem-status" role="status" aria-live="polite">Loading…</div>
		{:else if decisionsError}
			<LoadErrorBanner
				title="Couldn't load the decision feed"
				detail={decisionsError}
				hint="The memory-decisions endpoint may not be reachable yet."
				onRetry={loadDecisions}
			/>
		{:else if decisionItems.length === 0}
			<FilteredEmptyState
				icon={History}
				filtered={hasAppliedFilters}
				emptyText="No memory decisions recorded yet"
				emptySubtext="Decisions appear here once consolidation runs and promotes, archives, evaluates, or restores a lesson."
				filteredText={filteredEmptyText}
				filteredSubtext={filteredEmptySubtext}
				onClearFilters={clearFilters}
			>
				{#if sortedProjects.length > 0}
					<div class="mem-empty-projects">
						<span class="mem-empty-projects__label"
							>Known projects with captured data — pick one to filter by it:</span
						>
						<ul class="mem-empty-projects__list" role="list">
							{#each sortedProjects as p (p.project_path)}
								<li>
									<button
										type="button"
										class="mem-empty-projects__btn"
										onclick={() => applyKnownProjectFromEmptyState(p.project_path)}
									>
										{p.project_path}
									</button>
								</li>
							{/each}
						</ul>
					</div>
				{:else if projectsError}
					<span class="mem-empty-projects__label"
						>Couldn't load the project list to suggest one: {projectsError}</span
					>
				{/if}
			</FilteredEmptyState>
		{:else}
			<ul class="mem-feed" role="list">
				{#each decisionItems as action (action.id)}
					<li>
						<DecisionActionCard
							{action}
							onRestore={handleRestore}
							restoring={restoringActionId === action.id}
							restoreError={restoreErrors[action.id] ?? null}
							restored={restoredActionIds.has(action.id)}
						/>
					</li>
				{/each}
			</ul>
			{#if remaining > 0}
				<div class="mem-footer" data-augmented-ui="tl-clip br-clip border">
					<span class="mem-footer__count" role="status">
						Showing {decisionItems.length} of {decisionsTotal.toLocaleString()}; {remaining.toLocaleString()}
						more not loaded.
					</span>
					<button
						type="button"
						class="mem-footer__load-more"
						disabled={loadingMore}
						onclick={loadMoreDecisions}
					>
						{loadingMore ? 'Loading…' : 'Load more decisions'}
					</button>
					{#if loadMoreError}
						<span class="mem-footer__error" role="alert">{loadMoreError}</span>
					{/if}
				</div>
			{/if}
		{/if}
	</section>
</section>

<style>
	.mem-feed-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.mem-feed-heading {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-1);
	}

	.mem-page {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		max-width: 1000px;
		margin: 0 auto;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}

	.mem-filters {
		--edge-tl: var(--edge-sm);
		--edge-br: var(--edge-md);
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: var(--hairline);
		--aug-border-bg: linear-gradient(
			135deg,
			var(--cyan) 0%,
			var(--cyan-border) 45%,
			var(--line) 80%
		);
		display: flex;
		gap: var(--space-3);
		flex-wrap: wrap;
		align-items: flex-end;
		padding: calc(var(--edge-tl) + var(--space-1)) calc(var(--edge-br) + var(--space-1))
			calc(var(--edge-br) + var(--space-1)) calc(var(--edge-tl) + var(--space-1));
		background: var(--edge-surface);
	}
	.mem-filter {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 160px;
		flex: 1 1 160px;
	}
	.mem-filter__label {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-2);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
	}
	:global(.mem-filter__label-help) {
		color: var(--text-3);
		cursor: help;
	}
	.mem-filter__input,
	.mem-filter__select {
		background: var(--surface-2);
		border: var(--hairline) solid var(--line);
		border-radius: var(--r-tol);
		padding: var(--space-1) var(--space-2);
		font-size: var(--text-sm);
		font-family: var(--font-mono, monospace);
		color: var(--text-1);
	}
	.mem-filter__input--other {
		margin-top: var(--space-1);
		width: 100%;
	}
	.mem-filter__error {
		font-size: var(--text-xs);
		color: var(--signal-error);
	}
	.mem-filter__actions {
		display: flex;
		gap: var(--space-2);
	}
	.mem-filter__apply,
	.mem-filter__clear {
		border: var(--hairline) solid var(--line);
		background: var(--surface-2);
		color: var(--text-1);
		border-radius: var(--r-tol);
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-xs);
		cursor: pointer;
		transition:
			border-color var(--dur-1) var(--ease-io),
			color var(--dur-1) var(--ease-io);
	}
	.mem-filter__apply:hover,
	.mem-filter__clear:hover {
		border-color: var(--cyan-border);
		color: var(--text-1);
	}

	.mem-status {
		font-size: var(--text-sm);
		color: var(--text-2);
		padding: var(--space-2) 0;
	}

	.mem-feed {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.mem-footer {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--edge-line);
		background: var(--edge-surface);
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) calc(var(--edge) + var(--space-1));
		flex-wrap: wrap;
	}
	.mem-footer__count {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.mem-footer__load-more {
		border: var(--hairline) solid var(--line);
		background: var(--surface-2);
		color: var(--text-1);
		border-radius: var(--r-tol);
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-xs);
		cursor: pointer;
		transition: border-color var(--dur-1) var(--ease-io);
	}
	.mem-footer__load-more:hover:not(:disabled) {
		border-color: var(--cyan-border);
		color: var(--text-1);
	}
	.mem-footer__load-more:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.mem-footer__error {
		font-size: var(--text-xs);
		color: var(--signal-error);
	}

	.mem-empty-projects {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
	}
	.mem-empty-projects__label {
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.mem-empty-projects__list {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
		max-width: 64ch;
	}
	.mem-empty-projects__btn {
		border: var(--hairline) solid var(--line);
		background: var(--surface-2);
		color: var(--text-1);
		border-radius: var(--r-tol);
		padding: var(--space-1) var(--space-2);
		font-size: var(--text-xs);
		font-family: var(--font-mono, monospace);
		cursor: pointer;
		overflow-wrap: anywhere;
		transition: border-color var(--dur-1) var(--ease-io);
	}
	.mem-empty-projects__btn:hover {
		border-color: var(--cyan-border);
	}
</style>
