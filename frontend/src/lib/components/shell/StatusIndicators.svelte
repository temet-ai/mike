<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Activity, Database, Plug, Search } from '@lucide/svelte';
	import * as Popover from '$lib/components/ui/popover';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { proxyEvents } from '$lib/ws.svelte';
	import { toggleCommandPalette } from '$lib/stores/shellState.svelte';
	import { formatCount } from '$lib/time';
	import type { StatsResponse } from '$lib/types';

	interface Props {
		stats: StatsResponse | null;
		showCommandShortcut?: boolean;
	}

	const { stats, showCommandShortcut = true }: Props = $props();

	const wsState = $derived(proxyEvents.state);
	const connectionTone = $derived.by(() => {
		if (wsState === 'connected') return 'ok' as const;
		if (wsState === 'disconnected') return 'err' as const;
		return 'warn' as const;
	});
	const connectionLabel = $derived.by(() => {
		if (wsState === 'connected') return 'Connected';
		if (wsState === 'disconnected') return 'Disconnected';
		return 'Reconnecting…';
	});
	const connectionTooltip = $derived.by(() => {
		if (wsState === 'connected') return 'Inspect API live at :9989.';
		if (wsState === 'disconnected') return 'Inspect API not reachable at :9989. Click to retry.';
		return 'Reconnecting to inspect API at :9989…';
	});
	const isDisconnected = $derived(wsState === 'disconnected');

	function retryConnection(): void {
		proxyEvents.connect();
	}

	let lastCount = $state(-1);
	let pulseUntil = $state(0);
	let now = $state(Date.now());
	let nowTimer: ReturnType<typeof setInterval> | null = null;

	const captureCount = $derived(stats?.request_count ?? 0);
	const captureLabel = $derived(formatCount(captureCount));
	const isPulsing = $derived(now < pulseUntil);

	$effect(() => {
		const next = captureCount;
		if (lastCount === -1) {
			lastCount = next;
			return;
		}
		if (next > lastCount) {
			pulseUntil = Date.now() + 600;
		}
		lastCount = next;
	});

	function gotoFeed(): void {
		void goto(resolve('/'));
	}

	const dbDisabled = $derived(stats?.db_enabled === false);

	let isApple = $state(false);
	onMount(() => {
		try {
			const platform =
				(navigator as unknown as { userAgentData?: { platform?: string } }).userAgentData
					?.platform ??
				navigator.platform ??
				'';
			isApple = /Mac|iPhone|iPad|iPod/i.test(platform);
		} catch {
			isApple = false;
		}

		nowTimer = setInterval(() => {
			now = Date.now();
		}, 250);

		return () => {
			if (nowTimer !== null) {
				clearInterval(nowTimer);
				nowTimer = null;
			}
		};
	});

	const paletteShortcutLabel = $derived(isApple ? '⌘K' : 'Ctrl+K');
	const paletteTooltip = $derived(`Open command palette (${isApple ? '⌘K' : 'Ctrl+K'})`);

	const compactTriggerClass = $derived.by(() => {
		const tone =
			connectionTone === 'err'
				? 'status-chip--err'
				: connectionTone === 'warn' || dbDisabled
					? 'status-chip--warn'
					: 'status-chip--ok';
		return `status-chip-trigger status-chip ${tone}`;
	});

</script>

