<script lang="ts">
	import { resolve } from '$app/paths';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import { shortSessionId } from '$lib/format';
	import { formatDuration, formatRelative, parseIso } from '$lib/time';
	import { deriveSessionActivity } from '$lib/sessionActivity';
	import StreamingIndicator from '$lib/components/shared/StreamingIndicator.svelte';

	interface Props {
		store: SessionListStore;
		now: number;
	}

	const { store, now }: Props = $props();

	const sessions = $derived(store?.sessions ?? []);
	const loading = $derived(store?.loading ?? false);

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

	function isoForLabel(iso: string | null): string {
		const d = parseIso(iso);
		return d ? d.toLocaleString() : '';
	}

	const partitioned = $derived.by(() => {
		const decoratedRaw = sessions.map((s) => {
			const wsEvent = store?.latestWsEventForSession(s.id) ?? null;
			const { status, isLive } = deriveSessionActivity(s, wsEvent, now);
			if (isLive) store?.observeStatus(s.id, status.kind, now);
			const isActivePartition = isLive || (store?.isRecentlyActive(s.id, now) ?? false);
			return { s, status, isLive, isActivePartition };
		});
		return [
			...decoratedRaw.filter((e) => e.isActivePartition),
			...decoratedRaw.filter((e) => !e.isActivePartition)
		];
	});
</script>

<section class="ls-panel" aria-label="Live sessions">
	<h2 class="ls-hd">Live sessions <span class="ls-count">{sessions.length}</span></h2>

	{#if sessions.length === 0 && loading}
		<div class="ls-empty">Loading…</div>
	{:else if sessions.length === 0}
		<div class="ls-empty">Sessions appear here after Claude Code's first captured request.</div>
	{:else}
		<ul class="ls-list" role="list">
			{#each partitioned as entry (entry.s.id)}
				{@const session = entry.s}
				{@const status = entry.status}
				{@const isLive = entry.isLive}
				{@const hasTitle = typeof session.title === 'string' && session.title.length > 0}
				{@const shortId = shortSessionId(session.id)}
				{@const avg = session.avg_latency_ms}
				{@const lastSeenIso = isoForLabel(session.last_seen)}
				<li role="listitem">
					<a
						href={resolve(`/sessions/${encodeURIComponent(session.id)}` as const)}
						class="ls-row"
						class:ls-row--live={isLive}
					>
						<span class="ls-live-slot" aria-hidden={isLive ? undefined : 'true'}>
							{#if isLive}
								<StreamingIndicator label="Active — request in flight" />
							{/if}
						</span>
						<div class="ls-body">
							<div class="ls-top">
								{#if hasTitle}
									<span class="ls-name ls-name--title">{session.title}</span>
								{:else}
									<span class="ls-name ls-name--mono">{shortId}</span>
								{/if}
								<span class="ls-ts" title={lastSeenIso ? `Last activity: ${lastSeenIso}` : ''}
									>{formatRelative(session.last_seen, now)}</span
								>
							</div>
							<div class="ls-meta">
								{#if hasTitle}
									<span class="ls-mono">{shortId}</span>
									<span class="ls-sep">·</span>
								{/if}
								{#if session.project}
									<span class="ls-project">{session.project}</span>
									<span class="ls-sep">·</span>
								{/if}
								<span title={`${session.request_count} captured requests in this session`}>
									{session.request_count}{session.request_count === 1 ? ' req' : ' reqs'}
								</span>
								{#if avg !== null && avg !== undefined}
									<span class="ls-sep">·</span>
									<span title="Session-wide average request latency">avg {formatDuration(avg)}</span
									>
								{/if}
								<span class="ls-sep">·</span>
								<span title={statusTooltip(status.kind)} class="ls-status ls-status--{status.kind}">
									{status.label.toLowerCase()}
								</span>
							</div>
						</div>
					</a>
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.ls-panel {
		display: flex;
		flex-direction: column;
		min-height: 0;
		height: 100%;
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 8px;
		box-shadow: inset 0 1px 0 var(--surface-shine);
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: var(--text-1);
		overflow: hidden;
	}

	.ls-hd {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: 0 0 auto;
		margin: 0;
		padding: 12px 20px;
		font-size: 10px;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-3);
		border-bottom: 1px solid var(--border-subtle);
		font-family: var(--font-display);
	}

	.ls-count {
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.ls-empty {
		padding: 16px 20px;
		font-size: 12px;
		color: var(--text-3);
	}

	.ls-list {
		list-style: none;
		margin: 0;
		padding: 0;
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}

	.ls-row {
		display: flex;
		align-items: flex-start;
		gap: 0;
		padding: 8px 20px;
		text-decoration: none;
		color: inherit;
		border-bottom: 1px solid var(--border-subtle);
		transition: background 80ms ease-out;
	}
	.ls-row:hover {
		background: var(--interactive-hover-surface);
	}
	.ls-row:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
		border-radius: 2px;
	}
	.ls-row--live {
		box-shadow: inset 2px 0 0 0 var(--interactive-active-border);
		background: var(--interactive-active-bg);
	}
	.ls-row--live:hover {
		background: var(--interactive-active-bg);
	}

	.ls-live-slot {
		flex: 0 0 16px;
		display: inline-flex;
		align-items: center;
		padding-top: 4px;
	}

	.ls-body {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.ls-top {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
	}

	.ls-name {
		flex: 1 1 0;
		min-width: 0;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.ls-name--title {
		font-size: 12px;
		font-weight: 500;
		font-family: var(--font-sans);
	}
	.ls-name--mono {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
	}

	.ls-ts {
		flex-shrink: 0;
		font-size: 11px;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}

	.ls-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 6px;
		font-size: 11px;
		color: var(--text-2);
	}

	.ls-mono {
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
	}

	.ls-project {
		color: var(--text-2);
		overflow-wrap: anywhere;
	}

	.ls-sep {
		color: var(--text-faint);
	}

	.ls-status--complete {
		color: var(--status-success-fg);
	}
	.ls-status--streaming,
	.ls-status--pending {
		color: var(--status-warning-fg);
	}
	.ls-status--error {
		color: var(--status-danger-fg);
	}
</style>
