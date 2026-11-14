<script lang="ts" module>
	export type StatusMarkTone = 'success' | 'danger' | 'warning' | 'info' | 'strong' | 'quiet';
</script>

<script lang="ts">
	import type { Component } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn } from '$lib/utils';

	type GlyphComponent = Component<{
		size?: number | string;
		class?: string;
		'aria-hidden'?: 'true' | boolean;
	}>;

	type StatusMarkProps = Omit<HTMLAttributes<HTMLSpanElement>, 'class' | 'children'> & {
		ref?: HTMLSpanElement | null;
		class?: string;
		icon: GlyphComponent;
		label: string;
		tone: StatusMarkTone;
		size?: number;
	};

	let {
		ref = $bindable(null),
		class: className,
		icon: Glyph,
		label,
		tone,
		size = 12,
		...restProps
	}: StatusMarkProps = $props();
</script>

<span
	bind:this={ref}
	data-slot="status-mark"
	data-tone={tone}
	class={cn('status-mark', className)}
	{...restProps}
>
	<span class="status-mark__glyph" style="--status-mark-size: {size}px;">
		<Glyph {size} aria-hidden="true" />
	</span>
	<span class="status-mark__label">{label}</span>
</span>

<style>
	.status-mark {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		line-height: var(--leading-tight);
		color: var(--status-mark-fg);
		overflow-wrap: anywhere;
	}
	.status-mark__glyph {
		display: inline-flex;
		flex: none;
		align-items: center;
		justify-content: center;
		inline-size: var(--status-mark-size);
		block-size: var(--status-mark-size);
	}
	.status-mark[data-tone='success'] {
		--status-mark-fg: var(--status-success-fg);
	}
	.status-mark[data-tone='danger'] {
		--status-mark-fg: var(--status-danger-fg);
	}
	.status-mark[data-tone='warning'] {
		--status-mark-fg: var(--status-warning-fg);
	}
	.status-mark[data-tone='info'] {
		--status-mark-fg: var(--status-info-fg);
	}
	.status-mark[data-tone='strong'] {
		--status-mark-fg: var(--text-1);
	}
	.status-mark[data-tone='quiet'] {
		--status-mark-fg: var(--text-3);
	}
</style>