<div class="status-cluster" data-slot="status-cluster">
	{#if isDisconnected}
		<Tooltip text={connectionTooltip} slug="inspect-api">
			{#snippet child({ props })}
				<button
					{...props}
					type="button"
					class="status-chip status-chip--err"
					onclick={retryConnection}
					aria-label={connectionTooltip}
					data-slot="connection-status"
					data-state={wsState}
				>
					<span class="status-dot" aria-hidden="true"></span>
					<span class="status-label">{connectionLabel}</span>
				</button>
			{/snippet}
		</Tooltip>
	{:else}
		<Tooltip text={connectionTooltip} slug="inspect-api">
			<span
				class="status-chip"
				class:status-chip--ok={connectionTone === 'ok'}
				class:status-chip--warn={connectionTone === 'warn'}
				data-slot="connection-status"
				data-state={wsState}
			>
				<span class="status-dot" aria-hidden="true"></span>
				<span class="status-label">{connectionLabel}</span>
			</span>
		</Tooltip>
	{/if}

	<Tooltip text={`Captured ${captureLabel} requests. Click to view the feed.`} slug="request">
		{#snippet child({ props })}
			<button
				{...props}
				type="button"
				class="status-chip status-chip--capture"
				class:is-pulsing={isPulsing}
				onclick={gotoFeed}
				aria-label={`Captured ${captureLabel} requests. Click to view the feed.`}
				data-slot="capture-status"
			>
				<Activity class="size-3 status-icon" aria-hidden="true" />
				<span class="status-label">
					Captured ·
					<span aria-live="polite" class="status-count font-mono">{captureLabel}</span>
				</span>
			</button>
		{/snippet}
	</Tooltip>

	{#if dbDisabled}
		<Popover.Root>
			<Popover.Trigger
				class="status-chip-trigger status-chip status-chip--warn"
				aria-label="Persistence is disabled. Click for details."
				data-slot="db-status-trigger"
			>
				<Database class="size-3 status-icon" aria-hidden="true" />
				<span class="status-label">Persistence off</span>
			</Popover.Trigger>
			<Popover.Content
				class="status-popover max-w-[340px]"
				align="end"
				sideOffset={6}
				data-slot="db-status-content"
			>
				<div class="flex flex-col gap-2">
					<h4 class="text-xs font-semibold uppercase tracking-kpi text-[var(--text-3)]">
						Persistence is disabled
					</h4>
					<p class="text-[0.8125rem] leading-snug text-[var(--text-1)]">
						Sessions evict on restart, search history isn't saved, prompt anomalies aren't tracked,
						and rehydration after a crash isn't possible.
					</p>
					<p class="text-[0.8125rem] leading-snug text-[var(--text-1)]">
						To enable, use <code class="font-mono">/settings</code> and restart.
					</p>
				</div>
			</Popover.Content>
		</Popover.Root>
	{/if}

	{#if showCommandShortcut}
		<Tooltip text={paletteTooltip}>
			{#snippet child({ props })}
				<button
					{...props}
					type="button"
					class="status-chip status-chip--shortcut"
					onclick={toggleCommandPalette}
					aria-label={paletteTooltip}
					data-slot="command-palette-hint"
				>
					<Search class="size-3 status-icon" aria-hidden="true" />
					<span class="status-label font-mono">{paletteShortcutLabel}</span>
				</button>
			{/snippet}
		</Tooltip>
	{/if}
</div>

<div class="status-cluster-compact" data-slot="status-cluster-compact">
	<Popover.Root>
		<Popover.Trigger
			class={compactTriggerClass}
			aria-label="Open status breakdown"
			data-slot="status-compact-trigger"
		>
			<span class="status-dot" aria-hidden="true"></span>
			<span class="status-label">Status</span>
		</Popover.Trigger>
		<Popover.Content
			class="status-popover max-w-[360px]"
			align="end"
			sideOffset={6}
			data-slot="status-compact-content"
		>
			<div class="flex flex-col gap-3">
				<div class="flex items-center justify-between gap-3">
					<span class="text-xs font-semibold uppercase tracking-kpi text-[var(--text-3)]">
						Connection
					</span>
					<span
						class="status-chip"
						class:status-chip--ok={connectionTone === 'ok'}
						class:status-chip--warn={connectionTone === 'warn'}
						class:status-chip--err={connectionTone === 'err'}
					>
						<span class="status-dot" aria-hidden="true"></span>
						<span class="status-label">{connectionLabel}</span>
					</span>
				</div>

				<div class="flex items-center justify-between gap-3">
					<span class="text-xs font-semibold uppercase tracking-kpi text-[var(--text-3)]">
						Captured
					</span>
					<span class="font-mono text-[var(--text-1)]">{captureLabel}</span>
				</div>

				{#if dbDisabled}
					<div class="flex items-center justify-between gap-3">
						<span class="text-xs font-semibold uppercase tracking-kpi text-[var(--text-3)]">
							Persistence
						</span>
						<span class="status-chip status-chip--warn">
							<Database class="size-3 status-icon" aria-hidden="true" />
							<span class="status-label">off</span>
						</span>
					</div>
				{/if}

				{#if showCommandShortcut}
					<div
						class="flex items-center justify-between gap-3 pt-1 border-t border-[var(--border-default)]"
					>
						<span class="text-xs font-semibold uppercase tracking-kpi text-[var(--text-3)]">
							Command palette
						</span>
						<button
							type="button"
							class="status-chip status-chip--shortcut"
							onclick={toggleCommandPalette}
						>
							<Search class="size-3 status-icon" aria-hidden="true" />
							<span class="status-label font-mono">{paletteShortcutLabel}</span>
						</button>
					</div>
				{/if}

				{#if isDisconnected}
					<div class="pt-1 border-t border-[var(--border-default)]">
						<Tooltip text="Retry connection now.">
							{#snippet child({ props })}
								<button
									{...props}
									type="button"
									class="status-chip status-chip--err w-full justify-center"
									onclick={retryConnection}
								>
									<Plug class="size-3 status-icon" aria-hidden="true" />
									<span class="status-label">Retry now</span>
								</button>
							{/snippet}
						</Tooltip>
					</div>
				{/if}
			</div>
		</Popover.Content>
	</Popover.Root>
</div>

<style>
	.status-cluster {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.status-cluster-compact {
		display: none;
	}

	@media (max-width: 1080px) {
		.status-cluster {
			display: none;
		}
		.status-cluster-compact {
			display: inline-flex;
			align-items: center;
		}
	}

	.status-chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 8px;
		font-size: 10px;
		line-height: 1;
		border-radius: 2px;
		background: transparent;
		color: var(--text-2);
		box-shadow: inset 0 0 0 1px var(--border-default);
		transition:
			background-color var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap),
			box-shadow var(--dur-1) var(--ease-snap);
		cursor: default;
	}
	button.status-chip,
	:global(.status-chip-trigger) {
		cursor: pointer;
	}
	button.status-chip:hover,
	:global(.status-chip-trigger:hover) {
		color: var(--text-strong);
	}
	button.status-chip:focus-visible,
	:global(.status-chip-trigger:focus-visible) {
		outline: none;
		box-shadow:
			inset 0 0 0 1px var(--border-default),
			0 0 0 2px var(--ring);
	}

	.status-dot {
		width: 6px;
		height: 6px;
		background: currentColor;
		flex-shrink: 0;
	}

	.status-label {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		letter-spacing: 0.01em;
	}
	.status-count {
		color: var(--text-1);
	}

	.status-chip--ok {
		color: var(--status-success-fg);
		background: var(--status-success-bg);
		box-shadow: inset 0 0 0 1px var(--status-success-border);
	}
	.status-chip--warn {
		color: var(--status-warning-fg);
		background: var(--status-warning-bg);
		box-shadow: inset 0 0 0 1px var(--status-warning-border);
	}
	.status-chip--err {
		color: var(--status-danger-fg);
		background: var(--status-danger-bg);
		box-shadow: inset 0 0 0 1px var(--status-danger-border);
	}
	.status-chip--muted {
		color: var(--text-3);
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--border-default);
	}
	.status-chip--shortcut {
		color: var(--text-2);
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--border-default);
	}
	.status-chip--shortcut:hover {
		color: var(--text-strong);
		box-shadow: inset 0 0 0 1px var(--text-3);
	}
	.status-chip--capture {
		color: var(--text-1);
	}

	.status-chip.is-pulsing {
		color: var(--status-success-fg);
		box-shadow: inset 0 0 0 1px var(--status-success-border);
		background: var(--status-success-bg);
		animation: capture-pulse var(--dur-3) ease-out 1;
	}

	@keyframes capture-pulse {
		0% {
			box-shadow:
				inset 0 0 0 1px var(--status-success-border),
				0 0 0 0 oklch(from var(--status-success-fg) l c h / 0.5);
		}
		60% {
			box-shadow:
				inset 0 0 0 1px var(--status-success-border),
				0 0 0 6px oklch(from var(--status-success-fg) l c h / 0);
		}
		100% {
			box-shadow:
				inset 0 0 0 1px var(--status-success-border),
				0 0 0 0 oklch(from var(--status-success-fg) l c h / 0);
		}
	}

	:global(.status-popover) {
		background: var(--surface-card);
		border: 1px solid var(--border-default);
		border-radius: 2px;
		padding: 12px 14px;
		font-size: 0.8125rem;
		color: var(--text-1);
	}
	:global(.status-popover code) {
		font-size: 0.78em;
		padding: 1px 4px;
		border-radius: 2px;
		background: var(--surface-2);
		color: var(--text-strong);
	}
</style>
