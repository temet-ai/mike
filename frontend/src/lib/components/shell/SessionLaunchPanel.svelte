<script lang="ts">
	import { getContext, onDestroy, untrack } from 'svelte';
	import { SquareTerminal } from '@lucide/svelte';
	import { getLaunch, killLaunch, LaunchApiError } from '$lib/api/launches';
	import type { LaunchRow } from '$lib/api/launches';
	import { describeResult, pipelineFacts, placementOf } from '$lib/launches/status';
	import { formatRelative } from '$lib/time';
	import { proxyEvents, MAX_EVENTS } from '$lib/ws.svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		sessionId: string;
		now: number;
		grouped?: boolean;
	}

	const { sessionId, now, grouped = false }: Props = $props();

	const THROTTLE_MS = 500;

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const sectionKey = $derived(`session:${sessionId}:launch-open`);
	const detailKey = $derived(`session:${sessionId}:launch-detail-open`);

	let launch = $state<LaunchRow | null>(null);
	let loading = $state(false);
	let loadedForSession = $state<string | null>(null);

	async function load(id: string): Promise<void> {
		loading = true;
		try {
			launch = await getLaunch(id);
		} catch {
			launch = null;
		} finally {
			loading = false;
			loadedForSession = id;
		}
	}

	$effect(() => {
		const sid = sessionId;
		untrack(() => {
			launch = null;
			loadedForSession = null;
			if (!sid) return;
			void load(sid);
		});
	});

	let detailExpandedLocal = $state(false);
	const detailExpanded = $derived(
		collapseStore ? collapseStore.isExpanded(detailKey) : detailExpandedLocal
	);
	function toggleDetail(): void {
		if (collapseStore) collapseStore.toggle(detailKey);
		else detailExpandedLocal = !detailExpandedLocal;
	}

	let confirmKill = $state(false);
	let killing = $state(false);
	let killError = $state<string | null>(null);

	function beginKillConfirm(): void {
		confirmKill = true;
		killError = null;
	}
	function cancelKillConfirm(): void {
		confirmKill = false;
	}
	async function performKill(): Promise<void> {
		killing = true;
		try {
			await killLaunch(sessionId);
			confirmKill = false;
			await load(sessionId);
		} catch (err) {
			killError = err instanceof LaunchApiError ? err.message : String(err);
		} finally {
			killing = false;
		}
	}

	let processedEvents = $state(0);
	let refetchTimer: ReturnType<typeof setTimeout> | null = null;
	function scheduleRefetch(): void {
		if (refetchTimer !== null) clearTimeout(refetchTimer);
		refetchTimer = setTimeout(() => {
			refetchTimer = null;
			if (loadedForSession) void load(loadedForSession);
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
			if (
				ev.type === 'RowChanged' &&
				ev.table === 'launch_launches' &&
				ev.session_id === sessionId
			) {
				scheduleRefetch();
			}
		}
		processedEvents = total;
	});
	onDestroy(() => {
		if (refetchTimer !== null) clearTimeout(refetchTimer);
	});

	function absoluteTitle(iso: string | null): string | undefined {
		return iso ?? undefined;
	}

	const hasFullDetail = $derived(
		launch !== null &&
			(launch.claude_args.length > 0 || launch.prompt_present || launch.stderr_text !== null)
	);
</script>

