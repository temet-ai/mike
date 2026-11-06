<script lang="ts">
	import { GitCompareArrows, Info, RefreshCw } from '@lucide/svelte';
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import { ApiError } from '$lib/api';
	import {
		getSessionReconcile,
		type ReconcileUsage,
		type ReconcileWireRequest,
		type SessionReconcile
	} from '$lib/api/sessionReconcile';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { createRetryBackoff } from '$lib/retryBackoff';
	import { ORIGIN_CHIP_LABELS, isHousekeepingOrigin } from '$lib/turn-origins';

	interface Props {
		sessionId: string;
		grouped?: boolean;
	}

	interface OriginGroup {
		origin: string;
		label: string;
		count: number;
		quiet: boolean;
		rows: ReconcileWireRequest[];
	}

	let { sessionId, grouped = false }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:reconcile-open`);
	const sectionExpanded = $derived(collapseStore ? collapseStore.isExpanded(sectionKey) : false);

	let data = $state<SessionReconcile | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let attemptedForSession = $state<string | null>(null);
	let statusAnnouncement = $state('');

	const backoff = createRetryBackoff();

	const report = $derived(data?.available ? data.report : null);
	const bothEmpty = $derived(
		report !== null && report.only_in_mod.length === 0 && report.only_on_wire.length === 0
	);

	function originLabel(origin: string): string {
		if (!Object.hasOwn(ORIGIN_CHIP_LABELS, origin)) return origin;
		const label = ORIGIN_CHIP_LABELS[origin as keyof typeof ORIGIN_CHIP_LABELS];
		return origin === 'primary' && label === '' ? 'main' : label || origin;
	}

	const groups = $derived.by<OriginGroup[]>(() => {
		if (!report) return [];
		const byOrigin = new Map<string, ReconcileWireRequest[]>();
		for (const row of report.only_on_wire) {
			const rows = byOrigin.get(row.origin);
			if (rows) rows.push(row);
			else byOrigin.set(row.origin, [row]);
		}
		const built = [...byOrigin.entries()].map(([origin, rows]) => ({
			origin,
			label: originLabel(origin),
			count: report.only_on_wire_by_origin[origin] ?? rows.length,
			quiet: isHousekeepingOrigin(origin),
			rows
		}));
		const rank = (g: OriginGroup): number => (g.quiet ? 2 : g.origin === 'primary' ? 0 : 1);
		return built.sort(
			(a, b) =>
				rank(a) - rank(b) || b.count - a.count || a.label.localeCompare(b.label)
		);
	});

	function requestHref(requestId: string): string {
		return resolve(
			`/sessions/${encodeURIComponent(sessionId)}/requests/${encodeURIComponent(requestId)}` as `/sessions/${string}/requests/${string}`
		);
	}

	function sessionHref(id: string): string {
		return resolve(`/sessions/${encodeURIComponent(id)}` as `/sessions/${string}`);
	}

	async function load(id: string): Promise<void> {
		backoff.cancel();
		loading = true;
		error = null;
		try {
			const result = await getSessionReconcile(id);
			if (id !== sessionId) return;
			data = result;
			backoff.succeeded();
			statusAnnouncement =
				result.available && result.report
					? `Loaded: ${result.report.only_in_mod.length} unmatched steps and ${result.report.only_on_wire.length} unmatched requests.`
					: (result.reason ?? 'The step comparison is not available.');
		} catch (err) {
			if (id !== sessionId) return;
			error = err instanceof ApiError ? err.message : String(err);
			statusAnnouncement = "Couldn't load the step comparison.";
			backoff.failed(() => {
				if (id === sessionId) void load(id);
			});
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const id = sessionId;
		if (!sectionExpanded) return;
		if (attemptedForSession === id) return;
		attemptedForSession = id;
		backoff.reset();
		void load(id);
	});

	$effect(() => backoff.cancel);

	function reload(): void {
		backoff.reset();
		void load(sessionId);
	}
</script>

{#snippet usageCells(usage: ReconcileUsage | null)}
	{#if usage}
		<td class="rc-num">{usage.input_tokens.toLocaleString()}</td>
		<td class="rc-num">{usage.output_tokens.toLocaleString()}</td>
		<td class="rc-num">{usage.cache_read_input_tokens.toLocaleString()}</td>
		<td class="rc-num">{usage.cache_creation_input_tokens.toLocaleString()}</td>
	{:else}
		<td class="rc-none" colspan="4">no usage</td>
	{/if}
{/snippet}

{#snippet reloadFooter()}
	<div class="rc-foot">
		<Button variant="ghost" size="sm" onclick={reload} disabled={loading}>
			<RefreshCw size={12} aria-hidden="true" />
			Reload
		</Button>
	</div>
{/snippet}

<SectionCard
	title="Claude Code steps and captured requests"
	titleIcon={GitCompareArrows}
	collapsible
	bordered={!grouped}
	{sectionKey}
	defaultCollapsed
	titleTooltip="tether records every model step Claude Code took and matches each one to a request mike captured with identical token usage. This lists the ones that did not match."
>
	{#snippet headerActions()}
		{#if report}
			<span class="rc-meta" data-slot="session-reconcile-summary"
				>{report.mod_steps} steps · {report.wire_requests} requests · {report.matched} matched</span
			>
		{/if}
	{/snippet}
	{#snippet children()}
		<span class="sr-only" role="status" aria-live="polite">{statusAnnouncement}</span>
		<div class="rc-body" data-slot="session-reconcile-panel">
			{#if loading && !data}
				<div class="rc-skeleton" aria-hidden="true">
					<div class="rc-skel-row"></div>
					<div class="rc-skel-row rc-skel-row--short"></div>
				</div>
			{/if}
			{#if error}
				<LoadErrorBanner
					title="Couldn't load the step comparison"
					detail={error}
					onRetry={reload}
				/>
			{:else if data && !data.available}
				<p class="rc-note" data-slot="session-reconcile-unavailable">
					{data.reason ?? 'The step comparison is not available.'}
				</p>
				{@render reloadFooter()}
			{:else if report && bothEmpty}
				<p class="rc-note">Every step matched a captured request.</p>
				{@render reloadFooter()}
			{:else if report}
				{#if report.continues}
					<p class="rc-continues">
						Continues session
						<a class="rc-link rc-link--active" href={sessionHref(report.continues)}
							>{report.continues}</a
						>
						<CopyIconButton getText={() => report.continues} label="Copy session id" />
					</p>
				{/if}

				<section class="rc-section" data-slot="session-reconcile-steps">
					<h4 class="panel-subhd">Steps with no captured request ({report.only_in_mod.length})</h4>
					{#if report.mod_steps === 0}
						<p class="rc-empty">The mod recorded no steps for this session.</p>
					{:else if report.only_in_mod.length === 0}
						<p class="rc-empty">Every step matched a captured request.</p>
					{:else}
						<div class="rc-scroll">
							<table class="rc-table">
								<thead>
									<tr>
										<th class="panel-subhd" scope="col">Turn</th>
										<th class="panel-subhd rc-th-num" scope="col">Step</th>
										<th class="panel-subhd" scope="col">Model</th>
										<th class="panel-subhd" scope="col">Agent</th>
										<th class="panel-subhd rc-th-num" scope="col">Input</th>
										<th class="panel-subhd rc-th-num" scope="col">Output</th>
										<th class="panel-subhd rc-th-num" scope="col">Cache read</th>
										<th class="panel-subhd rc-th-num" scope="col">Cache write</th>
									</tr>
								</thead>
								<tbody>
									{#each report.only_in_mod as step (`${step.turn}:${step.index}`)}
										<tr>
											<td class="rc-id">{step.turn}</td>
											<td class="rc-num">{step.index}</td>
											<td class="rc-id">{step.model}</td>
											<td>{step.agent ?? 'main'}</td>
											{@render usageCells(step.usage)}
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					{/if}
				</section>

				<section class="rc-section" data-slot="session-reconcile-requests">
					<div class="rc-subhd-row">
						<h4 class="panel-subhd">
							Captured requests no step explains ({report.only_on_wire.length})
						</h4>
						<Tooltip
							text="Session titles, quota checks and the monitor are calls Claude Code makes on the side. They never appear as steps, so they are listed last."
						>
							<Info size={12} aria-hidden="true" class="rc-help" />
						</Tooltip>
					</div>
					{#if report.only_on_wire.length === 0}
						<p class="rc-empty">Every captured request matched a step.</p>
					{:else}
						<div class="rc-scroll">
							<table class="rc-table">
								<thead>
									<tr>
										<th class="panel-subhd" scope="col">Request</th>
										<th class="panel-subhd" scope="col">Model</th>
										<th class="panel-subhd rc-th-num" scope="col">Input</th>
										<th class="panel-subhd rc-th-num" scope="col">Output</th>
										<th class="panel-subhd rc-th-num" scope="col">Cache read</th>
										<th class="panel-subhd rc-th-num" scope="col">Cache write</th>
									</tr>
								</thead>
								{#each groups as group (group.origin)}
									<tbody class:rc-quiet={group.quiet} data-origin={group.origin}>
										<tr>
											<th scope="rowgroup" colspan="6" class="panel-subhd rc-group-head"
												>{group.label} · {group.count}</th
											>
										</tr>
										{#each group.rows as row (row.request_id)}
											<tr class="rc-row">
												<td class="rc-id">
													<a
														class="rc-link"
														class:rc-link--active={!group.quiet}
														href={requestHref(row.request_id)}>#{row.request_id}</a
													>
												</td>
												<td class="rc-id">{row.model ?? '—'}</td>
												{@render usageCells(row.usage)}
											</tr>
										{/each}
									</tbody>
								{/each}
							</table>
						</div>
					{/if}
				</section>

				{@render reloadFooter()}
			{/if}
		</div>
	{/snippet}
</SectionCard>

<style>
	.rc-meta {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}
	.rc-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.rc-foot {
		display: flex;
		justify-content: flex-end;
	}
	.rc-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.rc-empty {
		margin: var(--space-2) 0 0;
		font-size: var(--text-sm);
		color: var(--text-3);
	}
	.rc-continues {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
	.rc-section {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.rc-section h4 {
		margin: 0;
	}
	.rc-subhd-row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	:global(.rc-help) {
		color: var(--text-3);
		flex-shrink: 0;
	}
	.rc-scroll {
		overflow-x: auto;
	}
	.rc-table {
		width: 100%;
		border-collapse: collapse;
	}
	.rc-table th,
	.rc-table td {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		vertical-align: top;
	}
	.rc-table th {
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.rc-table td {
		font-size: var(--text-sm);
		color: var(--text-1);
		border-bottom: var(--hairline) solid var(--border-subtle);
	}
	.rc-table th.rc-th-num,
	.rc-table td.rc-num {
		text-align: right;
	}
	.rc-table td.rc-num {
		font-family: var(--font-mono, monospace);
		font-variant-numeric: tabular-nums;
	}
	.rc-table td.rc-id {
		font-family: var(--font-mono, monospace);
		overflow-wrap: anywhere;
	}
	.rc-table td.rc-none {
		color: var(--text-3);
		text-align: left;
	}
	.rc-table th.rc-group-head {
		padding-top: var(--space-3);
	}
	.rc-row:hover td {
		background: var(--surface-2);
	}
	.rc-quiet td {
		color: var(--text-3);
	}
	.rc-link {
		color: var(--text-3);
		text-decoration: none;
	}
	.rc-link--active {
		color: var(--interactive-active-fg);
	}
	.rc-link:hover {
		text-decoration: underline;
	}
	.rc-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.rc-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.rc-skel-row {
		height: 14px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
	}
	.rc-skel-row--short {
		width: 60%;
	}
</style>
