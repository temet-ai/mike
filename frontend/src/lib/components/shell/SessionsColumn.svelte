<script lang="ts">
	import { page } from '$app/state';
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { ChevronDown, ChevronsLeft } from '@lucide/svelte';
	import { COLLAPSE_STORE_KEY, SESSION_LIST_STORE_KEY } from '$lib/context-keys';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { groupByProject } from '$lib/grouping';
	import { shortSessionId } from '$lib/format';
	import { formatRelative, parseIso } from '$lib/time';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import PinToggle from '$lib/components/shared/PinToggle.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import ColumnResizer from '$lib/components/shell/ColumnResizer.svelte';
	import StreamingIndicator from '$lib/components/shared/StreamingIndicator.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import {
		SESSIONS_WIDTH_MAX,
		SESSIONS_WIDTH_MIN,
		setSessionsWidth,
		shellState,
		toggleSessionsCollapsed
	} from '$lib/stores/shellState.svelte';

	function isoForLabel(iso: string | null): string {
		const d = parseIso(iso);
		return d ? d.toLocaleString() : '';
	}

	function statusTooltip(kind: string): string {
		switch (kind) {
			case 'complete':
				return 'Completed — request finished, response stored.';
			case 'streaming':
				return 'Streaming response — chunks arriving via SSE.';
			case 'pending':
				return 'Pending — request received, awaiting upstream response.';
			case 'error':
				return 'Errored — see request detail.';
			default:
				return 'Status unknown — backend may be on a newer schema.';
		}
	}

	interface Props {
		now: number;
	}

	let { now }: Props = $props();

	const sessionStore = getContext<SessionListStore>(SESSION_LIST_STORE_KEY);
	const collapseStore = getContext<CollapseStore>(COLLAPSE_STORE_KEY);

	const activeSessionId = $derived.by(() => {
		const match = page.url.pathname.match(/^\/sessions\/([^/]+)/);
		return match && match[1] ? decodeURIComponent(match[1]) : null;
	});

	const sessions = $derived(sessionStore?.sessions ?? []);
	const loading = $derived(sessionStore?.loading ?? false);
	const hasMore = $derived(sessionStore?.hasMore ?? false);
	const loadingMore = $derived(sessionStore?.loadingMore ?? false);
	const loadMoreError = $derived(sessionStore?.loadMoreError ?? null);

	const groups = $derived(groupByProject(sessions));

	function collapseKeyFor(project: string): string {
		return `sidebar:project:${project}`;
	}

	const UNPINNED_VISIBLE_CAP = 5;
	function expandKeyFor(project: string): string {
		return `sidebar:project:expand:${project}`;
	}

	function isCollapsed(project: string): boolean {
		return collapseStore?.isCollapsed(collapseKeyFor(project)) ?? false;
	}

	function isProjectExpanded(project: string): boolean {
		return collapseStore?.isCollapsed(expandKeyFor(project)) ?? false;
	}

	function toggleGroup(project: string): void {
		collapseStore?.toggle(collapseKeyFor(project));
	}

	function toggleProjectExpand(project: string): void {
		collapseStore?.toggle(expandKeyFor(project));
	}

	function handleTogglePin(sessionId: string, nextPinned: boolean): void {
		void sessionStore?.togglePin(sessionId, nextPinned);
	}

	function isProjectPinned(group: { sessions: { pinned?: boolean }[] }): boolean {
		return group.sessions.length > 0 && group.sessions.every((s) => s.pinned === true);
	}

	function handleToggleProjectPin(project: string, nextPinned: boolean): void {
		void sessionStore?.toggleProjectPin(project, nextPinned);
	}

	function handleLoadMore(): void {
		void sessionStore?.loadMore();
	}
</script>

