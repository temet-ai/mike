<script lang="ts">
	import { onMount } from 'svelte';
	import { ArrowLeft } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		DataTable,
		DataTableCell,
		DataTableRow,
		type DataTableColumn
	} from '$lib/components/ui/data-table';
	import { TriangleAlert } from '@lucide/svelte';
	import { ApiError } from '$lib/api';
	import {
		applyConsolidationDecision,
		getConsolidationDecisions,
		getConsolidationSubject,
		undoConsolidationDecision,
		type DecisionRow,
		type SubjectDetail
	} from '$lib/api/knowledgeConsolidation';
	import { getFleetProjects } from '$lib/fleet/api';
	import CarrierRef from '$lib/components/memory/consolidation/CarrierRef.svelte';
	import ChangeRow from '$lib/components/memory/consolidation/ChangeRow.svelte';
	import {
		STATEMENT_STATES,
		carrierOfStatement,
		conflictedStatementIds,
		formatClock,
		formatDayMonth,
		formatFull,
		isRetired,
		refusalFor
	} from '$lib/components/memory/consolidation/labels';

	const PAGE_SIZE = 20;
	const id = $derived(Number(page.params.id));

	let subject = $state<SubjectDetail | null>(null);
	let notFound = $state(false);
	let loadError = $state<string | null>(null);
	let scope = $state('');

	let decisions = $state<DecisionRow[]>([]);
	let decisionsError = $state<string | null>(null);
	let next = $state<number | null>(null);
	let loadingOlder = $state(false);
	let conflicted = $state<Set<number>>(new Set());
	let inFlight = $state<number | null>(null);
	let rowErrors = $state<Record<number, string>>({});

	const COLUMNS: DataTableColumn[] = [
		{ key: 'statement', label: 'Statement' },
		{ key: 'carrier', label: 'Carrier' },
		{ key: 'stated', label: 'Stated' },
		{ key: 'state', label: 'State' }
	];

	let projectParam = $state('');

	async function load(): Promise<void> {
		loadError = null;
		notFound = false;
		try {
			subject = await getConsolidationSubject(id);
		} catch (err) {
			if (err instanceof ApiError && err.status === 404) notFound = true;
			else loadError = err instanceof Error ? err.message : String(err);
			return;
		}
		projectParam =
			page.url.searchParams.get('project') ??
			(subject.project_id !== null ? String(subject.project_id) : '');
		void loadScope(subject.project_id);
		await loadDecisions();
	}

	async function loadScope(projectId: number | null): Promise<void> {
		if (projectId === null) {
			scope = 'Machine-wide';
			return;
		}
		scope = `Project ${projectId}`;
		try {
			const found = (await getFleetProjects()).find((p) => p.project_id === projectId);
			if (found) scope = found.custom_name ?? found.project_path;
		} catch {
			// the numeric fallback stands
		}
	}

	async function loadDecisions(): Promise<void> {
		decisionsError = null;
		if (projectParam === '') {
			decisionsError = 'Open this subject from a project to list its decisions.';
			return;
		}
		try {
			const [all, open] = await Promise.all([
				getConsolidationDecisions({ project: projectParam, subject: id, limit: PAGE_SIZE }),
				getConsolidationDecisions({
					project: projectParam,
					subject: id,
					verdicts: ['conflict_open'],
					open: true,
					limit: 500
				})
			]);
			decisions = all.decisions;
			next = all.next_before;
			conflicted = conflictedStatementIds(open.decisions);
		} catch (err) {
			decisionsError = err instanceof Error ? err.message : String(err);
		}
	}

	async function loadOlder(): Promise<void> {
		if (next === null || loadingOlder) return;
		loadingOlder = true;
		try {
			const more = await getConsolidationDecisions({
				project: projectParam,
				subject: id,
				before: next,
				limit: PAGE_SIZE
			});
			decisions = [...decisions, ...more.decisions];
			next = more.next_before;
		} catch (err) {
			decisionsError = err instanceof Error ? err.message : String(err);
		} finally {
			loadingOlder = false;
		}
	}

	async function act(row: DecisionRow, kind: 'undo' | 'apply'): Promise<void> {
		if (inFlight !== null) return;
		inFlight = row.id;
		const rest = { ...rowErrors };
		delete rest[row.id];
		rowErrors = rest;
		try {
			const updated =
				kind === 'undo'
					? await undoConsolidationDecision(row.id)
					: await applyConsolidationDecision(row.id);
			decisions = decisions.map((d) => (d.id === updated.id ? updated : d));
		} catch (err) {
			rowErrors = {
				...rowErrors,
				[row.id]: refusalFor(kind, err instanceof Error ? err.message : String(err))
			};
		} finally {
			inFlight = null;
		}
	}

	onMount(() => void load());

	const description = $derived(
		subject
			? `${subject.members.toLocaleString('en-US')} ${subject.members === 1 ? 'statement' : 'statements'} in ${subject.carriers.toLocaleString('en-US')} ${subject.carriers === 1 ? 'item' : 'items'} · ${scope}`
			: ''
	);
