<script lang="ts">
	import { onMount } from 'svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Select, type SelectOption } from '$lib/components/ui/select';
	import * as Tabs from '$lib/components/ui/tabs';
	import { formatRun } from '$lib/components/ui/trend-chart';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import type { DecisionRow } from '$lib/api/knowledgeConsolidation';
	import ChangeRow from './ChangeRow.svelte';
	import ConflictRow from './ConflictRow.svelte';
	import MeasuresBand from './MeasuresBand.svelte';
	import NewRowsButton from './NewRowsButton.svelte';
	import SubjectsBand from './SubjectsBand.svelte';
	import WorkerStatusLine from './WorkerStatusLine.svelte';
	import {
		ConsolidationStore,
		type BandName,
		type ChangeFilter,
		type MeasureWindow
	} from './consolidation.svelte';
	import { COPY, VERDICTS, VERDICT_ORDER, formatClock, sectionState, showsBands } from './labels';

	interface Props {
		projects: FleetProjectSummary[];
		/** Project id to start on; the newest project is used when absent or unknown. */
		initialProject?: string | null;
		onProjectChange?: (project: FleetProjectSummary) => void;
		store?: ConsolidationStore;
		/** Turns polling off, for stories and tests that hold a fixed fixture. */
		poll?: boolean;
	}

	let {
		projects,
		initialProject = null,
		onProjectChange,
		store = new ConsolidationStore(),
		poll = true
	}: Props = $props();

	const options = $derived(
		projects
			.filter((p) => p.project_id !== null && p.project_id !== undefined)
			.sort((a, b) => a.project_path.localeCompare(b.project_path))
	);
	const selectOptions = $derived<SelectOption[]>(
		options.map((p) => ({ value: String(p.project_id), label: p.custom_name ?? p.project_path }))
	);

	function pickInitial(): FleetProjectSummary | null {
		const named = options.find(
			(p) => String(p.project_id) === initialProject || p.project_path === initialProject
		);
		if (named) return named;
		return [...options].sort((a, b) => b.last_seen.localeCompare(a.last_seen))[0] ?? null;
	}

	let selected = $state('');

	onMount(() => {
		const first = pickInitial();
		if (first) {
			selected = String(first.project_id);
			void store.start(selected, poll);
		}
		return () => store.dispose();
	});

	async function changeProject(): Promise<void> {
		const next = options.find((p) => String(p.project_id) === selected);
		if (!next) return;
		onProjectChange?.(next);
		store.measures = [];
		store.conflicts = [];
		store.changes = [];
		store.subjects = [];
		await store.start(selected, poll);
	}

	const mode = $derived(sectionState(store.capability, store.measures));
	const bands = $derived(showsBands(mode));
	const lastRun = $derived(
		store.measures.length > 0 ? store.measures[store.measures.length - 1]!.measured_at : null
	);
	const asOf = (band: BandName): string => {
		const at = store.refresh[band].at;
		return at === null ? '' : `as of ${formatClock(new Date(at).toISOString())}`;
	};

	const FILTERS: SelectOption[] = [
		{ value: 'changes', label: 'Changes' },
		{ value: 'every', label: 'Every decision' },
		...VERDICT_ORDER.map((v) => ({ value: v, label: VERDICTS[v].word }))
	];
	let filter = $state<ChangeFilter>('changes');

	const WINDOWS: { value: MeasureWindow; label: string }[] = [
		{ value: '7', label: '7 days' },
		{ value: '30', label: '30 days' },
		{ value: 'all', label: 'All' }
	];
	let windowValue = $state<MeasureWindow>('30');

	const pointedRun = $derived(store.pointAt !== null ? formatRun(store.pointAt) : null);
	const latestRun = $derived(lastRun ? formatRun(Date.parse(lastRun)) : '');

	function rowBusy(row: DecisionRow): boolean {
		return store.inFlight === row.id;
	}
</script>

