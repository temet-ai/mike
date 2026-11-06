<script lang="ts">
	import type { Snippet } from 'svelte';
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

{@render children()}
