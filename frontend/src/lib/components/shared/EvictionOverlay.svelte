<script lang="ts">
	import { onMount } from 'svelte';
	import { onCacheEviction } from '$lib/stores/cache/unifiedCache.svelte';
	import {
		onDetailEviction,
		onSessionPageEvict
	} from '$lib/stores/cache/sessionDetailStore.svelte';
	import { onRequestDetailEvict } from '$lib/stores/cache/requestDetailStore.svelte';
	import { onFindingEvict } from '$lib/stores/findings.svelte';

	interface EvictionRecord {
		reason: string;
		at: number;
	}

	let records = $state<EvictionRecord[]>([]);

	const RECORDS_CAP = 500;

	const WINDOW_MS = 60_000;

	function push(reason: string): void {
		records.push({ reason, at: Date.now() });
		if (records.length > RECORDS_CAP) {
			records.splice(0, records.length - RECORDS_CAP);
		}
	}

	let now = $state(Date.now());
	let tickTimer: ReturnType<typeof setInterval> | null = null;

	onMount(() => {
		const unsubs: Array<() => void> = [
			onCacheEviction((e) => push(`${e.source}:${e.reason}`)),
			onSessionPageEvict((e) => push(`session-pages:${e.reason}`)),
			onDetailEviction((e) => push(`session-details:${e.reason}`)),
			onRequestDetailEvict((e) => push(`request-detail:${e.reason}`)),
			onFindingEvict((e) => push(`findings:${e.reason}`))
		];
		tickTimer = setInterval(() => {
			now = Date.now();
		}, 1_000);
		return () => {
			for (const u of unsubs) u();
			if (tickTimer !== null) clearInterval(tickTimer);
		};
	});

	const counts = $derived.by((): Map<string, number> => {
		const cutoff = now - WINDOW_MS;
		const out = new Map<string, number>();
		for (const rec of records) {
			if (rec.at < cutoff) continue;
			out.set(rec.reason, (out.get(rec.reason) ?? 0) + 1);
		}
		return out;
	});

	const rows = $derived.by(() => {
		const arr = Array.from(counts.entries());
		arr.sort((a, b) => {
			if (b[1] !== a[1]) return b[1] - a[1];
			return a[0] < b[0] ? -1 : 1;
		});
		return arr;
	});
</script>

<div
	class="eviction-overlay"
	role="status"
	aria-live="polite"
	aria-label="Eviction pressure (last 60 seconds)"
>
	<header class="eviction-overlay__head">Evictions · last 60s</header>
	{#if rows.length === 0}
		<p class="eviction-overlay__empty">No evictions</p>
	{:else}
		<ul class="eviction-overlay__list">
			{#each rows as [reason, count] (reason)}
				<li class="eviction-overlay__row">
					<span class="eviction-overlay__reason">{reason}</span>
					<span class="eviction-overlay__count">{count}</span>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.eviction-overlay {
		position: fixed;
		bottom: 12px;
		right: 12px;
		min-width: 240px;
		max-width: 320px;
		padding: 10px 12px;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 11px;
		color: var(--text-2);
		background: oklch(from var(--surface-bg) l c h / 0.92);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		box-shadow: var(--shadow-lg);
		z-index: 9999;
		backdrop-filter: blur(8px);
		pointer-events: none;
	}

	.eviction-overlay__head {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin-bottom: 6px;
	}

	.eviction-overlay__empty {
		margin: 0;
		font-style: italic;
		color: var(--text-3);
	}

	.eviction-overlay__list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.eviction-overlay__row {
		display: flex;
		justify-content: space-between;
		gap: 12px;
	}

	.eviction-overlay__reason {
		color: var(--text-strong);
	}

	.eviction-overlay__count {
		color: var(--status-warning-fg);
		font-variant-numeric: tabular-nums;
	}
</style>
