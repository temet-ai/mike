<script lang="ts">
	import { setContext } from 'svelte';
	import {
		SESSION_DETAIL_STORE_KEY,
		SESSION_FLOW_STORE_KEY,
		SESSION_LIST_STORE_KEY
	} from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import type { SessionFlowStore } from '$lib/session-flow/sessionFlowStore.svelte';
	import SessionDetailPage from '../+page.svelte';

	interface Props {
		detailStore: SessionDetailStore;
		sessionStore: SessionListStore;
		flowStore?: SessionFlowStore;
	}

	let { detailStore, sessionStore, flowStore }: Props = $props();

	// svelte-ignore state_referenced_locally
	setContext(SESSION_DETAIL_STORE_KEY, detailStore);
	// svelte-ignore state_referenced_locally
	setContext(SESSION_LIST_STORE_KEY, sessionStore);
	// svelte-ignore state_referenced_locally
	if (flowStore) {
		setContext(SESSION_FLOW_STORE_KEY, flowStore);
	}
</script>

<SessionDetailPage />
