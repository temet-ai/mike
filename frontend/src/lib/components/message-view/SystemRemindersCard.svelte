<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import { Bell } from '@lucide/svelte';
	import { formatCount } from '$lib/time';
	import { formatTokens } from './parseSystemPrompt';

	interface Props extends WithElementRef<HTMLAttributes<HTMLElement>> {
		groupCount: number;
		totalChars: number;
		totalTokens: number;
		children: Snippet;
	}

	let {
		ref = $bindable(null),
		class: className,
		groupCount,
		totalChars,
		totalTokens,
		children,
		...restProps
	}: Props = $props();
</script>

<article
	bind:this={ref}
	data-slot="message-view-reminders"
	data-augmented-ui="tl-clip br-clip border"
	class={cn('reminders', className)}
	aria-label="System reminders"
	style="--aug-border-bg: linear-gradient(135deg, var(--status-info-fg) 0%, var(--status-info-border) 35%, var(--border-default) 70%);"
	{...restProps}
>
	<div class="reminders__head">
		<span class="reminders__label">
			<Bell size={12} class="ico" aria-hidden="true" />
			System reminders
		</span>
		<span class="reminders__stats">
			<span>{groupCount} group{groupCount === 1 ? '' : 's'}</span>
			<span class="dot-sep" aria-hidden="true">·</span>
			<span>{formatTokens(totalTokens)}</span>
		</span>
	</div>
	<div class="reminders__body">
		{@render children()}
	</div>
</article>

<style>
	:global(.reminders__label) {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		color: var(--text-3);
		font-weight: 600;
		font-size: var(--text-label);
		line-height: 1;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
	}
	:global(.reminders__label .ico) {
		color: var(--text-3);
	}
	:global(.reminders__stats) {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--text-2);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
	}
	:global(.reminders__stats .dot-sep) {
		color: var(--text-faint);
	}
</style>