</script>

<svelte:head>
	<title>{subject ? `${subject.title} — Memory` : 'Subject — Memory'} — mike</title>
</svelte:head>

<section class="subj-page">
	{#if notFound}
		<LoadErrorBanner
			title="No such subject"
			detail={`Subject ${page.params.id} does not exist. It may have been emptied and removed.`}
			backHref={`${resolve('/memory')}#consolidation`}
			backLabel="Consolidation"
		/>
	{:else if loadError}
		<LoadErrorBanner title="Couldn't load this subject" detail={loadError} onRetry={load} />
	{:else if subject}
		<PageIntro level="h1" title={subject.title} {description}>
			{#snippet actions()}
				<Button href={`${resolve('/memory')}#consolidation`} variant="ghost" size="sm">
					<ArrowLeft data-icon="inline-start" aria-hidden="true" />
					Consolidation
				</Button>
			{/snippet}
		</PageIntro>

		<div class="edge mech-group">
			<div>
				<div class="band-hd band-hd--static">
					<span class="machine subj__label">What it says</span>
					<span class="band-hd__meta">
						{#if subject.changed_at || !subject.judged_at}waiting to be judged{:else}judged {formatClock(
								subject.judged_at
							)}{/if}
					</span>
				</div>
				<p class="subj__says">{subject.text}</p>
			</div>

			<div>
				<div class="band-hd band-hd--static">
					<span class="machine subj__label">Statements</span>
					<span class="band-hd__meta">{subject.statements.length}</span>
				</div>
				<DataTable label="Statements" columns={COLUMNS}>
					<tbody>
						{#each subject.statements as s (s.statement_id)}
							{@const mark = STATEMENT_STATES[s.state]}
							{@const retired = isRetired(s.state)}
							{@const inConflict = s.state === 'current' && conflicted.has(s.statement_id)}
							{@const by = carrierOfStatement(subject.statements, s.resolved_by_statement_id)}
							<DataTableRow>
								<DataTableCell tone={retired ? 'quiet' : undefined}>
									<span class="subj__text">{s.text}</span>
								</DataTableCell>
								<DataTableCell><CarrierRef carrier={s.carrier} /></DataTableCell>
								<DataTableCell mono tone="quiet" wrap={false}>
									<Tooltip text={formatFull(s.stated_at)}>
										<span class="subj__nowrap">{formatDayMonth(s.stated_at)}</span>
									</Tooltip>
								</DataTableCell>
								<DataTableCell>
									<span class="subj__state">
										{#if inConflict}
											<StatusMark icon={TriangleAlert} tone="warning" label="In conflict" />
										{:else if mark.tooltip}
											<Tooltip text={mark.tooltip}>
												<StatusMark icon={mark.icon} tone={mark.tone} label={mark.word} />
											</Tooltip>
										{:else}
											<StatusMark icon={mark.icon} tone={mark.tone} label={mark.word} />
										{/if}
										{#if by && (s.state === 'covered' || s.state === 'superseded' || s.state === 'merged')}
											<span class="subj__by">by <CarrierRef carrier={by.carrier} /></span>
										{/if}
									</span>
								</DataTableCell>
							</DataTableRow>
						{/each}
					</tbody>
				</DataTable>
			</div>

			<div>
				<div class="band-hd band-hd--static">
					<span class="machine subj__label">Decisions</span>
				</div>
				{#if decisionsError}
					<div class="subj__pad">
						<LoadErrorBanner
							title="Couldn't load this subject's decisions"
							detail={decisionsError}
							onRetry={loadDecisions}
						/>
					</div>
				{:else if decisions.length === 0}
					<p class="subj__none">No decisions on this subject yet.</p>
				{:else}
					<div class="subj__list">
						{#each decisions as row (row.id)}
							<ChangeRow
								{row}
								project={projectParam}
								showEvidence
								hideSubject
								busy={inFlight === row.id}
								disabled={inFlight !== null}
								error={rowErrors[row.id] ?? null}
								onAction={(kind) => void act(row, kind)}
							/>
						{/each}
					</div>
					{#if next !== null}
						<div class="subj__more">
							<Button variant="outline" size="sm" disabled={loadingOlder} onclick={loadOlder}>
								Load older decisions
							</Button>
						</div>
					{/if}
				{/if}
			</div>
		</div>
	{/if}
</section>

<style>
	.subj-page {
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
	.subj__label {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.subj__says {
		margin: 0;
		padding: 0 var(--space-4) var(--space-4);
		font-size: var(--text-md);
		line-height: var(--leading-body);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.subj__nowrap {
		white-space: nowrap;
	}
	.subj__text {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.subj__state {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
	}
	.subj__by {
		display: inline-flex;
		gap: var(--space-1);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.subj__pad {
		padding: var(--space-3) var(--space-4);
	}
	.subj__none {
		margin: 0;
		padding: 0 var(--space-4) var(--space-3);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.subj__list > :global(* + *) {
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.subj__more {
		display: flex;
		justify-content: center;
		padding: var(--space-3) var(--space-4);
	}
</style>
