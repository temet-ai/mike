<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import { resolve } from '$app/paths';
	import { Workflow, CheckCircle, Clock } from '@lucide/svelte';
	import { listSessionWorkflowRuns, ApiError } from '$lib/api';
	import type { WorkflowRunDto, WorkflowAgentDto } from '$lib/types';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		sessionId: string;
		parentRequestId: number;
		now: number;
	}

	let { sessionId, parentRequestId, now: _now }: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);

	const sectionOpenKey = $derived(`session:${sessionId}:wf-run:${parentRequestId}:open`);
	function agentOpenKey(agentId: string): string {
		return `session:${sessionId}:wf-agent:${agentId}:open`;
	}

	let sectionExpandedLocal = $state(false);
	let expandedAgentsLocal = $state<Set<string>>(new Set());

	const sectionExpanded = $derived(
		collapseStore ? collapseStore.isCollapsed(sectionOpenKey) : sectionExpandedLocal
	);

	function toggleSection(): void {
		if (collapseStore) collapseStore.toggle(sectionOpenKey);
		else sectionExpandedLocal = !sectionExpandedLocal;
	}

	function isAgentOpen(agentId: string): boolean {
		return collapseStore
			? collapseStore.isCollapsed(agentOpenKey(agentId))
			: expandedAgentsLocal.has(agentId);
	}

	function toggleAgent(agentId: string): void {
		if (collapseStore) {
			collapseStore.toggle(agentOpenKey(agentId));
			return;
		}
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- copy-mutate-reassign pattern: `next` is a throwaway local, never read while being mutated; reactivity comes from replacing `expandedAgentsLocal` below, not from in-place mutation
		const next = new Set(expandedAgentsLocal);
		if (next.has(agentId)) next.delete(agentId);
		else next.add(agentId);
		expandedAgentsLocal = next;
	}

	let allRuns = $state<WorkflowRunDto[]>([]);
	let loading = $state(false);
	let errorMessage = $state<string | null>(null);
	let loadedSessionId = $state<string | null>(null);

	const run = $derived(allRuns.find((r) => r.parent_request_id === parentRequestId) ?? null);

	$effect(() => {
		const id = sessionId;

		function refetch(): void {
			if (!id) return;
			loading = true;
			errorMessage = null;
			listSessionWorkflowRuns(id)
				.then((runs) => {
					if (sessionId !== id) return;
					allRuns = runs;
					loadedSessionId = id;
				})
				.catch((err) => {
					if (sessionId !== id) return;
					errorMessage = err instanceof ApiError ? err.message : String(err);
					allRuns = [];
					loadedSessionId = null;
				})
				.finally(() => {
					if (sessionId !== id) return;
					loading = false;
				});
		}

		untrack(() => {
			if (!id) {
				allRuns = [];
				loading = false;
				errorMessage = null;
				loadedSessionId = null;
				return;
			}
			refetch();
		});

		const unsub = onApiCacheUpdate(
			`/api/sessions/${id}/workflow-runs`,
			() => {
				void refetch();
			},
			{ prefix: true }
		);
		return unsub;
	});

	function runLabel(r: WorkflowRunDto): string {
		if (r.run_uid) return r.run_uid;
		if (r.name) return r.name;
		return `run #${r.run_id}`;
	}

	function isPartial(r: WorkflowRunDto): boolean {
		return r.result_count < r.agent_count;
	}

	function isComplete(r: WorkflowRunDto): boolean {
		return r.agent_count > 0 && r.result_count >= r.agent_count;
	}

	function isFailed(r: WorkflowRunDto): boolean {
		return r.result_count === 0 && r.agent_count > 0;
	}

	function resultJson(agent: WorkflowAgentDto): string {
		if (agent.result_json === null || agent.result_json === undefined) return '';
		try {
			return JSON.stringify(agent.result_json, null, 2);
		} catch {
			return String(agent.result_json);
		}
	}
</script>

