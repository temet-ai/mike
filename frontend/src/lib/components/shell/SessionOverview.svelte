<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import { Users } from '@lucide/svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { SESSION_DETAIL_STORE_KEY, SESSION_LIST_STORE_KEY } from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import type { RequestSummary, ToolUseDto } from '$lib/types';
	import { computeDurationMs } from '$lib/time';
	import { listFindingsPaginated, ApiError } from '$lib/api';
	import type { Finding } from '$lib/types';
	import { getSessionEnrolments, listSteerers } from '$lib/api/steering';
	import type { SessionEnrolmentRow } from '$lib/api/steering';
	import type { SteererDto } from '$lib/types';
	import { getSessionFilterRules } from '$lib/api/filterRules';
	import type { SessionFilterRuleDto } from '$lib/api/filterRules';
	import { getSessionAnalyzers } from '$lib/api/analyzers';
	import type { SessionAnalyzerDto } from '$lib/api/analyzers';
	import { deriveAgentDispatchGroups } from '$lib/agentDispatches';
	import DetailSkeleton from '$lib/components/detail/DetailSkeleton.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import FilesChangedCard from '$lib/components/shell/files-changed-card';
	import SessionNarrativeOverview from '$lib/components/shell/SessionNarrativeOverview.svelte';
	import SessionMetrics from '$lib/components/shell/SessionMetrics.svelte';
	import type { AvgLatency } from '$lib/components/shell/SessionMetrics.svelte';
	import type { ValidationState } from '$lib/components/shell/ValidationStatusIndicator.svelte';
	import SessionOverviewSummary from '$lib/components/shell/SessionOverviewSummary.svelte';
	import SessionControlsPanel from '$lib/components/shell/SessionControlsPanel.svelte';
	import AnalysisToolsCard from '$lib/components/shell/AnalysisToolsCard.svelte';
	import SessionMemoryActions from '$lib/components/memory/SessionMemoryActions.svelte';
	import SessionLessonsPanel from '$lib/components/memory/SessionLessonsPanel.svelte';
	import SessionDeliveryPanel from '$lib/components/memory/SessionDeliveryPanel.svelte';
	import SessionReconcilePanel from '$lib/components/shell/SessionReconcilePanel.svelte';
	import TimeSpentPanel from '$lib/components/sessions/TimeSpentPanel.svelte';
	import SessionEmbeddingPanel from '$lib/components/graph/SessionEmbeddingPanel.svelte';
	import SessionMailboxPanel from '$lib/components/shell/SessionMailboxPanel.svelte';
	import SessionLaunchPanel from '$lib/components/shell/SessionLaunchPanel.svelte';
	import RuntimeSessionPanel from '$lib/components/shell/RuntimeSessionPanel.svelte';

	function jumpToRequest(requestId: number): void {
		goto(
			resolve(
				`sessions/${encodeURIComponent(sessionId)}/requests/${requestId}` as `sessions/${string}/requests/${string}`
			)
		);
	}
	import {
		loadSessionToolUses,
		invalidateSessionToolUses
	} from '$lib/stores/cache/sessionToolUses.svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';

	interface Props {
		now: number;
		promptKindCounts?: Record<string, number>;
		validation?: ValidationState;
		validationSince?: string | null;
	}

	let { now, promptKindCounts = {}, validation = null, validationSince = null }: Props = $props();

	const detailStore = getContext<SessionDetailStore>(SESSION_DETAIL_STORE_KEY);
	const sessionListStore = getContext<SessionListStore>(SESSION_LIST_STORE_KEY);

	const sessionId = $derived(page.params.sessionId ?? '');
	const detail = $derived(detailStore?.detail ?? null);
	const summary = $derived(detail?.summary ?? null);
	const requests = $derived<readonly RequestSummary[]>(detail?.requests ?? []);

	const outcomeCounts = $derived.by((): Record<string, number> => {
		const counts: Record<string, number> = {};
		for (const t of detailStore?.operatorTurns ?? []) {
			const k = t.outcome ?? 'unknown';
			counts[k] = (counts[k] ?? 0) + 1;
		}
		return counts;
	});

	const listEntry = $derived.by(() => {
		if (!sessionId || !sessionListStore) return null;
		return sessionListStore.sessions.find((s) => s.id === sessionId) ?? null;
	});
	const project = $derived(listEntry?.project ?? summary?.project ?? null);

	function handleRetry(): void {
		void detailStore?.refresh().then(() => detailStore?.loadAll());
	}

	const requestCount = $derived(summary?.request_count ?? requests.length);

	type StatusKind = 'ok' | 'live' | 'error' | 'muted';
	const statusInfo = $derived.by((): { kind: StatusKind; label: string } => {
		const s = summary?.last_status ?? '';
		if (s === 'Complete') return { kind: 'ok', label: 'Complete' };
		if (s === 'Streaming' || s === 'Pending') return { kind: 'live', label: 'Live' };
		if (s.startsWith('Error')) return { kind: 'error', label: 'Error' };
		return { kind: 'muted', label: s.length > 0 ? s : '—' };
	});

	const modelRows = $derived.by(() => {
		const breakdown = detail?.model_breakdown ?? [];
		if (breakdown.length > 0) return breakdown;
		return (summary?.models ?? []).map((model) => ({ model, count: 0 }));
	});

	const avgLatency = $derived.by((): AvgLatency | null => {
		if (summary?.avg_latency_ms != null) {
			return {
				ms: summary.avg_latency_ms,
				partial: false,
				sampleCount: summary.request_count ?? requests.length
			};
		}
		const samples: number[] = [];
		for (const r of requests) {
			if (r.completed_at === null) continue;
			const ms = computeDurationMs(r.created_at, r.completed_at, now);
			if (ms === null) continue;
			samples.push(ms);
		}
		if (samples.length === 0) return null;
		const total = detail?.total_requests ?? requests.length;
		return {
			ms: samples.reduce((acc, ms) => acc + ms, 0) / samples.length,
			partial: total > requests.length,
			sampleCount: samples.length
		};
	});

	let toolUses = $state<ToolUseDto[]>([]);
	let toolUsesLoading = $state(false);
	let toolUsesError = $state<string | null>(null);
	let toolUsesReloadKey = $state(0);

	function retryToolUses(): void {
		invalidateSessionToolUses(sessionId);
		toolUsesReloadKey += 1;
	}

	$effect(() => {
		const sid = sessionId;
		void toolUsesReloadKey;
		untrack(() => {
			toolUses = [];
			toolUsesError = null;
			if (!sid) {
				toolUsesLoading = false;
				return;
			}
			toolUsesLoading = true;
			loadSessionToolUses(sid)
				.then((rows) => {
					if (sessionId !== sid) return;
					toolUses = rows;
				})
				.catch((err) => {
					if (sessionId !== sid) return;
					toolUsesError = err instanceof ApiError ? err.message : String(err);
					toolUses = [];
				})
				.finally(() => {
					if (sessionId !== sid) return;
					toolUsesLoading = false;
				});
		});
	});

	const WRITE_TOOLS: ReadonlySet<string> = new Set(['Write', 'Edit', 'MultiEdit']);

	const changedFiles = $derived.by((): string[] => {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local: built fresh and returned each recompute, never read by anything else while being mutated
		const set = new Set<string>();
		for (const r of toolUses) {
			if (r.file_path && WRITE_TOOLS.has(r.tool_name)) {
				set.add(r.file_path);
			}
		}
		return Array.from(set).sort();
	});

	const AGENT_SLOT: Record<string, string> = {
		'rust-developer': '1',
		'frontend-developer': '2',
		'ui-designer': '3',
		'db-dev': '4',
		watchdog: '5'
	};
	function agentFgVar(type: string): string | null {
		const slot = AGENT_SLOT[type];
		return slot ? `var(--agent-${slot}-fg)` : null;
	}

	const agentGroups = $derived.by(() => deriveAgentDispatchGroups(toolUses, requests));

	const totalDispatches = $derived(agentGroups.reduce((n, g) => n + g.events.length, 0));

	let enrolments = $state<SessionEnrolmentRow[]>([]);
	let steererDtos = $state<SteererDto[]>([]);
	let enrolmentsReloadKey = $state(0);

	$effect(() => {
		const sid = sessionId;
		void enrolmentsReloadKey;
		untrack(() => {
			enrolments = [];
			if (!sid) return;
			getSessionEnrolments(sid)
				.then((rows) => {
					if (sessionId !== sid) return;
					enrolments = rows;
				})
				.catch(() => {
					if (sessionId !== sid) return;
					enrolments = [];
				});
		});
	});

	$effect(() => {
		listSteerers()
			.then((rows) => {
				steererDtos = rows;
			})
			.catch(() => {
				steererDtos = [];
			});
	});

	$effect(() => {
		const sid = sessionId;
		return onApiCacheUpdate(
			`/api/sessions/${sid}`,
			() => {
				invalidateSessionToolUses(sid);
				toolUsesReloadKey += 1;
				enrolmentsReloadKey += 1;
			},
			{ prefix: true }
		);
	});

	const activeEnrolmentSteererIds = $derived(
		new Set(enrolments.filter((e) => e.disenrolled_at === null).map((e) => e.steerer_id))
	);
	const steerersEnrolledCount = $derived(
		steererDtos.filter((dto) =>
			dto.kind === 'verifier' ? (dto.enabled ?? true) : activeEnrolmentSteererIds.has(dto.id)
		).length
	);
	const steerersTotalCount = $derived(steererDtos.length);

	let findings = $state<Finding[]>([]);
	let findingsLoading = $state(false);
	let findingsError = $state<string | null>(null);

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			findings = [];
			findingsError = null;
			if (!sid) {
				findingsLoading = false;
				return;
			}
			findingsLoading = true;
			listFindingsPaginated({ session_id: sid, limit: 200 })
				.then((envelope) => {
					if (sessionId !== sid) return;
					findings = envelope.findings;
				})
				.catch((err) => {
					if (sessionId !== sid) return;
					findingsError = err instanceof ApiError ? err.message : String(err);
					findings = [];
				})
				.finally(() => {
					if (sessionId !== sid) return;
					findingsLoading = false;
				});
		});
	});
	let filterRuleRows = $state<SessionFilterRuleDto[]>([]);

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			filterRuleRows = [];
			if (!sid) return;
			getSessionFilterRules(sid)
				.then((rows) => {
					if (sessionId !== sid) return;
					filterRuleRows = Array.isArray(rows) ? rows : [];
				})
				.catch(() => {
					if (sessionId !== sid) return;
					filterRuleRows = [];
				});
		});
	});

	let analyzerRows = $state<SessionAnalyzerDto[]>([]);

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			analyzerRows = [];
			if (!sid) return;
			getSessionAnalyzers(sid)
				.then((rows) => {
					if (sessionId !== sid) return;
					analyzerRows = Array.isArray(rows) ? rows : [];
				})
				.catch(() => {
					if (sessionId !== sid) return;
					analyzerRows = [];
				});
		});
	});

	const rulesActiveCount = $derived(filterRuleRows.filter((r) => r.enabled).length);
	const analyzersActiveCount = $derived(analyzerRows.filter((a) => a.enabled).length);

	const showSkel = $derived(
		(detailStore?.loading ?? false) && !detail && !(detailStore?.notFound ?? false)
	);
	const showError = $derived(!!detailStore?.error && !detail);
	const notFound = $derived(detailStore?.notFound ?? false);
	const hasRequests = $derived(requests.length > 0);
