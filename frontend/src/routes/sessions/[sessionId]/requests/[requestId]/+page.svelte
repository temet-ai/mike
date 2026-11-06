<script lang="ts">
	import { page } from '$app/state';
	import RequestDetailColumn from '$lib/components/shell/RequestDetailColumn.svelte';
</script>

<svelte:head>
	<title>Request #{page.params.requestId ?? ''} — mike</title>
</svelte:head>

<RequestDetailColumn />
