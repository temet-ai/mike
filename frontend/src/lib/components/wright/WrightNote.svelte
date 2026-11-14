<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		role?: 'status' | 'alert';
		children: Snippet;
	}

	const { role, children }: Props = $props();
</script>

<p class="note" {role} data-slot="wright-note">{@render children()}</p>

<style>
	.note {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}
</style>