</script>

<section class="s-overview" aria-label="Session overview">
	{#if showSkel}
		<div class="p-4">
			<DetailSkeleton />
		</div>
	{:else if showError}
		<div class="p-4">
			<LoadErrorBanner
				title="Couldn't load this session"
				detail={detailStore?.error ?? ''}
				hint="The session may have been evicted by the TTL policy. Pinned sessions are exempt."
				onRetry={handleRetry}
				backHref="/"
				backLabel="Back to sessions"
			/>
		</div>
	{:else if notFound}
		<div class="p-4">
			<LoadErrorBanner
				title="Session not found"
				detail={sessionId}
				hint="This session id is not in the store. It may have been evicted by the TTL policy, or never existed. Pinned sessions are exempt from TTL eviction."
				backHref="/"
				backLabel="Back to sessions"
			/>
		</div>
	{:else if summary}
		<div class="s-scroll">
			{#if detail?.observation}
				<RuntimeSessionPanel
					{sessionId}
					observation={detail.observation}
					initialItems={detail.runtime_items ?? null}
				/>
			{/if}
			{#if !hasRequests && !detail?.observation}
				<p class="s-empty-note">No requests captured yet.</p>
			{:else if hasRequests}
				<div class="s-top-grid">
					<div class="s-tg-main">
						<SessionNarrativeOverview overview={summary?.narrative_overview ?? ''} />

						<SessionOverviewSummary quality={detail?.quality ?? null} {sessionId} />
					</div>
					<div class="s-tg-aside">
						<SessionMetrics
							{requestCount}
							status={statusInfo}
							models={modelRows}
							tokenUsage={detail?.token_usage ?? null}
							firstSeen={summary.first_seen}
							lastSeen={summary.last_seen}
							{avgLatency}
							{promptKindCounts}
							{outcomeCounts}
							{validation}
							{validationSince}
							{now}
						/>
					</div>

					<div class="s-tg-files">
						<FilesChangedCard
							files={changedFiles}
							loading={toolUsesLoading}
							error={toolUsesError}
							onRetry={retryToolUses}
						/>
					</div>

					<div class="s-tg-agents">
						<div class="s-card s-card-no-pad edge edge-cyan">
							<div class="s-card-hd panel-hd s-card-hd-pad">
								<Users size={12} aria-hidden="true" />
								Agents active
								<span class="s-badge">{totalDispatches}</span>
								{#if agentGroups.length > 0}
									<span class="s-badge-note">· {agentGroups.length} types</span>
								{/if}
								<a
									class="s-card-hd-link"
									href={resolve(
										(project
											? `/diagnostics/agent-dispatches?project=${encodeURIComponent(project)}`
											: '/diagnostics/agent-dispatches') as
											`/diagnostics/agent-dispatches?${string}` | '/diagnostics/agent-dispatches'
									)}
									aria-label="View all agent dispatches across sessions"
								>
									All →
								</a>
							</div>
							{#if toolUsesError}
								<div class="s-card-err-pad">
									<LoadErrorBanner
										title="Couldn't load tool uses"
										detail={toolUsesError}
										hint="Files changed and agents active are derived from the same fetch."
										onRetry={retryToolUses}
									/>
								</div>
							{:else if toolUsesLoading}
								<div class="s-agent-skel">
									<div class="s-skel-block" data-motion="wait" aria-hidden="true"></div>
									<div
										class="s-skel-block s-skel-block--short"
										data-motion="wait"
										aria-hidden="true"
									></div>
								</div>
							{:else if agentGroups.length === 0}
								<div class="s-empty-inline s-empty-inline--pad">No agents dispatched</div>
							{:else}
								{#each agentGroups as group (group.subagentType)}
									{@const fgVar = agentFgVar(group.subagentType)}
									<div class="s-agent-group">
										<div class="s-agent-type-row">
											<span class="s-agent-type" style:color={fgVar ?? ''}
												>{group.subagentType}</span
											>
											<span class="s-agent-count">
												{group.events.length}
												{group.events.length === 1 ? 'dispatch' : 'dispatches'}
											</span>
											{#if group.model}
												<span class="s-agent-model">{group.model}</span>
											{:else}
												<span
													class="s-agent-model s-agent-model-unknown"
													title="Model unavailable — no subagent request matched this persona label"
													>—</span
												>
											{/if}
											<span
												class="s-agent-turns-dep"
												title="View agent turns — requires per-request agent attribution (deferred, see dependency-findings.md §D2)"
												aria-disabled="true">turns ↗</span
											>
										</div>
										<div class="s-dispatch-list">
											{#each group.events as event, di (di)}
												<div class="s-dispatch-item">
													<a
														href={resolve(
															`sessions/${encodeURIComponent(sessionId)}/requests/${event.requestId}` as `sessions/${string}/requests/${string}`
														)}
														class="s-dispatch-link"
														aria-label="Go to dispatch request #{event.requestId}"
														title="Go to the request that issued this Task call"
														>#{event.requestId}</a
													>
													<div class="s-dispatch-desc">
														{event.description ?? '(no description)'}
													</div>
												</div>
											{/each}
										</div>
									</div>
								{/each}
							{/if}
						</div>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</section>

<div class="edge mech-group s-mech" data-slot="session-mechanisms">
	{#if summary && hasRequests}
		<SessionControlsPanel
			grouped
			{sessionId}
			{now}
			steerersEnrolled={steerersEnrolledCount}
			steerersTotal={steerersTotalCount}
			gradersActive={0}
			rulesActive={rulesActiveCount}
			analyzersActive={analyzersActiveCount}
			agents={agentGroups}
			onJumpTo={jumpToRequest}
		/>

		<TimeSpentPanel grouped scope="session" {sessionId} sessionFrom={summary.first_seen} />

		<AnalysisToolsCard
			grouped
			{sessionId}
			{now}
			{findings}
			{findingsLoading}
			{findingsError}
			analysisHref={`/analysis?session=${encodeURIComponent(sessionId)}`}
		/>

		<SessionMemoryActions grouped {sessionId} />

		<SessionLessonsPanel grouped {sessionId} />

		<SessionDeliveryPanel grouped {sessionId} />

		<SessionReconcilePanel grouped {sessionId} />

		<SessionEmbeddingPanel grouped {sessionId} />
	{/if}

	<SessionLaunchPanel grouped {sessionId} {now} />

	<SessionMailboxPanel grouped {sessionId} {now} />
</div>

<style>
	.s-overview {
		display: flex;
		flex-direction: column;
		flex: 1 1 auto;
		min-height: 0;
		overflow: hidden;
	}

	.s-scroll {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
	}

	.s-empty-note {
		margin: 16px 12px;
		font-size: 12px;
		color: var(--text-3);
	}

	.s-top-grid {
		display: grid;
		grid-template-columns: 1fr;
		grid-template-areas: 'main' 'aside' 'files' 'agents';
		flex: 0 0 auto;
	}
	.s-tg-main {
		grid-area: main;
	}
	.s-tg-aside {
		grid-area: aside;
	}
	.s-tg-files {
		grid-area: files;
	}
	.s-tg-agents {
		grid-area: agents;
	}
	.s-tg-main,
	.s-tg-aside,
	.s-tg-files,
	.s-tg-agents {
		display: flex;
		flex-direction: column;
		min-width: 0;
		background: var(--surface-card);
	}
	.s-tg-main > :global(* + *) {
		margin-top: -1px;
	}
	.s-tg-main > :global(:last-child),
	.s-tg-aside > :global(:last-child),
	.s-tg-files > :global(:last-child),
	.s-tg-agents > :global(:last-child) {
		flex: 1 1 auto;
	}

	.s-tg-main > :global(:first-child) {
		--edge-br: 0px;
	}
	.s-tg-main > :global(:last-child) {
		--edge-tl: 0px;
		--edge-br: 0px;
	}
	.s-tg-aside > :global(*) {
		--edge-tl: 0px;
		--edge-br: 0px;
	}
	.s-tg-files > :global(*) {
		--edge-tl: 0px;
		--edge-br: 0px;
	}
	.s-tg-agents > :global(*) {
		--edge-tl: 0px;
	}

	@media (min-width: 860px) {
		.s-top-grid {
			grid-template-columns: minmax(0, 1fr) minmax(340px, 1fr);
			grid-template-areas:
				'main aside'
				'files agents';
		}
		.s-tg-aside,
		.s-tg-agents {
			margin-left: -1px;
		}
		.s-tg-files,
		.s-tg-agents {
			margin-top: -1px;
		}
	}

	@media (min-width: 1850px) {
		.s-top-grid {
			grid-template-columns: minmax(0, 820px) minmax(340px, 440px) minmax(320px, 1fr);
			grid-template-areas:
				'main aside files'
				'agents agents agents';
		}
		.s-tg-aside,
		.s-tg-files {
			margin-left: -1px;
		}
		.s-tg-files {
			margin-top: 0;
		}
		.s-tg-agents {
			margin-left: 0;
			margin-top: -1px;
		}
	}

	@media (min-width: 2200px) {
		.s-top-grid {
			grid-template-columns:
				minmax(0, 820px) minmax(340px, 440px)
				minmax(320px, 1fr) minmax(320px, 1fr);
			grid-template-areas: 'main aside files agents';
		}
		.s-tg-aside,
		.s-tg-files,
		.s-tg-agents {
			margin-left: -1px;
		}
		.s-tg-files,
		.s-tg-agents {
			margin-top: 0;
		}
	}

	.s-card {
		--edge: 10px;
		min-width: 0;
	}
	.s-card-no-pad {
		padding: 1px;
	}
	.s-card-hd {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 8px;
	}
	.s-card-hd-pad {
		padding: 12px 12px 10px;
		margin-bottom: 0;
		border-bottom: 1px solid var(--border-subtle);
	}
	.s-badge {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-2);
		font-size: 10px;
		font-variant-numeric: tabular-nums;
	}
	.s-badge-note {
		color: var(--text-faint);
		font-size: 10px;
		text-transform: none;
		letter-spacing: 0;
	}
	.s-card-hd-link {
		margin-left: auto;
		color: var(--status-info-fg);
		font-size: 10px;
		text-transform: none;
		letter-spacing: 0;
		text-decoration: none;
		white-space: nowrap;
	}
	.s-card-hd-link:hover {
		color: var(--text-strong);
		text-decoration: underline;
	}
	.s-card-hd-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}

	.s-agent-group {
		padding: 10px 12px;
		border-top: 1px solid var(--border-subtle);
	}
	.s-agent-group:first-of-type {
		border-top: none;
	}
	.s-agent-type-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 8px;
		margin-bottom: 6px;
	}
	.s-agent-type {
		font-family: var(--font-mono-condensed);
		font-size: 12px;
		font-weight: 600;
		color: var(--text-strong);
	}
	.s-agent-count {
		font-size: 11px;
		color: var(--text-3);
	}
	.s-agent-model {
		font-family: var(--font-mono-condensed);
		font-size: 10px;
		color: var(--text-2);
		background: var(--surface-2);
		padding: 1px 6px;
		border-radius: 2px;
	}
	.s-agent-model-unknown {
		color: var(--text-faint);
		font-style: italic;
	}
	.s-agent-turns-dep {
		margin-left: auto;
		font-size: 10px;
		color: var(--text-faint);
		cursor: not-allowed;
		white-space: nowrap;
	}
	.s-dispatch-list {
		display: flex;
		flex-direction: column;
		gap: 6px;
		border-left: 2px solid var(--border-subtle);
		padding-left: 10px;
	}
	.s-dispatch-item {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.s-dispatch-link {
		font-family: var(--font-mono-condensed);
		font-size: 11px;
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		align-self: flex-start;
	}
	.s-dispatch-link:hover {
		color: var(--text-strong);
		text-decoration-color: var(--status-info-fg);
	}
	.s-dispatch-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.s-dispatch-desc {
		font-size: 11px;
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
		word-break: break-word;
	}

	.s-empty-inline {
		font-size: 11px;
		color: var(--text-3);
	}
	.s-empty-inline--pad {
		padding: 12px;
	}
	.s-card-err-pad {
		padding: 12px;
	}
	.s-skel-block {
		height: 12px;
		border-radius: 2px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
		margin: 4px 0;
	}
	.s-skel-block--short {
		width: 55%;
	}
	.s-agent-skel {
		padding: 12px;
	}

	.s-mech {
		margin-block: var(--space-4, 16px);
		flex: 0 0 auto;
	}
</style>
