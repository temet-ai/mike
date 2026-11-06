<script lang="ts">
	import { setContext } from 'svelte';
	import { SESSION_DETAIL_STORE_KEY } from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import RequestDrawer from '../RequestDrawer.svelte';

	interface Props {
		detailStore: SessionDetailStore;
	}

	let { detailStore }: Props = $props();

	// svelte-ignore state_referenced_locally
	setContext(SESSION_DETAIL_STORE_KEY, detailStore);
</script>

<RequestDrawer />
