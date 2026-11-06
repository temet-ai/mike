<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import CopyIconButton from '../shared/CopyIconButton.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		lang: string;
		code: string;
		copyable?: boolean;
	}

	let {
		ref = $bindable(null),
		class: className,
		lang,
		code,
		copyable = true,
		...restProps
	}: Props = $props();
</script>

<div
	bind:this={ref}
	data-slot="message-view-code-block"
	class={cn('code-block', className)}
	{...restProps}
>
	<div class="bar">
		<span class="lang">{lang}</span>
		{#if copyable}
			<CopyIconButton getText={() => code} label="Copy code" copiedLabel="Copied" />
		{/if}
	</div>
	<pre>{code}</pre>
</div>