{#snippet refreshMeta(band: BandName, text: string)}
	{#if store.refresh[band].failedAt !== null}
		<Tooltip text={store.refresh[band].error ?? ''}>
			<span class="band-hd__meta">
				not refreshed since <span class="cs__stale">
					{formatClock(new Date(store.refresh[band].at ?? Date.now()).toISOString())}
				</span>
			</span>
		</Tooltip>
	{:else}
		<span class="band-hd__meta">{text}</span>
	{/if}
{/snippet}

<section
	class="cs"
	id="consolidation"
	aria-label="Consolidation"
	data-slot="consolidation-section"
	data-state={mode}
>
	<PageIntro level="h2" title="Consolidation" description={COPY.lede}>
		{#snippet actions()}
			{#if selectOptions.length > 0}
				<Select
					class="cs__project"
					label="Project"
					options={selectOptions}
					bind:value={selected}
					onchange={changeProject}
				/>
			{/if}
		{/snippet}
	</PageIntro>

	<div class="edge mech-group">
		<div data-band="status">
			{#if store.capabilityError}
				<div class="cs__pad">
					<LoadErrorBanner
						title="Couldn't load the worker status"
						detail={store.capabilityError}
						onRetry={() => void store.loadCapability()}
					/>
				</div>
			{:else}
				<WorkerStatusLine {mode} capability={store.capability} {lastRun} />
			{/if}
			{#if mode === 'empty'}
				<div class="cs__note">
					<span>{COPY.empty}</span>
					<Button variant="outline" size="sm" href="/settings/knowledge">Knowledge sources</Button>
				</div>
			{/if}
		</div>

		{#if bands}
			<div data-band="conflicts">
				<div class="band-hd band-hd--static">
					<span class="machine cs__label" id="consolidation-conflicts">Open conflicts</span>
					<span class="cs__grow"></span>
					<NewRowsButton
						count={store.newConflicts}
						onShow={() => void store.showNew('conflicts')}
					/>
					{@render refreshMeta('conflicts', `${store.conflicts.length} · ${asOf('conflicts')}`)}
				</div>
				{#if store.conflictsError}
					<div class="cs__pad">
						<LoadErrorBanner
							title="Couldn't load open conflicts"
							detail={store.conflictsError}
							onRetry={() => void store.loadConflicts()}
						/>
					</div>
				{:else if store.conflicts.length === 0}
					<p class="cs__none">None open.</p>
				{:else}
					<div class="cs__list">
						{#each store.conflicts as row (row.id)}
							<ConflictRow
								{row}
								project={selected}
								settled={store.settled[row.id]}
								busy={rowBusy(row)}
								disabled={store.inFlight !== null}
								error={store.rowErrors[row.id] ?? null}
								onKeep={(id) => void store.keep(row, id)}
							/>
						{/each}
					</div>
				{/if}
			</div>

			<div data-band="measures">
				<div class="band-hd band-hd--static">
					<span class="machine cs__label">Measures</span>
					<Tabs.Root
						value={windowValue}
						onValueChange={(v) => {
							windowValue = v as MeasureWindow;
							void store.setWindow(windowValue);
						}}
					>
						<Tabs.List aria-label="Window">
							{#each WINDOWS as w (w.value)}
								<Tabs.Trigger value={w.value}>{w.label}</Tabs.Trigger>
							{/each}
						</Tabs.List>
					</Tabs.Root>
					<span class="cs__grow"></span>
					<span class="band-hd__meta cs__run">
						{#if store.refresh.measures.failedAt !== null}
							<Tooltip text={store.refresh.measures.error ?? ''}>
								<span>
									not refreshed since <span class="cs__stale">
										{formatClock(new Date(store.refresh.measures.at ?? Date.now()).toISOString())}
									</span>
								</span>
							</Tooltip>
						{:else if latestRun}
							{pointedRun ? `Run ${pointedRun}` : `Latest run ${latestRun}`}
						{/if}
					</span>
				</div>
				{#if store.measuresError}
					<div class="cs__pad">
						<LoadErrorBanner
							title="Couldn't load the measures"
							detail={store.measuresError}
							onRetry={() => void store.loadMeasures()}
						/>
					</div>
				{:else if mode === 'first-pass' || store.measures.length === 0}
					<p class="cs__none">{COPY.firstPass}</p>
				{:else}
					<MeasuresBand measures={store.measures} bind:pointAt={store.pointAt} />
				{/if}
			</div>

			<div data-band="changes">
				<div class="band-hd band-hd--static">
					<span class="machine cs__label">Recent changes</span>
					<Select
						label="Show"
						class="cs__show"
						options={FILTERS}
						bind:value={filter}
						onchange={() => void store.setChangesFilter(filter)}
					/>
					<span class="cs__grow"></span>
					<NewRowsButton count={store.newChanges} onShow={() => void store.showNew('changes')} />
					{@render refreshMeta('changes', asOf('changes'))}
				</div>
				{#if store.changesError && store.changes.length === 0}
					<div class="cs__pad">
						<LoadErrorBanner
							title="Couldn't load recent changes"
							detail={store.changesError}
							onRetry={() => void store.loadChanges()}
						/>
					</div>
				{:else if store.changes.length === 0}
					<p class="cs__none">No changes yet.</p>
				{:else}
					<div class="cs__list">
						{#each store.changes as row (row.id)}
							<ChangeRow
								{row}
								project={selected}
								busy={rowBusy(row)}
								disabled={store.inFlight !== null}
								error={store.rowErrors[row.id] ?? null}
								onAction={(kind) => void (kind === 'undo' ? store.undo(row) : store.apply(row))}
							/>
						{/each}
					</div>
					{#if store.changesNext !== null}
						<div class="cs__more">
							<Button
								variant="outline"
								size="sm"
								disabled={store.loadingOlder}
								onclick={() => void store.loadOlderChanges()}
							>
								Load older changes
							</Button>
						</div>
					{/if}
				{/if}
			</div>

			<div data-band="subjects">
				<div class="band-hd band-hd--static">
					<span class="machine cs__label">Subjects</span>
					<span class="cs__grow"></span>
					<NewRowsButton count={store.newSubjects} onShow={() => void store.showNew('subjects')} />
					{@render refreshMeta('subjects', `${store.subjects.length} loaded · ${asOf('subjects')}`)}
				</div>
				{#if store.subjectsError && store.subjects.length === 0}
					<div class="cs__pad">
						<LoadErrorBanner
							title="Couldn't load subjects"
							detail={store.subjectsError}
							onRetry={() => void store.loadSubjects()}
						/>
					</div>
				{:else if store.subjects.length === 0}
					<p class="cs__none">No subjects yet.</p>
				{:else}
					<SubjectsBand subjects={store.subjects} project={selected} />
					{#if store.subjectsMore}
						<div class="cs__more">
							<Button
								variant="outline"
								size="sm"
								disabled={store.loadingMoreSubjects}
								onclick={() => void store.loadMoreSubjects()}
							>
								Load more subjects
							</Button>
						</div>
					{/if}
				{/if}
			</div>
		{/if}
	</div>
</section>

<style>
	.cs {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		scroll-margin-top: var(--space-4);
	}
	.cs__pad {
		padding: var(--space-3) var(--space-4);
	}
	.cs__label {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.cs__grow {
		flex: 1 1 0;
	}
	.cs__none {
		margin: 0;
		padding: 0 var(--space-4) var(--space-3);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.cs__note {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		padding: 0 var(--space-4) var(--space-3);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.cs__list > :global(* + *) {
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.cs__more {
		display: flex;
		justify-content: center;
		padding: var(--space-3) var(--space-4);
	}
	.cs__run {
		min-width: 11rem;
		text-align: right;
		white-space: nowrap;
	}
	.cs__stale {
		color: var(--status-warning-fg);
	}
	.cs :global([data-slot='page-intro-actions']) {
		flex: 0 1 24rem;
		min-width: 0;
	}
	.cs :global(.cs__project) {
		inline-size: 100%;
	}
	.cs :global(.band-hd) {
		flex-wrap: wrap;
	}
	.cs :global(.cs__show) {
		flex-direction: row;
		align-items: center;
	}
</style>
