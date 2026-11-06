<script lang="ts" module>
	export type ShareBarTone = 'live' | 'info' | 'active' | 'warn';

	export interface ShareBarSegment {
		value: number;
		tone: ShareBarTone;
	}
</script>

<script lang="ts">
	import { cn } from '$lib/utils';

	interface Props {
		segments: ShareBarSegment[];
		max?: number;
		size?: 'row' | 'split';
		class?: string;
	}

	let { segments, max, size = 'row', class: className }: Props = $props();

	const total = $derived(max ?? segments.reduce((sum, seg) => sum + Math.max(0, seg.value), 0));

	function widthOf(value: number): number {
		if (!(total > 0)) return 0;
		return Math.min(100, (Math.max(0, value) / total) * 100);
	}
</script>

<span class={cn('sbar', className)} data-slot="share-bar" data-size={size} aria-hidden="true">
	{#each segments as seg, i (i)}
		<span
			class="sbar__seg"
			data-slot="share-bar-segment"
			data-tone={seg.tone}
			style:width="{widthOf(seg.value)}%"
		></span>
	{/each}
</span>

<style>
	.sbar {
		display: flex;
		min-width: 0;
		height: 8px;
		background: var(--surface-2);
	}
	.sbar[data-size='split'] {
		height: 12px;
	}
	.sbar__seg {
		display: block;
		height: 100%;
		flex: none;
	}
	.sbar__seg[data-tone='live'] {
		background: var(--signal-live);
	}
	.sbar__seg[data-tone='info'] {
		background: var(--signal-info);
	}
	.sbar__seg[data-tone='active'] {
		background: var(--signal-active);
	}
	.sbar__seg[data-tone='warn'] {
		background: var(--signal-warn);
	}
</style>
