<script lang="ts">
	import { page } from '$app/state';
	import FilterWorkbench from '$lib/components/filter/FilterWorkbench.svelte';
	import { parseFromParam } from '$lib/components/filter/workbench';

	const requestId = $derived(parseFromParam(page.url.searchParams.get('from')));

	const blockParam = $derived.by(() => {
		const raw = page.url.searchParams.get('block');
		if (raw === null) return null;
		const idx = Number.parseInt(raw, 10);
		return Number.isFinite(idx) && idx >= 0 ? idx : null;
	});

	const prefillParam = $derived.by(() => {
		const raw = page.url.searchParams.get('prefill');
		if (raw === null || raw.length === 0 || raw.length > 300) return null;
		return raw;
	});

	const ruleParam = $derived.by(() => {
		const raw = page.url.searchParams.get('rule');
		if (raw === null || raw.length === 0 || raw.length > 300) return null;
		return raw;
	});
</script>

<svelte:head>
	<title>Filter workbench — mike</title>
</svelte:head>

<div class="wb-page-shell flex min-h-0 flex-col overflow-hidden" data-slot="filter-workbench-page">
	<div class="min-h-0 flex-1">
		<FilterWorkbench {requestId} {blockParam} {prefillParam} {ruleParam} />
	</div>
</div>

<style>
	.wb-page-shell {
		height: calc(100vh - 44px);
	}
</style>
