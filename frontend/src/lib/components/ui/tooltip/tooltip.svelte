<script lang="ts">
	import { Tooltip as TooltipPrimitive } from 'bits-ui';

	let {
		delayDuration = 120,
		disableHoverableContent = true,
		...restProps
	}: TooltipPrimitive.RootProps = $props();
</script>

<TooltipPrimitive.Provider {delayDuration} {disableHoverableContent}>
	<TooltipPrimitive.Root {delayDuration} {disableHoverableContent} {...restProps} />
</TooltipPrimitive.Provider>
