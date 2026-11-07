<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { createLiveSteeringStore } from '$lib/state/liveSteering.svelte';
	import type { LiveInterventionEvent, LiveSteeringStore } from '$lib/state/liveSteering.svelte';
	import { VERDICT_LABELS } from '$lib/steering';
	import type { Verdict } from '$lib/steering';
	import { Button } from '$lib/components/ui/button';
	import { Radio, Activity, ArrowRight, ChevronUp, ChevronDown } from '@lucide/svelte';

	interface Props {
		now: number;
		store?: LiveSteeringStore;
	}

	let { now, store: storeProp }: Props = $props();

	// svelte-ignore state_referenced_locally
	const ownsStore = storeProp === undefined;
	// svelte-ignore state_referenced_locally
	const store = storeProp ?? createLiveSteeringStore();

	onMount(() => {
		if (!ownsStore) return;
		const stop = store.start();
		return stop;
	});

	const wsDown = $derived(store.wsState !== 'connected' && store.wsState !== 'reconnecting');
	const feed = $derived(store.feed);
	const perSteerer = $derived(Array.from(store.perSteerer.values()));

	let expanded = $state(new Set<number>());

	function toggleRow(key: number): void {
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- `next` is a plain local copy, mutated then reassigned wholesale to `expanded` below; never mutated in place, so plain reactivity already tracks it.
		const next = new Set(expanded);
		if (next.has(key)) {
			next.delete(key);
		} else {
			next.add(key);
		}
		expanded = next;
	}

	function relativeTime(ms: number): string {
		const diffSec = Math.max(0, Math.floor((now - ms) / 1000));
		if (diffSec < 60) return `${diffSec}s ago`;
		const diffMin = Math.floor(diffSec / 60);
		if (diffMin < 60) return `${diffMin}m ago`;
		const diffHr = Math.floor(diffMin / 60);
		return `${diffHr}h ago`;
	}

	function verdictLabel(v: string | null): string {
		if (!v) return 'unknown';
		return VERDICT_LABELS[v as Verdict] ?? v;
	}

	interface Accent {
		fg: string;
		bg: string;
		border: string;
	}

	function verdictAccent(v: string | null): Accent {
		switch (v) {
			case 'fired':
				return {
					fg: 'var(--status-success-fg)',
					bg: 'var(--status-success-bg)',
					border: 'var(--status-success-border)'
				};
			case 'failed_closed':
				return {
					fg: 'var(--status-danger-fg)',
					bg: 'var(--status-danger-bg)',
					border: 'var(--status-danger-border)'
				};
			default:
				return { fg: 'var(--text-3)', bg: 'var(--surface-2)', border: 'var(--border-subtle)' };
		}
	}

	function displayName(ev: LiveInterventionEvent): string {
		return ev.steererDisplayName ?? ev.steererName ?? 'unknown';
	}
</script>

<div
	class="sm-panel"
	data-slot="steering-monitor"
	data-augmented-ui="tl-clip tr-clip br-clip bl-clip border"
