<script lang="ts">
	import { KNOWN_KINDS } from './promptKind';

	let { counts }: { counts: Record<string, number> } = $props();

	const segments = $derived(
		KNOWN_KINDS.map((kind) => ({ kind, count: counts[kind] ?? 0 })).filter((s) => s.count > 0)
	);
</script>

{#if segments.length > 0}
	<span class="pkc" data-testid="prompt-kind-counts">
		{#each segments as s, i (s.kind)}
			{#if i > 0}<span class="pkc__sep" aria-hidden="true">·</span>{/if}
			<span class="pkc__seg" class:pkc__seg--operator={s.kind === 'operator'}>
				{s.count}
				{s.kind}
			</span>
		{/each}
	</span>
{/if}

<style>
	.pkc {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
	}

	.pkc__sep {
		color: var(--text-3);
	}

	.pkc__seg {
		color: var(--text-3);
		white-space: nowrap;
	}

	.pkc__seg--operator {
		color: var(--text-2);
	}
</style>
