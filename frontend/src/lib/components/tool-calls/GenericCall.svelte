<script lang="ts">
	import { isRecord } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	function fieldValue(v: unknown): string {
		if (typeof v === 'string') return v;
		try {
			return JSON.stringify(v);
		} catch {
			return String(v);
		}
	}

	const entries = $derived(isRecord(input) ? Object.entries(input) : []);
</script>

{#if entries.length > 0}
	<dl class="generic-fields">
		{#each entries as [key, value] (key)}
			<div class="generic-fields__row">
				<dt class="generic-fields__key">{key}</dt>
				<dd class="generic-fields__val">{fieldValue(value)}</dd>
			</div>
		{/each}
	</dl>
{:else}
	<span class="generic-fields__empty">No structured input fields.</span>
{/if}

<style>
	.generic-fields {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0;
	}
	.generic-fields__row {
		display: grid;
		grid-template-columns: minmax(80px, max-content) 1fr;
		gap: 10px;
		align-items: baseline;
	}
	.generic-fields__key {
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.03em;
		white-space: nowrap;
	}
	.generic-fields__val {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-1);
		word-break: break-word;
		overflow-wrap: anywhere;
		white-space: pre-wrap;
	}
	.generic-fields__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
