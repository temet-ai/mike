<script lang="ts">
	import { setContext } from 'svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import SessionTimeline from '../SessionTimeline.svelte';

	interface Props {
		detailStore: SessionDetailStore;
		now: number;
		scrollContainer: HTMLElement | undefined;
	}

	let { detailStore, now, scrollContainer }: Props = $props();

	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<SessionTimeline {detailStore} {now} {scrollContainer} />
