<script lang="ts">
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import CompressionReviewQueue from '$lib/components/compression/CompressionReviewQueue.svelte';
</script>

<PageIntro
	title="Compression review"
	description="Investigate captured content and follow-up work. Flags nominate evidence for evaluation; they do not enable compression."
/>
<CompressionReviewQueue />