<section class="col sessions" aria-label="Sessions">
	<div class="col-head">
		<h3>Sessions</h3>
		<HelpHint slug="session" />
		<span class="count">{sessions.length}</span>
	</div>

	<div class="col-body" data-scope="sessions">
		{#if sessions.length === 0 && !loading}
			<div class="empty-sessions" data-slot="sessions-empty">
				<p class="empty-sessions__title">No sessions yet</p>
				<p class="empty-sessions__hint">
					Sessions appear here after Claude Code's first captured request.
				</p>
				<p class="empty-sessions__cmd">
					<code>ANTHROPIC_BASE_URL=http://localhost:9988 claude</code>
				</p>
			</div>
		{:else if sessions.length === 0 && loading}
			<div class="p-4 text-center text-xs text-[var(--text-3)]">Loading…</div>
		{:else}
			{#each groups as group (group.project)}
				{@const collapsed = isCollapsed(group.project)}
				{@const projectPinned = isProjectPinned(group)}
				<div class="project-group">
					<div class="project-head-row">
						<Tooltip text={group.project}>
							{#snippet child({ props })}
								<button
									{...props}
									type="button"
									class="project-head"
									aria-expanded={!collapsed}
									aria-controls={`project-body-${group.project}`}
									onclick={() => toggleGroup(group.project)}
								>
									<DisclosureMarker open={!collapsed} size={11} />
									<span class="project-label">{group.project}</span>
									<span class="project-count">{group.sessions.length}</span>
								</button>
							{/snippet}
						</Tooltip>
						<PinToggle
							pinned={projectPinned}
							ariaLabel={projectPinned ? 'Unpin project' : 'Pin project'}
							onToggle={() => handleToggleProjectPin(group.project, !projectPinned)}
						/>
					</div>
					{#if !collapsed}
						{@const decoratedRaw = group.sessions.map((s) => {
							const wsEvent = sessionStore?.latestWsEventForSession(s.id) ?? null;
							const { status, isLive } = deriveSessionActivity(s, wsEvent, now);
							if (isLive) sessionStore?.observeStatus(s.id, status.kind, now);
							const isActivePartition =
								isLive || (sessionStore?.isRecentlyActive(s.id, now) ?? false);
							return { s, status, isLive, isActivePartition };
						})}
						{@const decorated = [
							...decoratedRaw.filter((e) => e.isActivePartition),
							...decoratedRaw.filter((e) => !e.isActivePartition)
						]}
						{@const expanded = isProjectExpanded(group.project)}
						{@const visibleDecorated = expanded
							? decorated
							: decorated.slice(0, UNPINNED_VISIBLE_CAP)}
						{@const hiddenUnpinned = decorated.length - visibleDecorated.length}
						{@const visibleSessions = visibleDecorated}
						<div class="project-body" id={`project-body-${group.project}`}>
							{#each visibleSessions as entry (entry.s.id)}
								{@const session = entry.s}
								{@const status = entry.status}
								{@const isLive = entry.isLive}
								{@const isSelected = session.id === activeSessionId}
								{@const shortId = shortSessionId(session.id)}
								{@const hasTitle = typeof session.title === 'string' && session.title.length > 0}
								{@const isPinned = session.pinned === true}
								{@const lastSeenIso = isoForLabel(session.last_seen)}
								<Tooltip text={session.id}>
									{#snippet child({ props })}
										<a
											{...props}
											href={resolve(`sessions/${encodeURIComponent(session.id)}`)}
											class="session-item block text-inherit no-underline"
											class:selected={isSelected}
											class:session-item--live={isLive}
											aria-current={isSelected ? 'page' : undefined}
										>
											<div class="top">
												<span class="live-slot" aria-hidden={isLive ? undefined : 'true'}>
													{#if isLive}
														<StreamingIndicator label="Active — request in flight" />
													{/if}
												</span>
												{#if hasTitle}
													<span class="name name--title">{session.title}</span>
												{:else}
													<span class="name font-mono">{shortId}</span>
												{/if}
												<span class="top-actions">
													<PinToggle
														pinned={isPinned}
														onToggle={() => handleTogglePin(session.id, !isPinned)}
													/>
													<span
														class="ts"
														title={lastSeenIso ? `Last activity: ${lastSeenIso}` : ''}
														>{formatRelative(session.last_seen, now)}</span
													>
												</span>
											</div>
											<div class="meta">
												{#if hasTitle}
													<span class="font-mono">{shortId}</span>
													<span class="dot-sep">·</span>
												{/if}
												<span title={`${session.request_count} captured requests in this session`}>
													{session.request_count}{session.request_count === 1 ? ' req' : ' reqs'}
												</span>
												<span class="dot-sep">·</span>
												<span
													title={statusTooltip(status.kind)}
													class:text-[var(--status-success-fg)]={status.kind === 'complete'}
													class:text-[var(--status-warning-fg)]={status.kind === 'streaming' ||
														status.kind === 'pending'}
													class:text-[var(--status-danger-fg)]={status.kind === 'error'}
												>
													{status.label.toLowerCase()}
												</span>
											</div>
										</a>
									{/snippet}
								</Tooltip>
							{/each}
							{#if hiddenUnpinned > 0}
								<Tooltip
									text={`${hiddenUnpinned} more ${hiddenUnpinned === 1 ? 'session' : 'sessions'} in this project`}
								>
									{#snippet child({ props })}
										<button
											{...props}
											type="button"
											class="project-expand"
											aria-expanded={expanded}
											onclick={() => toggleProjectExpand(group.project)}
										>
											<ChevronDown size={11} aria-hidden="true" />
											<span>{hiddenUnpinned} more</span>
										</button>
									{/snippet}
								</Tooltip>
							{:else if expanded && decorated.length > UNPINNED_VISIBLE_CAP}
								<Tooltip text="Collapse to the latest 5">
									{#snippet child({ props })}
										<button
											{...props}
											type="button"
											class="project-expand project-expand--collapse"
											aria-expanded={expanded}
											onclick={() => toggleProjectExpand(group.project)}
										>
											<ChevronDown size={11} aria-hidden="true" class="project-expand__caret--up" />
											<span>Show fewer</span>
										</button>
									{/snippet}
								</Tooltip>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
			{#if hasMore}
				<div class="flex flex-col items-stretch border-t border-[var(--border-subtle)] p-3">
					<button
						type="button"
						class="inline-flex items-center justify-center rounded border border-[var(--border-default)] bg-[var(--surface-card)] px-3 py-1.5 text-label uppercase tracking-kpi text-[var(--text-2)] transition-colors hover:border-[var(--interactive-active-fg)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--interactive-focus-ring)] disabled:cursor-not-allowed disabled:opacity-60"
						disabled={loadingMore}
						onclick={handleLoadMore}
					>
						{loadingMore ? 'Loading…' : 'Load older sessions'}
					</button>
					{#if loadMoreError}
						<span class="mt-2 text-caption text-[var(--status-danger-fg)]" role="alert">
							{loadMoreError}
						</span>
					{/if}
				</div>
			{/if}
		{/if}
	</div>

	<div class="col-foot">
		<Tooltip text="Collapse the Sessions column">
			{#snippet child({ props })}
				<button
					{...props}
					type="button"
					class="col-foot__btn"
					aria-label="Collapse Sessions column"
					onclick={toggleSessionsCollapsed}
				>
					<ChevronsLeft size={14} aria-hidden="true" />
				</button>
			{/snippet}
		</Tooltip>
	</div>

	<ColumnResizer
		width={shellState.sessionsWidth}
		min={SESSIONS_WIDTH_MIN}
		max={SESSIONS_WIDTH_MAX}
		ariaLabel="Resize Sessions column"
		onResize={setSessionsWidth}
	/>
</section>

<style>
	.session-item {
		color: inherit;
	}
	.project-group + .project-group {
		border-top: 1px solid var(--border-subtle);
	}
	.project-head-row {
		display: flex;
		align-items: center;
	}
	.project-head-row :global(.is-pinned),
	.project-head-row > :last-child {
		flex-shrink: 0;
		margin-right: 8px;
	}
	.project-head {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 14px 6px 12px;
		background: transparent;
		border: 0;
		color: var(--text-3);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 10px;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		cursor: pointer;
		text-align: left;
		transition:
			color 0.14s,
			background 0.14s;
	}
	.project-head:hover {
		color: var(--text-2);
		background: var(--interactive-hover-surface);
	}
	.project-head:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.project-label {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.project-count {
		flex-shrink: 0;
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}
	.project-body {
		display: flex;
		flex-direction: column;
	}
	.project-expand {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 14px 8px 28px;
		background: transparent;
		border: 0;
		color: var(--text-3);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 10px;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		cursor: pointer;
		text-align: left;
		transition:
			color 0.14s,
			background 0.14s;
	}
	.project-expand:hover {
		color: var(--text-2);
		background: var(--interactive-hover-surface);
	}
	.project-expand:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.project-expand :global(svg) {
		flex-shrink: 0;
		color: var(--text-faint);
		transition: transform 0.14s ease-out;
	}
	.project-expand :global(.project-expand__caret--up) {
		transform: rotate(180deg);
	}
	:global(.session-item .top .name) {
		flex: 1 1 0;
		min-width: 0;
	}
	:global(.session-item .name.name--title) {
		font-size: var(--text-label);
		font-weight: 500;
		font-family: var(--font-sans);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.session-item .top .top-actions) {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}
	.empty-sessions {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 16px 12px;
		text-align: center;
	}
	.empty-sessions__title {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--text-strong);
		margin: 0;
	}
	.empty-sessions__hint {
		font-size: 0.6875rem;
		color: var(--text-3);
		margin: 0;
	}
	.empty-sessions__cmd {
		margin: 4px 0 0;
	}
	.empty-sessions__cmd code {
		display: inline-block;
		padding: 4px 6px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 0.6875rem;
		color: var(--text-1);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		word-break: break-all;
		white-space: pre-wrap;
	}
	:global(.session-item .top .live-slot) {
		flex: 0 0 16px;
		display: inline-flex;
		align-items: center;
	}
	:global(.session-item .meta) {
		padding-left: 16px;
	}
	:global(.session-item.session-item--live) {
		--status-warning-fg: var(--green-live);
		--status-warning-border: var(--green-live-border);
		box-shadow: inset 2px 0 0 0 var(--status-warning-fg);
	}
</style>
