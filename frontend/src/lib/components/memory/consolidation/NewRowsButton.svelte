<script lang="ts">
	import { Button } from '$lib/components/ui/button';

	interface Props {
		count: number;
		onShow: () => void;
	}

	const { count, onShow }: Props = $props();
</script>

<span class="new-rows" class:new-rows--hidden={count === 0} data-slot="new-rows-button">
	<Button variant="ghost" size="sm" tabindex={count === 0 ? -1 : 0} onclick={onShow}>
		Show {count} new
	</Button>
</span>

<style>
	.new-rows {
		display: inline-flex;
		justify-content: flex-end;
		min-width: 7rem;
	}
	.new-rows--hidden {
		visibility: hidden;
	}
</style>
