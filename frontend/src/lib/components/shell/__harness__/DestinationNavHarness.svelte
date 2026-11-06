<script lang="ts">
	import { shellState } from '$lib/stores/shellState.svelte';
	import DestinationNav from '../DestinationNav.svelte';

	shellState.sessionsCollapsed = false;
</script>

<div class="flex h-[38rem] w-60 bg-[var(--surface-sidebar)]">
	<DestinationNav />
</div>
