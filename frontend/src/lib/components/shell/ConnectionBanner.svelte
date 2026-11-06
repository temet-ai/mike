<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Loader2, Plug } from '@lucide/svelte';
	import { proxyEvents } from '$lib/ws.svelte';
	import { resolve } from '$app/paths';

	interface Props {
		visibilityDelayMs?: number;
	}

	const { visibilityDelayMs = 3000 }: Props = $props();

	let pendingVisible = $state(false);
	let pendingTimer: ReturnType<typeof setTimeout> | null = null;

	const wsState = $derived(proxyEvents.state);

	function clearPendingTimer(): void {
		if (pendingTimer !== null) {
			clearTimeout(pendingTimer);
			pendingTimer = null;
		}
	}

	$effect(() => {
		const state = wsState;
		if (state === 'connected') {
			clearPendingTimer();
			pendingVisible = false;
			return;
		}
		if (state === 'disconnected') {
			clearPendingTimer();
			pendingVisible = true;
			return;
		}
		if (pendingTimer === null && !pendingVisible) {
			pendingTimer = setTimeout(() => {
				pendingTimer = null;
				pendingVisible = true;
			}, visibilityDelayMs);
		}
	});

	onDestroy(() => {
		clearPendingTimer();
	});

	function retryNow(): void {
		proxyEvents.connect();
	}

	const isDisconnected = $derived(wsState === 'disconnected');
	const isReconnecting = $derived(wsState === 'connecting' || wsState === 'reconnecting');
	const showBanner = $derived(pendingVisible && wsState !== 'connected');
</script>

{#if showBanner}
	<div
		class="connection-banner"
		class:connection-banner--err={isDisconnected}
		class:connection-banner--warn={isReconnecting}
		role="status"
		aria-live="polite"
		data-slot="connection-banner"
	>
		{#if isDisconnected}
			<div class="connection-banner__icon" aria-hidden="true">
				<Plug class="size-4" />
			</div>
			<div class="connection-banner__body">
				<strong class="connection-banner__title" data-slot="connection-banner-title">
					Cannot reach this instance’s inspect API.
				</strong>
				<p class="connection-banner__hint">
					Is <code class="font-mono">mike</code> running? Open its terminal control centre to start it,
					or read the recovery guide.
				</p>
			</div>
			<div class="connection-banner__actions">
				<button
					type="button"
					class="connection-banner__btn"
					onclick={retryNow}
					data-slot="connection-banner-retry"
				>
					<span>Retry now</span>
				</button>
				<a
					href={resolve('/help/docs/[slug]', { slug: 'recovery-and-maintenance' })}
					class="connection-banner__btn"
					data-slot="connection-banner-docs"
				>
					<span>Recovery guide</span>
				</a>
			</div>
		{:else}
			<div class="connection-banner__icon" aria-hidden="true">
				<Loader2 class="size-4 animate-spin" />
			</div>
			<div class="connection-banner__body">
				<strong class="connection-banner__title" data-slot="connection-banner-title">
					Reconnecting to inspect API…
				</strong>
			</div>
		{/if}
	</div>
{/if}

<style>
	.connection-banner {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px;
		border-bottom: 1px solid var(--border-default);
		font-size: 0.8125rem;
	}
	.connection-banner--err {
		background: var(--status-danger-bg);
		border-bottom-color: var(--status-danger-border);
		color: var(--status-danger-fg);
	}
	.connection-banner--warn {
		background: var(--surface-2);
		border-bottom-color: var(--border-default);
		color: var(--text-2);
	}
	.connection-banner__icon {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.connection-banner__body {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.connection-banner__title {
		font-weight: 600;
		color: inherit;
	}
	.connection-banner__hint {
		font-size: 0.75rem;
		color: var(--text-2);
		margin: 0;
	}
	.connection-banner--err .connection-banner__hint {
		color: var(--status-danger-fg);
		opacity: 0.85;
	}
	.connection-banner__actions {
		display: flex;
		flex-shrink: 0;
		gap: 8px;
	}
	.connection-banner__btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 10px;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		border: 1px solid var(--status-danger-border);
		border-radius: 4px;
		background: transparent;
		color: var(--status-danger-fg);
		text-decoration: none;
		cursor: pointer;
		transition:
			background 0.14s,
			border-color 0.14s;
	}
	.connection-banner__btn:hover {
		border-color: var(--status-danger-fg);
		background: var(--status-danger-bg);
	}
	.connection-banner__btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--ring);
	}
</style>
