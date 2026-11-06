<script lang="ts">
	import { Dialog as DialogPrimitive } from 'bits-ui';
	import { cn } from '$lib/utils';

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: DialogPrimitive.OverlayProps = $props();
</script>

<DialogPrimitive.Overlay
	bind:ref
	data-slot="sheet-overlay"
	class={cn('fixed inset-0 isolate z-50 bg-[var(--surface-bg)]/80', className)}
	{...restProps}
/>
