<script lang="ts">
	import { shellState } from '$lib/stores/shellState.svelte';
	import HelpDrawer from '../shared/HelpDrawer.svelte';
	import type { StatsResponse } from '$lib/types';

	interface Props {
		stats: StatsResponse | null;
	}

	let { stats }: Props = $props();

	shellState.helpDrawerOpen = true;
</script>

<HelpDrawer {stats} />
