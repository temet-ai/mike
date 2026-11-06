<script lang="ts">
	import { shellState } from '$lib/stores/shellState.svelte';
	import CommandPalette from '../shell/CommandPalette.svelte';

	shellState.commandPaletteOpen = true;
</script>

<CommandPalette />
