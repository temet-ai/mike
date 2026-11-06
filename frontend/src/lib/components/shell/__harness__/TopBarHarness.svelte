<script lang="ts">
	import type { ComponentProps } from 'svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import TopBar from '../TopBar.svelte';

	let props: ComponentProps<typeof TopBar> = $props();
	shellState.paused = false;
	shellState.helpDrawerOpen = false;
	shellState.commandPaletteOpen = false;
</script>

<TopBar {...props} />
