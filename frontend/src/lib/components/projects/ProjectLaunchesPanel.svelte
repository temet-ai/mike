<script lang="ts">
	import { onDestroy, onMount, getContext } from 'svelte';
	import { SquareTerminal } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { getLaunches, killLaunch, LaunchApiError } from '$lib/api/launches';
	import type { LaunchRow } from '$lib/api/launches';
	import type { FleetProjectSummary } from '$lib/fleet/types';
	import { describeResult, pipelineFacts, placementOf } from '$lib/launches/status';
	import { formatRelative } from '$lib/time';
	import { shortSessionId } from '$lib/format';
	import { proxyEvents, MAX_EVENTS } from '$lib/ws.svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		project: FleetProjectSummary;
		nested?: boolean;
	}

	const { project, nested = false }: Props = $props();
	const projectPath = $derived(project.project_path);

	const THROTTLE_MS = 500;

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`project:${projectPath}:launches`);

	let allRows = $state<LaunchRow[]>([]);
	let loading = $state(true);
	let errorMessage = $state<string | null>(null);
	let openOnly = $state(false);
	let now = $state(Date.now());
	let tickTimer: ReturnType<typeof setInterval> | null = null;

	async function load(): Promise<void> {
		loading = true;
		errorMessage = null;
		try {
			allRows = await getLaunches();
		} catch (err) {
			errorMessage = err instanceof LaunchApiError ? err.message : String(err);
		} finally {
			loading = false;
		}
	}

	const projectRows = $derived(
		allRows.filter((r) => r.folder === projectPath || r.repo_root === projectPath)
	);

	const sortedRows = $derived(
		[...projectRows].sort((a, b) => {
			const byTime = Date.parse(b.created_at) - Date.parse(a.created_at);
			return byTime !== 0 ? byTime : b.id - a.id;
		})
	);
	const visibleRows = $derived(
		openOnly ? sortedRows.filter((r) => r.closed_at === null) : sortedRows
	);

	let expandedLocal = $state<Set<number>>(new Set());
	function isExpanded(id: number): boolean {
		const key = `project:${projectPath}:launch-row:${id}`;
		return collapseStore ? collapseStore.isExpanded(key) : expandedLocal.has(id);
	}
	function toggleRow(id: number): void {
		const key = `project:${projectPath}:launch-row:${id}`;
		if (collapseStore) {
			collapseStore.toggle(key);
			return;
		}
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- cloned, mutated, then reassigned wholesale; reactivity comes from the reassignment below
		const next = new Set(expandedLocal);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		expandedLocal = next;
	}

	let confirmKillId = $state<number | null>(null);
	let killingId = $state<number | null>(null);
	let killErrors = $state<Record<number, string>>({});

	function beginKillConfirm(id: number): void {
		confirmKillId = id;
		if (killErrors[id]) {
			const next = { ...killErrors };
			delete next[id];
			killErrors = next;
		}
	}
	function cancelKillConfirm(): void {
		confirmKillId = null;
	}
	async function performKill(row: LaunchRow): Promise<void> {
		if (!row.session_uuid) return;
		killingId = row.id;
		try {
			await killLaunch(row.session_uuid);
			confirmKillId = null;
			await load();
		} catch (err) {
			killErrors = {
				...killErrors,
				[row.id]: err instanceof LaunchApiError ? err.message : String(err)
			};
		} finally {
			killingId = null;
		}
	}

	let processedEvents = $state(0);
	let refetchTimer: ReturnType<typeof setTimeout> | null = null;
	function scheduleRefetch(): void {
		if (refetchTimer !== null) clearTimeout(refetchTimer);
		refetchTimer = setTimeout(() => {
			refetchTimer = null;
			void load();
		}, THROTTLE_MS);
	}
	$effect(() => {
		const total = proxyEvents.totalEvents;
		if (total === processedEvents) return;
		const start = total < processedEvents ? 0 : Math.max(processedEvents, total - MAX_EVENTS);
		for (let i = start; i < total; i++) {
			const ev = proxyEvents.at(i);
			if (!ev) continue;
			if (ev.type === 'Lagged') {
				scheduleRefetch();
				continue;
			}
			if (ev.type === 'RowChanged' && ev.table === 'launch_launches') scheduleRefetch();
		}
		processedEvents = total;
	});

	onMount(() => {
		void load();
		tickTimer = setInterval(() => {
			now = Date.now();
		}, 1000);
	});
	onDestroy(() => {
		if (tickTimer !== null) clearInterval(tickTimer);
		if (refetchTimer !== null) clearTimeout(refetchTimer);
	});

	function absoluteTitle(iso: string | null): string | undefined {
		return iso ?? undefined;
	}
