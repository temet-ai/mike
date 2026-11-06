<script lang="ts">
	import { useSvelteFlow } from '@xyflow/svelte';

	interface Props {
		onReady: (fitView: () => void) => void;
	}

	let { onReady }: Props = $props();

	const { fitView } = useSvelteFlow();

	$effect(() => {
		onReady(() => {
			void fitView({ duration: 200, padding: 0.15, maxZoom: 1 });
		});
	});
</script>
