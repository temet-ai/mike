<script lang="ts">
	import { onMount } from 'svelte';
	import { listCompressionReviews, type CompressionReview } from '$lib/api/compressionReview';
	import { Button } from '$lib/components/ui/button';
	import { Select } from '$lib/components/ui/select';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import CompressionReviewItem from './CompressionReviewItem.svelte';
	let items = $state<CompressionReview[]>([]);
	let status = $state('');
	let loading = $state(true);
	let error = $state('');
	const options = [
		{ value: '', label: 'All statuses' },
		...['open', 'investigating', 'evaluated', 'dismissed'].map((value) => ({ value, label: value }))
	];
	const visible = $derived(items.filter((item) => !status || item.status === status));
	async function load() {
		loading = true;
		error = '';
		try {
			items = await listCompressionReviews();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Could not load flags.';
		} finally {
			loading = false;
		}
	}
	onMount(() => {
		void load();
	});
</script>

<div class="flex flex-col gap-4">
	<div class="flex flex-wrap items-end gap-3">
		<Select label="Filter status" {options} bind:value={status} /><Button
			variant="outline"
			onclick={load}
			disabled={loading}>Refresh</Button
		>
	</div>
	{#if error}<LoadErrorBanner title="Review queue unavailable" detail={error} onRetry={load} />
	{:else if loading}<p role="status">Loading compression flags</p>
	{:else if visible.length === 0}<p role="status">
			No flags match this status. Flag a request, response or tool result to start an investigation.
		</p>
	{:else}
		<p role="status">{visible.length} {visible.length === 1 ? 'flag' : 'flags'}</p>
		{#each visible as item (item.id)}<CompressionReviewItem {item} onSaved={load} />{/each}
	{/if}
</div>