<section
	class="wf-band"
	aria-label="Workflow run {parentRequestId}"
	data-augmented-ui="tl-clip br-clip border"
	style="--aug-border-all: 1px; --aug-border-bg: linear-gradient(135deg, var(--data-teal-fg) 0%, var(--data-teal-border) 35%, var(--border-default) 70%);"
>
	<button
		type="button"
		class="wf-header"
		class:wf-header--collapsed={!sectionExpanded}
		aria-expanded={sectionExpanded}
		aria-controls="wf-body-{parentRequestId}"
		onclick={toggleSection}
	>
		<div class="wf-header__left">
			<DisclosureMarker open={sectionExpanded} size={14} />
			<Workflow size={14} aria-hidden="true" class="wf-header-icon" />
			{#if loading && loadedSessionId !== sessionId}
				<span class="wf-header__title">Loading workflow…</span>
			{:else if errorMessage}
				<span class="wf-header__title wf-header__title--error">Workflow (error)</span>
			{:else if run === null}
				<span class="wf-header__title wf-header__title--missing"
					>Workflow run (req #{parentRequestId})</span
				>
			{:else}
				<span class="wf-header__title">{runLabel(run)}</span>
				{#if run.phases.length > 0}
					<div class="wf-phases" aria-label="Phases">
						{#each run.phases as phase (phase)}
							<span class="wf-phase-chip">{phase}</span>
						{/each}
					</div>
				{/if}
				<span class="wf-count">
					{run.agent_count}
					{run.agent_count === 1 ? 'agent' : 'agents'}
					·
					{run.result_count} ✓
				</span>
				{#if isFailed(run)}
					<span class="wf-badge wf-badge--fail" aria-label="All agents failed">failed</span>
				{:else if isPartial(run)}
					<span
						class="wf-badge wf-badge--partial"
						aria-label="{run.agent_count - run.result_count} agents pending"
					>
						{run.agent_count - run.result_count} pending
					</span>
				{:else if isComplete(run)}
					<span class="wf-badge wf-badge--ok" aria-label="All agents complete">complete</span>
				{/if}
			{/if}
		</div>
		<span class="wf-header__hint">{sectionExpanded ? 'Hide' : 'Show'}</span>
	</button>

	{#if sectionExpanded}
		<div id="wf-body-{parentRequestId}" class="wf-body">
			{#if loading && loadedSessionId !== sessionId}
				<div class="wf-status" role="status" aria-live="polite">Loading…</div>
			{:else if errorMessage}
				<div class="wf-status wf-status--error" role="alert">
					Could not load workflow runs: {errorMessage}
				</div>
			{:else if run === null}
				<div class="wf-status wf-status--missing" role="status">
					Run for request #{parentRequestId} not found in session data (endpoint may not be live yet).
				</div>
			{:else}
				{#if run.description}
					<div class="wf-description">{run.description}</div>
				{/if}
				{#if run.agents.length === 0}
					<div class="wf-status" role="status">No agents captured for this run.</div>
				{:else}
					<ul class="wf-agent-list" role="list" aria-label="Workflow agents">
						{#each run.agents as agent (agent.agent_id)}
							{@const open = isAgentOpen(agent.agent_id)}
							{@const json = resultJson(agent)}
							<li class="wf-agent-row" class:wf-agent-row--expanded={open}>
								<button
									type="button"
									class="wf-agent-toggle"
									aria-expanded={open}
									aria-label={open
										? `Collapse agent ${agent.agent_id}`
										: `Expand agent ${agent.agent_id}`}
									onclick={() => toggleAgent(agent.agent_id)}
								>
									<DisclosureMarker {open} size={12} />
								</button>
								<div class="wf-agent-main">
									<div class="wf-agent-top">
										{#if agent.status === 'result'}
											<CheckCircle
												size={12}
												aria-hidden="true"
												class="wf-status-icon wf-status-icon--ok"
											/>
										{:else}
											<Clock
												size={12}
												aria-hidden="true"
												class="wf-status-icon wf-status-icon--pending"
											/>
										{/if}
										{#if agent.agent_type}
											<span class="wf-agent-type">{agent.agent_type}</span>
										{/if}
										<span class="wf-agent-id" title={agent.agent_id}>{agent.agent_id}</span>
										{#if agent.gaps_count > 0}
											<span class="wf-gaps" aria-label="{agent.gaps_count} gaps"
												>{agent.gaps_count} gap{agent.gaps_count === 1 ? '' : 's'}</span
											>
										{/if}
										{#if agent.request_ids.length > 0}
											<span class="wf-req-count"
												>{agent.request_ids.length} req{agent.request_ids.length === 1
													? ''
													: 's'}</span
											>
										{/if}
									</div>
									{#if agent.summary}
										<div class="wf-agent-summary">{agent.summary}</div>
									{/if}
									{#if open}
										<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
										<div class="wf-transcript" aria-label="Agent transcript" tabindex="0">
											{#if agent.request_ids.length > 0}
												<div class="wf-transcript__req-ids">
													<span class="wf-transcript__label">Captured request IDs:</span>
													<span class="wf-transcript__ids">
														{#each agent.request_ids as rid, i (rid)}
															{#if i > 0}<span aria-hidden="true">, </span>{/if}
															<a
																href={resolve(
																	`/sessions/${encodeURIComponent(sessionId)}/requests/${rid}` as `/sessions/${string}/requests/${string}`
																)}
																class="wf-req-link"
																aria-label="Open request #{rid}">#{rid}</a
															>
														{/each}
													</span>
												</div>
											{/if}
											{#if json}
												<div class="wf-result-json">
													<div class="wf-result-json__label">result_json</div>
													<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
													<div
														class="wf-result-json__body"
														tabindex="0"
														aria-label="result_json content"
													>
														<pre class="wf-result-json__pre">{json}</pre>
													</div>
												</div>
											{/if}
											{#if agent.request_ids.length === 0 && !json}
												<div class="wf-transcript__empty">No transcript data yet.</div>
											{/if}
										</div>
									{/if}
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		</div>
	{/if}
</section>

<style>
	.wf-band {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: 12px;
		background: var(--surface-card);
		box-shadow: inset 0 1px 0 var(--surface-shine);
		overflow: hidden;
		margin: 4px 0;
	}

	.wf-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 10px 14px;
		border: none;
		border-bottom: 1px solid var(--border-subtle);
		background: none;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
		gap: 8px;
	}

	.wf-header:hover {
		background: var(--surface-2);
	}

	.wf-header:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.wf-header--collapsed {
		border-bottom: none;
	}

	.wf-header :global(.wf-header-icon) {
		color: var(--data-teal-fg, var(--text-3));
		flex-shrink: 0;
	}

	.wf-header__left {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		flex: 1;
		flex-wrap: wrap;
	}

	.wf-header__title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		white-space: nowrap;
	}

	.wf-header__title--error {
		color: var(--status-danger-fg);
	}

	.wf-header__title--missing {
		color: var(--text-3);
	}

	.wf-header__hint {
		font-size: var(--text-caption);
		color: var(--text-3);
		white-space: nowrap;
		flex-shrink: 0;
	}

	.wf-phases {
		display: flex;
		gap: 4px;
		flex-wrap: wrap;
	}

	.wf-phase-chip {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border-radius: 2px;
		font-size: var(--text-caption);
		font-weight: 500;
		background: var(--surface-2);
		color: var(--text-2);
		border: 1px solid var(--border-subtle);
		white-space: nowrap;
	}

	.wf-count {
		font-size: var(--text-label);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.wf-badge {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		border-radius: 2px;
		font-size: var(--text-caption);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		white-space: nowrap;
	}

	.wf-badge--ok {
		color: var(--status-success-fg);
		background: var(--status-success-bg);
		box-shadow: inset 0 0 0 1px var(--status-success-border);
	}

	.wf-badge--partial {
		color: var(--status-warning-fg);
		background: var(--status-warning-bg);
		box-shadow: inset 0 0 0 1px var(--status-warning-border);
	}

	.wf-badge--fail {
		color: var(--status-danger-fg);
		background: var(
			--status-danger-bg,
			color-mix(in srgb, var(--status-danger-fg) 10%, transparent)
		);
		box-shadow: inset 0 0 0 1px var(--status-danger-border, var(--status-danger-fg));
	}

	.wf-body {
		display: flex;
		flex-direction: column;
	}

	.wf-status {
		padding: 12px 16px;
		font-size: 12px;
		color: var(--text-3);
	}

	.wf-status--error {
		color: var(--status-danger-fg);
	}

	.wf-status--missing {
		color: var(--text-3);
		font-style: italic;
	}

	.wf-description {
		padding: 8px 16px;
		font-size: 12px;
		color: var(--text-2);
		border-bottom: 1px solid var(--border-subtle);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.wf-agent-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.wf-agent-row {
		display: flex;
		align-items: flex-start;
		gap: 6px;
		padding: 8px 12px 8px 14px;
		border-bottom: 1px solid var(--border-subtle);
		border-left: 2px solid var(--border-subtle);
	}

	.wf-agent-row:last-child {
		border-bottom: none;
	}

	.wf-agent-row:hover {
		background: var(--surface-2);
	}

	.wf-agent-row--expanded {
		background: var(--surface-2);
	}

	.wf-agent-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 14px;
		height: 14px;
		flex-shrink: 0;
		margin-top: 1px;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: var(--text-3);
		border-radius: 2px;
	}

	.wf-agent-toggle:hover {
		color: var(--text-strong);
	}

	.wf-agent-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wf-agent-toggle :global(.chev) {
		transition: transform var(--dur-2) var(--ease-io);
	}

	.wf-agent-toggle :global(.chev--open) {
		transform: rotate(90deg);
	}

	.wf-agent-main {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1 1 auto;
		min-width: 0;
	}

	.wf-agent-top {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
		min-width: 0;
	}

	.wf-agent-top :global(.wf-status-icon--ok) {
		color: var(--status-success-fg);
		flex-shrink: 0;
	}

	.wf-agent-top :global(.wf-status-icon--pending) {
		color: var(--text-3);
		flex-shrink: 0;
	}

	.wf-agent-type {
		font-size: var(--text-label);
		font-weight: 600;
		color: var(--data-teal-fg, var(--text-strong));
		white-space: nowrap;
		font-family: var(--font-mono);
	}

	.wf-agent-id {
		font-size: var(--text-caption);
		color: var(--text-3);
		font-family: var(--font-mono);
		white-space: nowrap;
		display: inline-block;
		max-width: 16ch;
		overflow: hidden;
		text-overflow: ellipsis;
		vertical-align: bottom;
	}

	.wf-gaps {
		font-size: var(--text-caption);
		color: var(--status-warning-fg);
		font-weight: 500;
		white-space: nowrap;
	}

	.wf-req-count {
		font-size: var(--text-caption);
		color: var(--text-3);
		white-space: nowrap;
	}

	.wf-agent-summary {
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.wf-transcript {
		margin-top: 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		overflow: auto;
		max-height: 480px;
	}

	.wf-transcript__req-ids {
		display: flex;
		align-items: baseline;
		gap: 6px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-subtle);
		flex-wrap: wrap;
	}

	.wf-transcript__label {
		font-size: var(--text-caption);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.wf-transcript__ids {
		font-size: var(--text-caption);
		color: var(--text-2);
		font-family: var(--font-mono);
		display: flex;
		flex-wrap: wrap;
		gap: 2px;
	}

	.wf-req-link {
		font-size: var(--text-caption);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		white-space: nowrap;
	}

	.wf-req-link:hover {
		color: var(--text-strong);
	}

	.wf-req-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}

	.wf-result-json {
		overflow: auto;
	}

	.wf-result-json__label {
		padding: 6px 12px;
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-bg);
		position: sticky;
		top: 0;
	}

	.wf-result-json__body {
		padding: 10px;
		overflow-x: auto;
	}

	.wf-result-json__pre {
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-1);
		white-space: pre;
		word-break: normal;
		line-height: 1.6;
		margin: 0;
	}

	.wf-transcript__empty {
		padding: 12px;
		font-size: 12px;
		color: var(--text-3);
		font-style: italic;
	}
</style>
