<script lang="ts">
	import { setContext } from 'svelte';
	import { SESSION_LIST_STORE_KEY, COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import type { SessionListStore } from '$lib/stores/sessions.svelte';
	import SessionsColumn from '../SessionsColumn.svelte';

	interface Props {
		sessionStore: SessionListStore;
		now: number;
		collapsedProject?: string;
	}

	let { sessionStore, now, collapsedProject }: Props = $props();

	const collapseStore = createCollapseStore();
	// svelte-ignore state_referenced_locally
	if (collapsedProject !== undefined) {
		// svelte-ignore state_referenced_locally
		collapseStore.toggle(`sidebar:project:${collapsedProject}`);
	}
	// svelte-ignore state_referenced_locally
	setContext(SESSION_LIST_STORE_KEY, sessionStore);
	setContext(COLLAPSE_STORE_KEY, collapseStore);
</script>

<SessionsColumn {now} />
