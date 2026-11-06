<script lang="ts">
	import type { ComponentProps } from 'svelte';
	import StatusIndicators from '../StatusIndicators.svelte';

	let props: ComponentProps<typeof StatusIndicators> = $props();
</script>

<StatusIndicators {...props} />
