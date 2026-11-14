<script lang="ts">
	interface Props {
		/** A JSON object whose fields are shown whole: scalars as text, nested values as JSON. */
		facts: unknown;
		label: string;
	}

	const { facts, label }: Props = $props();

	const entries = $derived(
		facts !== null && typeof facts === 'object' && !Array.isArray(facts)
			? Object.entries(facts as Record<string, unknown>)
			: []
	);

	function scalar(value: unknown): string | null {
		if (typeof value === 'string') return value;
		if (typeof value === 'number' || typeof value === 'boolean') return String(value);
		if (value === null) return '—';
		return null;
	}
</script>

{#if entries.length > 0}
	<dl class="facts" aria-label={label} data-slot="wright-facts">
		{#each entries as [key, value] (key)}
			{@const text = scalar(value)}
			<dt>{key}</dt>
			<dd>
				{#if text !== null}
					{text}
				{:else}
					<pre>{JSON.stringify(value, null, 2)}</pre>
				{/if}
			</dd>
		{/each}
	</dl>
{:else if facts !== null && facts !== undefined && typeof facts !== 'object'}
	<p class="facts__plain" aria-label={label}>{String(facts)}</p>
{/if}

<style>
	.facts {
		display: grid;
		grid-template-columns: max-content minmax(0, 1fr);
		gap: var(--space-1) var(--space-3);
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
	}

	.facts dt {
		color: var(--text-3);
	}

	.facts dd {
		margin: 0;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.facts pre {
		margin: 0;
		overflow-x: auto;
		font: inherit;
	}

	.facts__plain {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