{#if launch !== null}
	{@const row = launch}
	{@const result = describeResult(row.result)}
	{@const canKill = row.closed_at === null}
	<SectionCard
		title="Launch"
		titleIcon={SquareTerminal}
		ariaLabel="How this session was launched"
		collapsible
		bordered={!grouped}
		{sectionKey}
		defaultCollapsed
		titleTooltip="How mike's own launcher started this session — backend, placement, and the channel-check / opened / delivered / confirmed / result pipeline."
	>
		{#snippet headerActions()}
			<span class="slp-fact slp-fact--{result.tone}" title={result.title}>
				<result.icon size={13} aria-hidden="true" />
				<span>{result.label}</span>
			</span>
		{/snippet}
		{#snippet children()}
			<div class="slp-body">
				<div class="slp-summary">
					<span class="slp-backend">{row.backend}</span>
					<span class="slp-meta">placement {placementOf(row)}</span>
					<span class="slp-meta" title={absoluteTitle(row.created_at)}
						>launched {formatRelative(row.created_at, now)}</span
					>
					{#if row.closed_at}
						<span class="slp-meta" title={absoluteTitle(row.closed_at)}
							>closed {formatRelative(row.closed_at, now)}</span
						>
					{/if}
				</div>

				<div class="slp-facts" role="list" aria-label="Launch pipeline status">
					{#each pipelineFacts(row) as d (d.label)}
						<span class="slp-fact slp-fact--{d.tone}" role="listitem" title={d.title}>
							<d.icon size={13} aria-hidden="true" />
							<span>{d.label}</span>
						</span>
					{/each}
				</div>

				{#if row.reason}
					<p class="slp-reason">{row.reason}</p>
				{/if}

				<div class="slp-actions">
					{#if canKill}
						{#if confirmKill}
							<span class="slp-confirm" role="group" aria-label="Confirm kill">
								<span class="slp-confirm__text">Close this surface?</span>
								<Button
									variant="destructive"
									size="sm"
									disabled={killing}
									onclick={() => void performKill()}
								>
									{killing ? 'Killing...' : 'Confirm'}
								</Button>
								<Button variant="outline" size="sm" onclick={cancelKillConfirm} disabled={killing}
									>Cancel</Button
								>
							</span>
						{:else}
							<Button variant="outline" size="sm" onclick={beginKillConfirm}>Kill</Button>
						{/if}
					{/if}
				</div>
				{#if killError}
					<p class="slp-error" role="alert">{killError}</p>
				{/if}

				{#if hasFullDetail}
					<button
						type="button"
						class="slp-detail-toggle"
						aria-expanded={detailExpanded}
						aria-controls="slp-detail-body"
						onclick={toggleDetail}
					>
						{detailExpanded
							? 'hide full launch detail'
							: `click to expand full launch detail (brief, argv${row.stderr_text ? ', stderr' : ''})`}
					</button>
					{#if detailExpanded}
						<div class="slp-detail-body" id="slp-detail-body">
							{#if row.claude_args.length > 0}
								<div class="slp-block">
									<span class="slp-block__label">claude argv (full)</span>
									<!-- svelte-ignore a11y_no_noninteractive_tabindex (axe scrollable-region-focusable requires the overflow container be keyboard-focusable) -->
									<pre
										class="slp-pre"
										tabindex="0"
										role="region"
										aria-label="Complete claude arguments (scrollable)">{row.claude_args.join(
											'\n'
										)}</pre>
								</div>
							{/if}
							{#if row.prompt_present}
								<div class="slp-block">
									<span class="slp-block__label">
										Brief {row.brief_mode
											? `(${row.brief_mode}${row.brief_path ? `: ${row.brief_path}` : ''})`
											: ''}
									</span>
									{#if row.brief_text}
										<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
										<pre
											class="slp-pre"
											tabindex="0"
											role="region"
											aria-label="Full brief text (scrollable)">{row.brief_text}</pre>
									{:else}
										<p class="slp-note">A brief was sent, but its full text is not on this row.</p>
									{/if}
								</div>
							{/if}
							{#if row.stderr_text}
								<div class="slp-block">
									<span class="slp-block__label">
										Backend stderr{row.stderr_truncated ? ' (capped at 64 KiB — more was cut)' : ''}
									</span>
									<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
									<pre
										class="slp-pre"
										tabindex="0"
										role="region"
										aria-label="Backend stderr (scrollable)">{row.stderr_text}</pre>
								</div>
							{/if}
						</div>
					{/if}
				{/if}
			</div>
		{/snippet}
	</SectionCard>
{/if}

<style>
	.slp-body {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.slp-summary {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
	}
	.slp-backend {
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
	.slp-meta {
		font-size: var(--text-caption, 11px);
		color: var(--text-3);
		white-space: nowrap;
	}

	.slp-facts {
		display: flex;
		flex-wrap: wrap;
		gap: 14px;
	}
	.slp-fact {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: var(--text-caption, 11px);
		white-space: nowrap;
	}
	.slp-fact--success {
		color: var(--status-success-fg);
	}
	.slp-fact--warning {
		color: var(--status-warning-fg);
	}
	.slp-fact--danger {
		color: var(--status-danger-fg);
	}
	.slp-fact--info {
		color: var(--status-info-fg);
	}
	.slp-fact--muted {
		color: var(--text-3);
	}

	.slp-reason {
		margin: 0;
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
		white-space: pre-wrap;
	}

	.slp-actions {
		display: flex;
		align-items: center;
	}
	.slp-confirm {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	.slp-confirm__text {
		font-size: var(--text-caption, 11px);
		color: var(--status-danger-fg);
	}
	.slp-error {
		margin: 0;
		font-size: 11px;
		color: var(--status-danger-fg);
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.slp-detail-toggle {
		display: block;
		width: 100%;
		background: none;
		border: none;
		border-top: 1px solid var(--border-subtle);
		border-radius: 0;
		padding: 8px 0 0;
		margin: 0;
		text-align: left;
		cursor: pointer;
		color: inherit;
		font: inherit;
	}
	.slp-detail-toggle:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.slp-hint {
		margin: 0;
		font-size: 11px;
		font-style: italic;
		color: var(--text-3);
	}

	.slp-detail-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.slp-block {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.slp-block__label {
		font-size: var(--text-label, 10px);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.slp-note {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}

	.slp-pre {
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
