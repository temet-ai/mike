<script lang="ts">
	import { Search } from '@lucide/svelte';
	import { strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const pattern = $derived(strField(input, 'pattern'));
	const path = $derived(strField(input, 'path'));
	const glob = $derived(strField(input, 'glob'));
	const fileType = $derived(strField(input, 'type'));
	const outputMode = $derived(strField(input, 'output_mode'));
</script>

<div class="search-call">
	{#if pattern !== null}
		<div class="search-call__pattern-row">
			<Search size={13} aria-hidden="true" class="search-call__icon" />
			<code class="search-call__pattern">{pattern}</code>
		</div>
	{:else}
		<span class="search-call__empty">No pattern in input.</span>
	{/if}

	{#if path !== null || glob !== null || fileType !== null || outputMode !== null}
		<div class="search-call__scope">
			{#if path !== null}
				<span class="search-call__chip"><span class="search-call__chip-k">in</span>{path}</span>
			{/if}
			{#if glob !== null}
				<span class="search-call__chip"><span class="search-call__chip-k">glob</span>{glob}</span>
			{/if}
			{#if fileType !== null}
				<span class="search-call__chip"
					><span class="search-call__chip-k">type</span>{fileType}</span
				>
			{/if}
			{#if outputMode !== null}
				<span class="search-call__chip"
					><span class="search-call__chip-k">mode</span>{outputMode}</span
				>
			{/if}
		</div>
	{/if}
</div>

<style>
	.search-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.search-call__pattern-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		min-width: 0;
	}
	.search-call :global(.search-call__icon) {
		color: var(--agent-3-fg);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.search-call__pattern {
		padding: 2px 8px;
		background: var(--agent-3-bg);
		border: 1px solid var(--agent-3-border);
		border-radius: 2px;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-strong);
		word-break: break-all;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.search-call__scope {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.search-call__chip {
		display: inline-flex;
		align-items: baseline;
		gap: 5px;
		padding: 1px 7px;
		border: 1px solid var(--border-subtle);
		border-radius: 2px;
		background: var(--surface-2);
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-2);
		word-break: break-all;
		overflow-wrap: anywhere;
	}
	.search-call__chip-k {
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.search-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
