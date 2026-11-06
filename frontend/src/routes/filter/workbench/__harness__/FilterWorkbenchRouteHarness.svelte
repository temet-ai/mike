<script lang="ts">
	import { setContext } from 'svelte';
	import { FILTER_RULES_STORE_KEY } from '$lib/context-keys';
	import type { FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import FilterWorkbenchPage from '../+page.svelte';

	interface Props {
		rulesStore: FilterRulesStore;
	}

	let { rulesStore }: Props = $props();

	// svelte-ignore state_referenced_locally -- captures the init-time store prop once
	setContext(FILTER_RULES_STORE_KEY, rulesStore);
</script>

<FilterWorkbenchPage />