</script>

<div data-slot="project-launches-panel">
	<SectionCard
		title="Launches"
		titleIcon={SquareTerminal}
		ariaLabel="Project launches"
		collapsible
		bordered={!nested}
		{sectionKey}
		defaultCollapsed
		titleTooltip="Every terminal launch mike's own launcher has attempted into this project's working directory — folder, backend, session, placement, and the channel-check / opened / delivered / confirmed / result pipeline for each."
	>
		{#snippet headerActions()}
			{#if !loading && !errorMessage}
				<span class="plp-count"
					>{visibleRows.length} {visibleRows.length === 1 ? 'launch' : 'launches'}</span
				>
			{/if}
		{/snippet}
		{#snippet children()}
			<div class="plp-toolbar">
				<label class="plp-toggle">
					<input type="checkbox" bind:checked={openOnly} />
					<span>Open only</span>
				</label>
			</div>

			{#if errorMessage}
				<LoadErrorBanner
					title="Couldn't load this project's launches"
					detail={errorMessage}
					onRetry={() => void load()}
				/>
			{:else if loading && allRows.length === 0}
				<div class="plp-status" role="status" aria-live="polite">Loading...</div>
			{:else if visibleRows.length === 0}
				<FilteredEmptyState
					icon={SquareTerminal}
					filtered={openOnly}
					emptyText="No launches recorded for this project"
					emptySubtext="A launch appears here as soon as mike's launcher starts one into this project's directory — mike spawn open, or POST /api/launches."
					filteredText="No launches are currently open."
					filteredSubtext="Every recorded launch for this project has closed. Turn off “Open only” to see the full history."
					onClearFilters={() => (openOnly = false)}
					clearLabel="Show all"
				/>
			{:else}
				<ul class="plp-list" role="list">
					{#each visibleRows as row (row.id)}
						{@const open = isExpanded(row.id)}
						{@const result = describeResult(row.result)}
						{@const bodyId = `plp-body-${row.id}`}
						{@const canKill = row.session_uuid !== null && row.closed_at === null}
						<li class="plp-row" class:plp-row--expanded={open}>
							<div class="plp-row__head">
								<button
									type="button"
									class="plp-row__disclose"
									aria-expanded={open}
									aria-controls={bodyId}
									aria-label={open ? `Collapse launch #${row.id}` : `Expand launch #${row.id}`}
									onclick={() => toggleRow(row.id)}
								>
									<DisclosureMarker {open} size={12} />
									<span class="plp-backend">{row.backend}</span>
									<span class="plp-fact plp-fact--{result.tone}" title={result.title}>
										<result.icon size={13} aria-hidden="true" />
										<span>{result.label}</span>
									</span>
									<span class="plp-ts" title={absoluteTitle(row.created_at)}
										>{formatRelative(row.created_at, now)}</span
									>
								</button>
								<div class="plp-row__meta">
									{#if row.session_uuid}
										<a
											class="plp-link"
											href={resolve(`sessions/${encodeURIComponent(row.session_uuid)}`)}
										>
											{shortSessionId(row.session_uuid)}
										</a>
									{/if}
									{#if canKill}
										{#if confirmKillId === row.id}
											<span class="plp-confirm" role="group" aria-label="Confirm kill">
												<span class="plp-confirm__text">Close this surface?</span>
												<Button
													variant="destructive"
													size="sm"
													disabled={killingId === row.id}
													onclick={() => void performKill(row)}
												>
													{killingId === row.id ? 'Killing...' : 'Confirm'}
												</Button>
												<Button
													variant="outline"
													size="sm"
													onclick={cancelKillConfirm}
													disabled={killingId === row.id}
												>
													Cancel
												</Button>
											</span>
										{:else}
											<Button variant="outline" size="sm" onclick={() => beginKillConfirm(row.id)}
												>Kill</Button
											>
										{/if}
									{/if}
								</div>
							</div>
							{#if killErrors[row.id]}
								<p class="plp-row__error" role="alert">{killErrors[row.id]}</p>
							{/if}

							<div class="plp-row__body" class:plp-row__body--open={open} id={bodyId}>
								<div class="plp-row__body-clip" inert={!open}>
									<div class="plp-detail">
										<div class="plp-facts" role="list" aria-label="Launch pipeline status">
											{#each pipelineFacts(row) as d (d.label)}
												<span class="plp-fact plp-fact--{d.tone}" role="listitem" title={d.title}>
													<d.icon size={13} aria-hidden="true" />
													<span>{d.label}</span>
												</span>
											{/each}
										</div>

										{#if row.reason}
											<div class="plp-block">
												<span class="plp-block__label">Reason</span>
												<p class="plp-pre-text">{row.reason}</p>
											</div>
										{/if}

										<div class="plp-grid">
											<div class="plp-cell">
												<span class="plp-cell__k">Folder</span><span class="plp-cell__v"
													>{row.folder}</span
												>
											</div>
											{#if row.repo_root}
												<div class="plp-cell">
													<span class="plp-cell__k">Repo root</span><span class="plp-cell__v"
														>{row.repo_root}</span
													>
												</div>
											{/if}
											<div class="plp-cell">
												<span class="plp-cell__k">Placement</span><span class="plp-cell__v"
													>{placementOf(row)}</span
												>
											</div>
											{#if row.model}
												<div class="plp-cell">
													<span class="plp-cell__k">Model</span><span class="plp-cell__v"
														>{row.model}</span
													>
												</div>
											{/if}
											{#if row.agent}
												<div class="plp-cell">
													<span class="plp-cell__k">Agent</span><span class="plp-cell__v"
														>{row.agent}</span
													>
												</div>
											{/if}
											<div class="plp-cell">
												<span class="plp-cell__k">Created</span><span
													class="plp-cell__v"
													title={absoluteTitle(row.created_at)}
													>{formatRelative(row.created_at, now)}</span
												>
											</div>
											<div class="plp-cell">
												<span class="plp-cell__k">Closed</span><span
													class="plp-cell__v"
													title={absoluteTitle(row.closed_at)}
													>{row.closed_at ? formatRelative(row.closed_at, now) : '—'}</span
												>
											</div>
											{#if row.close_reason}
												<div class="plp-cell">
													<span class="plp-cell__k">Close reason</span><span class="plp-cell__v"
														>{row.close_reason}</span
													>
												</div>
											{/if}
										</div>

										{#if row.claude_args.length > 0}
											<div class="plp-block">
												<span class="plp-block__label">claude argv (full)</span>
												<!-- svelte-ignore a11y_no_noninteractive_tabindex (axe scrollable-region-focusable requires the overflow container be keyboard-focusable) -->
												<pre
													class="plp-pre"
													tabindex="0"
													role="region"
													aria-label="Complete claude arguments (scrollable)">{row.claude_args.join(
														'\n'
													)}</pre>
											</div>
										{/if}

										{#if row.prompt_present}
											<div class="plp-block">
												<span class="plp-block__label">
													Brief {row.brief_mode
														? `(${row.brief_mode}${row.brief_path ? `: ${row.brief_path}` : ''})`
														: ''}
												</span>
												{#if row.brief_text}
													<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
													<pre
														class="plp-pre"
														tabindex="0"
														role="region"
														aria-label="Full brief text (scrollable)">{row.brief_text}</pre>
												{:else}
													<p class="plp-note">
														A brief was sent, but its full text is not on this row.
													</p>
												{/if}
											</div>
										{/if}

										{#if row.stderr_text}
											<div class="plp-block">
												<span class="plp-block__label">
													Backend stderr{row.stderr_truncated
														? ' (capped at 64 KiB — more was cut)'
														: ''}
												</span>
												<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
												<pre
													class="plp-pre"
													tabindex="0"
													role="region"
													aria-label="Backend stderr (scrollable)">{row.stderr_text}</pre>
											</div>
										{/if}
									</div>
								</div>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		{/snippet}
	</SectionCard>
</div>

<style>
	.plp-count {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.plp-toolbar {
		display: flex;
		align-items: center;
		margin-bottom: var(--space-2, 8px);
	}
	.plp-toggle {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-2);
		cursor: pointer;
	}
	.plp-toggle input {
		accent-color: var(--interactive-active-fg);
	}

	.plp-status {
		padding: 8px 0;
		font-size: 12px;
		color: var(--text-3);
	}

	.plp-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}

	.plp-row {
		--row-bg: transparent;
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-subtle);
		background: var(--row-bg);
		transition: background var(--dur-1) var(--ease-snap);
	}
	.plp-row:last-child {
		border-bottom: none;
	}
	.plp-row:hover {
		--row-bg: var(--surface-2);
	}
	.plp-row--expanded {
		--row-bg: var(--surface-2);
	}

	.plp-row__head {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		flex-wrap: wrap;
		min-width: 0;
		box-sizing: border-box;
		padding: 6px 4px;
		min-height: var(--control-h);
		border-bottom: 1px solid transparent;
	}
	.plp-row--expanded .plp-row__head {
		border-bottom-color: var(--border-subtle);
	}

	.plp-row__disclose {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
		flex: 1 1 auto;
		min-width: 0;
		background: none;
		border: none;
		border-radius: 0;
		padding: 0;
		margin: 0;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.plp-row__disclose:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.plp-row__meta {
		display: flex;
		align-items: center;
		gap: 10px;
		flex: 0 0 auto;
		flex-wrap: wrap;
		min-height: 24px;
	}

	.plp-backend {
		--aug-tl: 4px;
		--aug-br: 4px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: var(--text-label, 10px);
		letter-spacing: 0.04em;
		color: var(--data-teal-fg);
		white-space: nowrap;
		padding: 2px 7px;
		background: var(--data-teal-bg);
	}

	.plp-ts {
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
		margin-left: auto;
	}

	.plp-link {
		font-size: var(--text-caption, 11px);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		white-space: nowrap;
	}
	.plp-link:hover {
		color: var(--text-strong);
		text-decoration-color: var(--status-info-fg);
	}
	.plp-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}

	.plp-fact {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: var(--text-caption, 11px);
		white-space: nowrap;
	}
	.plp-fact--success {
		color: var(--status-success-fg);
	}
	.plp-fact--warning {
		color: var(--status-warning-fg);
	}
	.plp-fact--danger {
		color: var(--status-danger-fg);
	}
	.plp-fact--info {
		color: var(--status-info-fg);
	}
	.plp-fact--muted {
		color: var(--text-3);
	}

	.plp-confirm {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	.plp-confirm__text {
		font-size: var(--text-caption, 11px);
		color: var(--status-danger-fg);
	}

	.plp-row__error {
		margin: 0;
		padding: 0 4px 6px;
		font-size: 11px;
		color: var(--status-danger-fg);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.plp-row__body {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows var(--dur-2) var(--ease-io);
	}
	.plp-row__body--open {
		grid-template-rows: 1fr;
	}
	.plp-row__body-clip {
		overflow: hidden;
	}

	.plp-detail {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 10px 4px 12px;
	}

	.plp-facts {
		display: flex;
		flex-wrap: wrap;
		gap: 14px;
	}

	.plp-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 8px 16px;
	}
	.plp-cell {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.plp-cell__k {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.plp-cell__v {
		font-size: 12px;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.plp-block {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.plp-block__label {
		font-size: var(--text-label, 10px);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.plp-note {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}

	.plp-pre-text {
		margin: 0;
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
		white-space: pre-wrap;
	}

	.plp-pre {
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-label, 11px);
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-wrap: anywhere;
		line-height: 1.55;
		margin: 0;
		padding: calc(var(--edge-sm, 4px) + var(--space-1, 4px));
		background: var(--surface-bg);
		border-radius: 0;
		max-height: 420px;
		overflow-y: auto;
	}
</style>
