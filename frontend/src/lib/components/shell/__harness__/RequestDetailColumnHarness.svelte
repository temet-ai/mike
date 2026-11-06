<script lang="ts">
	import { setContext } from 'svelte';
	import {
		SESSION_DETAIL_STORE_KEY,
		SESSION_LIST_STORE_KEY,
		COLLAPSE_STORE_KEY
	} from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import RequestDetailColumn from '../RequestDetailColumn.svelte';

	interface Props {
		detailStore: SessionDetailStore;
		sessionStore: SessionListStore;
	}

	let { detailStore, sessionStore }: Props = $props();

	// svelte-ignore state_referenced_locally
	setContext(SESSION_DETAIL_STORE_KEY, detailStore);
	// svelte-ignore state_referenced_locally
	setContext(SESSION_LIST_STORE_KEY, sessionStore);
	setContext(COLLAPSE_STORE_KEY, createCollapseStore());
</script>

<RequestDetailColumn />