>
	{#if wsDown}
		<div class="sm-ws-banner" role="alert">
			<span class="sm-banner-dot" aria-hidden="true"></span>
			Disconnected — live steering feed paused. Reconnecting…
		</div>
	{/if}

	<div class="sm-header">
		<span class="sm-live-dot" class:sm-live-dot--down={wsDown} aria-hidden="true"></span>
		<span class="sm-title-backing">
			<span class="sm-title" data-augmented-ui="tl-clip br-clip border">
				Live steering monitor
			</span>
		</span>
		<span class="sm-enrolled-backing">
			<span class="sm-enrolled-badge" data-augmented-ui="tl-clip br-clip border">
				{store.enrolledSessionCount} session{store.enrolledSessionCount === 1 ? '' : 's'} enrolled
			</span>
		</span>
	</div>

	{#if perSteerer.length > 0}
		<div class="sm-steerer-strip">
			{#each perSteerer as s (s.steererName)}
				<div class="sm-steerer-row">
					<span class="sm-steerer-name">{s.displayName ?? s.steererName}</span>
					<span class="sm-steerer-stat sm-stat--fire">
						{s.fires} fire{s.fires === 1 ? '' : 's'}
					</span>
					{#if s.suppressions > 0}
						<span class="sm-steerer-stat sm-stat--supp">
							{s.suppressions} suppressed
						</span>
					{/if}
					{#if s.noDrifts > 0}
						<span class="sm-steerer-stat sm-stat--muted">
							{s.noDrifts} no-drift
						</span>
					{/if}
					{#if s.lastFireAt}
						<span class="sm-steerer-stat sm-stat--time">
							last: {new Date(s.lastFireAt).toLocaleTimeString(undefined, {
								hour: '2-digit',
								minute: '2-digit',
								second: '2-digit'
							})}
						</span>
					{/if}
				</div>
			{/each}
		</div>
	{/if}

	{#if store.awaitingFirstEvent}
		{#if store.enrolledSessionCount === 0}
			<div class="sm-empty-state">
				<div class="sm-empty-state__icon" aria-hidden="true">
					<Radio size={28} />
				</div>
				<h3 class="sm-empty-state__heading">No sessions enrolled for steering</h3>
				<p class="sm-empty-state__body">
					Enrol a session in a steerer to start receiving live intervention events here.
				</p>
				<Button variant="outline" href={resolve('/steering?tab=fleet')}>
					<span>Go to Fleet</span>
					<ArrowRight class="size-3.5" aria-hidden="true" />
				</Button>
				<a href={resolve('/help/guides/steering-concepts')} class="sm-empty-state__learn">
					What does an event look like?
				</a>
			</div>
		{:else}
			<div class="sm-awaiting">
				<div class="sm-awaiting__icon" aria-hidden="true">
					<Activity size={22} />
				</div>
				<p class="sm-await-text">
					Watching {store.enrolledSessionCount} enrolled session{store.enrolledSessionCount === 1
						? ''
						: 's'}…
				</p>
				<p class="sm-await-sub">Events appear here as steerers evaluate requests.</p>
			</div>
		{/if}
	{:else if feed.length === 0}
		<p class="sm-empty">No steering events in the current window.</p>
	{:else}
		<div class="sm-feed" role="list" aria-label="Live steering interventions">
			{#each feed as ev (ev.key)}
				{@const isExpanded = expanded.has(ev.key)}
				{@const isFired = ev.verdict === 'fired'}
				{@const accent = verdictAccent(ev.verdict)}
				<div class="sm-feed-row" role="listitem">
					<div
						class="sm-rail"
						style="background: {isFired ? 'var(--status-success-fg)' : 'var(--border-subtle)'};"
						aria-hidden="true"
					></div>

					<div class="sm-row-collapsed">
						<button
							type="button"
							class="sm-row-toggle"
							onclick={() => toggleRow(ev.key)}
							aria-expanded={isExpanded}
							aria-label="Toggle steering intervention detail"
						></button>

						<div class="sm-row-chrome">
							<span class="sm-chip-backing">
								<span
									class="sm-chip"
									data-augmented-ui="tl-clip br-clip border"
									style="--acc-fg: {accent.fg}; --acc-bg: {accent.bg}; --aug-border-bg: {accent.border};"
								>
									{verdictLabel(ev.verdict)}
								</span>
							</span>

							<span
								class="sm-steerer-label"
								style="color: {isFired ? 'var(--status-success-fg)' : 'var(--text-2)'};"
							>
								{displayName(ev)}
							</span>

							<a
								href={resolve(`sessions/${encodeURIComponent(ev.sessionId)}#steering` as const)}
								class="sm-session-link"
							>
								session →
							</a>

							<span class="sm-row-time">{relativeTime(ev.receivedAt)}</span>

							<span class="sm-expand-cue" aria-hidden="true">
								{#if isExpanded}
									<ChevronUp size={12} />
								{:else}
									<ChevronDown size={12} />
								{/if}
							</span>
						</div>

						{#if ev.offThreadTopic}
							<p class="sm-topic">{ev.offThreadTopic}</p>
						{/if}
					</div>

					{#if isExpanded && ev.payloadText}
						<div class="sm-row-body">
							<p class="sm-body-label">Injected payload</p>
							<pre class="sm-body-pre">{ev.payloadText}</pre>
							<a
								href={resolve(`sessions/${encodeURIComponent(ev.sessionId)}#steering` as const)}
								class="sm-full-record-link"
							>
								→ Full record in session view
							</a>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.sm-panel {
		--edge: 14px;
		--aug-tl: var(--edge);
		--aug-tr: var(--edge);
		--aug-br: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1.5px;
		--aug-border-bg: linear-gradient(
			90deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 26%,
			var(--border-default) 55%
		);
		padding: calc(var(--edge) + 4px) calc(var(--edge) + 8px) calc(var(--edge) + 6px)
			calc(var(--edge) + 4px);
		background:
			linear-gradient(105deg, var(--status-danger-bg), transparent 34%), var(--surface-bg);
		display: flex;
		flex-direction: column;
		gap: 12px;
		height: 100%;
		min-height: 0;
	}

	.sm-ws-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		border: 1px solid var(--status-warning-border);
		background: var(--status-warning-bg);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--status-warning-fg);
	}

	.sm-banner-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--status-warning-fg);
		flex-shrink: 0;
	}

	.sm-header {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
	}

	.sm-live-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--status-success-fg);
		box-shadow: 0 0 6px var(--status-success-fg);
		flex-shrink: 0;
	}
	.sm-live-dot--down {
		background: var(--status-danger-fg);
		box-shadow: none;
	}

	.sm-title-backing,
	.sm-enrolled-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}

	.sm-title {
		--aug-tl: 6px;
		--aug-tr: 0;
		--aug-br: 6px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-danger-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--status-danger-fg);
		padding: 3px 8px;
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-bg);
	}

	.sm-enrolled-badge {
		--aug-tl: 6px;
		--aug-tr: 0;
		--aug-br: 6px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-2);
		padding: 3px 8px;
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-bg);
	}

	.sm-steerer-strip {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
	}

	.sm-steerer-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.sm-steerer-name {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs, 12px);
		font-weight: 600;
		color: var(--status-danger-fg);
		flex-shrink: 0;
	}

	.sm-steerer-stat {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		flex-shrink: 0;
	}

	.sm-stat--fire {
		color: var(--status-success-fg);
	}

	.sm-stat--supp {
		color: var(--text-2);
	}

	.sm-stat--muted {
		color: var(--text-3);
	}

	.sm-stat--time {
		color: var(--text-3);
		margin-left: auto;
	}

	.sm-empty {
		padding: 16px 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs, 12px);
		color: var(--text-3);
		margin: 0;
	}

	.sm-empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 40px 16px;
		text-align: center;
	}

	.sm-empty-state__icon {
		color: var(--text-3);
		margin-bottom: 4px;
	}

	.sm-empty-state__heading {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		letter-spacing: -0.02em;
		font-size: 14px;
		color: var(--text-strong);
	}

	.sm-empty-state__body {
		margin: 0;
		font-size: var(--text-xs, 12px);
		color: var(--text-2);
		line-height: 1.5;
		max-width: 280px;
	}

	.sm-empty-state__learn {
		font-size: var(--text-label);
		color: var(--text-3);
		text-decoration: underline;
		text-underline-offset: 2px;
		transition: color 0.15s;
	}

	.sm-empty-state__learn:hover {
		color: var(--text-1);
	}

	.sm-awaiting {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 32px 16px;
		text-align: center;
	}

	.sm-awaiting__icon {
		color: var(--text-3);
	}

	.sm-await-text {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs, 12px);
		color: var(--text-2);
	}

	.sm-await-sub {
		margin: 0;
		font-size: var(--text-label);
		color: var(--text-3);
	}

	.sm-feed {
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		flex: 1 1 auto;
		min-height: 0;
	}

	.sm-feed-row {
		position: relative;
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-subtle);
	}
	.sm-feed-row:last-child {
		border-bottom: none;
	}

	.sm-rail {
		position: absolute;
		inset-block: 4px;
		left: 0;
		width: 3px;
	}

	.sm-row-collapsed {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px 8px 20px;
		text-align: left;
	}

	.sm-row-toggle {
		position: absolute;
		inset: 0;
		z-index: 0;
		margin: 0;
		padding: 0;
		background: transparent;
		border: none;
		cursor: pointer;
		width: 100%;
	}

	.sm-row-toggle:hover {
		background: var(--interactive-hover-surface, var(--surface-2));
	}

	.sm-row-toggle:focus-visible {
		outline: 1px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.sm-row-chrome {
		position: relative;
		z-index: 1;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		min-width: 0;
		pointer-events: none;
	}

	.sm-chip-backing {
		display: inline-flex;
		background: var(--surface-bg);
		flex-shrink: 0;
	}

	.sm-chip {
		--aug-tl: 5px;
		--aug-tr: 0;
		--aug-br: 5px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		padding: 1px 7px;
		color: var(--acc-fg);
		background: linear-gradient(0deg, var(--acc-bg), var(--acc-bg)), var(--surface-bg);
	}

	.sm-steerer-label {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		font-weight: 500;
		flex-shrink: 0;
	}

	.sm-session-link {
		position: relative;
		z-index: 2;
		pointer-events: auto;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.sm-session-link:hover {
		color: var(--text-strong);
	}

	.sm-row-time {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--text-3);
		margin-left: auto;
		flex-shrink: 0;
	}

	.sm-expand-cue {
		display: inline-flex;
		color: var(--text-faint);
		flex-shrink: 0;
	}

	.sm-topic {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: var(--text-2);
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
		text-align: left;
	}

	.sm-row-body {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 0 12px 10px 20px;
		border-top: 1px solid var(--border-subtle);
	}

	.sm-body-label {
		margin: 0;
		padding-top: 8px;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.sm-body-pre {
		margin: 0;
		max-height: 240px;
		overflow-y: auto;
		padding: 10px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-2);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.sm-full-record-link {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-label);
		color: var(--status-info-fg);
		text-decoration: underline;
		text-decoration-color: var(--status-info-border);
		text-underline-offset: 2px;
		align-self: flex-start;
	}

	.sm-full-record-link:hover {
		color: var(--text-strong);
	}
</style>
