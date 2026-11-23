<script lang="ts">
	import { resolve } from '$app/paths';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import type { Carrier } from '$lib/api/knowledgeConsolidation';

	interface Props {
		carrier: Carrier | null;
		/** Raw reference, shown when an older row carries no carrier object. */
		reference?: string | null;
	}

	const { carrier, reference = null }: Props = $props();
</script>

<span class="carrier" data-slot="carrier-ref">
	{#if carrier && carrier.kind === 'lesson' && carrier.lesson_id !== null}
		<a
			class="carrier__lesson"
			href={resolve('/sessions/improvement/lessons/[id]', { id: String(carrier.lesson_id) })}
		>
			Lesson {carrier.lesson_id}
		</a>
	{:else if carrier}
		<span class="carrier__file">{carrier.label}</span>
		{#if carrier.relative_path}
			<CopyIconButton label="Copy path" getText={() => carrier.relative_path} />
		{/if}
		{#if carrier.scope === 'machine'}<span class="carrier__scope"> · machine-wide</span>{/if}
	{:else if reference}
		<span class="carrier__raw">{reference}</span>
	{/if}
</span>

<style>
	.carrier {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0 var(--space-1);
		font-size: var(--text-xs);
		min-width: 0;
	}
	.carrier__lesson {
		color: var(--text-1);
	}
	.carrier__lesson:hover {
		color: var(--text-strong);
		text-decoration: underline;
	}
	.carrier__lesson:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
	.carrier__file {
		font-family: var(--font-mono, monospace);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
	.carrier__scope {
		color: var(--text-3);
	}
	.carrier__raw {
		font-family: var(--font-mono, monospace);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
</style>
