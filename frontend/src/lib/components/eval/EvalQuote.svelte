<!--
@component
One block of captured conversation on the Review labels page, framed and headed by who said
it and when ("You wrote, 11:32", "Claude's last message, 11:52"), so it never reads as the page's
own copy. Markdown code spans inside a quote render as plain text, not as the app's value chips.
`key` gives the frame the blue key-evidence edge; `scroll` holds a long text in a ten-line scroll
box with every character still reachable.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { BellRing, Bot, User } from '@lucide/svelte';

	interface Props {
		who: 'you' | 'claude' | 'guard';
		heading: string;
		key?: boolean;
		scroll?: boolean;
		testid?: string;
		meta?: Snippet;
		children: Snippet;
	}

	let { who, heading, key = false, scroll = false, testid, meta, children }: Props = $props();
</script>

<figure class="eq edge" class:edge-blue={key} data-who={who}>
	<figcaption class="band-hd band-hd--static">
		{#if who === 'you'}<User size={13} aria-hidden="true" />{:else if who === 'guard'}<BellRing
				size={13}
				aria-hidden="true"
			/>{:else}<Bot size={13} aria-hidden="true" />{/if}
		<span class="machine eq-who">{heading}</span>
		{#if meta}<span class="band-hd__meta eq-meta">{@render meta()}</span>{/if}
	</figcaption>
	{#if scroll}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scroll container must take focus so the keyboard can scroll it -->
		<div
			class="band-body eq-body eq-scroll"
			data-testid={testid}
			tabindex="0"
			role="region"
			aria-label={heading}
		>
			{@render children()}
		</div>
	{:else}
		<div class="band-body eq-body" data-testid={testid}>{@render children()}</div>
	{/if}
</figure>

<style>
	.eq {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		margin: 0;
		min-width: 0;
	}
	.eq:not(.edge-blue) {
		--edge-surface: var(--surface-2);
	}
	.eq-who {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.eq-meta {
		min-width: 0;
		overflow-x: auto;
		color: var(--status-info-fg);
	}
	.eq-body {
		min-width: 0;
		font-size: var(--text-md);
		line-height: var(--leading-body);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.eq-scroll {
		max-block-size: 10lh;
		overflow-y: auto;
	}
	.eq-scroll:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.eq :global(.md-code) {
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
	}
</style>
