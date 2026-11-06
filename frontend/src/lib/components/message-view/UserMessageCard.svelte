<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { cn, type WithElementRef } from '$lib/utils';
	import { formatCount } from '$lib/time';
	import CopyIconButton from '../shared/CopyIconButton.svelte';
	import { estimateTokens, formatTokens } from './parseSystemPrompt';

	interface Props extends WithElementRef<HTMLAttributes<HTMLElement>> {
		text: string;
	}

	let { ref = $bindable(null), class: className, text, ...restProps }: Props = $props();

	const chars = $derived(text.length);
	const tokens = $derived(estimateTokens(chars));
</script>

<div
	bind:this={ref}
	role="group"
	data-slot="message-view-you"
	data-augmented-ui="tl-clip br-clip border"
	class={cn('you', className)}
	aria-label="User message"
	style="--aug-border-bg: linear-gradient(135deg, var(--status-info-fg) 0%, var(--status-info-border) 35%, var(--border-default) 70%);"
	{...restProps}
>
	<div class="you__head">
		<span class="you__label">USER</span>
		<span class="you__stats">
			<span>{formatTokens(tokens)}</span>
		</span>
		<span class="you__copy">
			<CopyIconButton getText={() => text} label="Copy message" copiedLabel="Copied message" />
		</span>
	</div>
	<pre class="you__body">{text}</pre>
</div>

<style>
	:global(.you__head) {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	:global(.you__label) {
		color: var(--status-info-fg);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
	}
	:global(.you__stats) {
		color: var(--text-2);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
	}
	:global(.you__stats .sep) {
		color: var(--text-faint);
	}
	:global(.you__copy) {
		margin-left: auto;
	}
	:global(.you__body) {
		margin: 8px 0 0;
		color: var(--text-strong);
		font-family: var(--font-mono);
		font-size: var(--text-md);
		line-height: 1.55;
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
