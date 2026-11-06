<script lang="ts">
	import { Search } from '@lucide/svelte';
	import { isRecord, strField } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	const query = $derived(strField(input, 'query'));

	function stringList(key: string): string[] {
		if (!isRecord(input)) return [];
		const v = input[key];
		return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
	}
	const allowed = $derived(stringList('allowed_domains'));
	const blocked = $derived(stringList('blocked_domains'));
</script>

<div class="websearch-call">
	{#if query !== null}
		<div class="websearch-call__query-row">
			<Search size={13} aria-hidden="true" class="websearch-call__icon" />
			<span class="websearch-call__query">{query}</span>
		</div>
	{:else}
		<span class="websearch-call__empty">No query in input.</span>
	{/if}

	{#if allowed.length > 0 || blocked.length > 0}
		<div class="websearch-call__domains">
			{#each allowed as d (d)}
				<span class="websearch-call__chip websearch-call__chip--allow">+ {d}</span>
			{/each}
			{#each blocked as d (d)}
				<span class="websearch-call__chip websearch-call__chip--block">− {d}</span>
			{/each}
		</div>
	{/if}
</div>

<style>
	.websearch-call {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.websearch-call__query-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		min-width: 0;
	}
	.websearch-call :global(.websearch-call__icon) {
		color: var(--status-info-fg);
		flex-shrink: 0;
		margin-top: 2px;
	}
	.websearch-call__query {
		font-size: 13px;
		color: var(--text-strong);
		word-break: break-word;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.websearch-call__domains {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.websearch-call__chip {
		padding: 1px 7px;
		border-radius: 2px;
		border: 1px solid var(--border-subtle);
		font-family: var(--font-mono);
		font-size: var(--text-label);
		word-break: break-all;
		overflow-wrap: anywhere;
	}
	.websearch-call__chip--allow {
		background: var(--status-success-bg);
		border-color: var(--status-success-border);
		color: var(--status-success-fg);
	}
	.websearch-call__chip--block {
		background: var(--status-danger-bg);
		border-color: var(--status-danger-border);
		color: var(--status-danger-fg);
	}
	.websearch-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
