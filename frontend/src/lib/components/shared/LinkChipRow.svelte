<script lang="ts" module>
	import type { Component } from 'svelte';

	export interface LinkChip {
		icon: Component;
		/** Small-caps word before the text, such as `Problem` or `Action`. */
		kind?: string;
		text: string;
		/**
		 * An app path (with any query), an in-page `#fragment`, or, with `external`, a full URL.
		 * Absent and no `onclick`: drawn as a label.
		 */
		href?: string;
		external?: boolean;
		onclick?: () => void;
		title?: string;
	}
</script>

<script lang="ts">
	import { resolveAppPath } from '$lib/resolveHref';

	interface Props {
		items: LinkChip[];
		label?: string;
	}

	const { items, label }: Props = $props();
</script>

{#if items.length > 0}
	<ul class="link-chips" role="list" aria-label={label} data-slot="link-chip-row">
		{#each items as item, i (item.text + i)}
			{@const Icon = item.icon}
			<li>
				{#if item.href && item.external}
					<a
						class="link-chip"
						href={item.href}
						target="_blank"
						rel="external noopener noreferrer"
						title={item.title}
					>
						<Icon size={11} aria-hidden="true" />
						{#if item.kind}<span class="link-chip__kind">{item.kind}</span>{/if}
						{item.text}
					</a>
				{:else if item.href?.startsWith('#')}
					<a class="link-chip" href={`#${item.href.slice(1)}`} title={item.title}>
						<Icon size={11} aria-hidden="true" />
						{#if item.kind}<span class="link-chip__kind">{item.kind}</span>{/if}
						{item.text}
					</a>
				{:else if item.href}
					<a class="link-chip" href={resolveAppPath(item.href)} title={item.title}>
						<Icon size={11} aria-hidden="true" />
						{#if item.kind}<span class="link-chip__kind">{item.kind}</span>{/if}
						{item.text}
					</a>
				{:else if item.onclick}
					<button
						type="button"
						class="link-chip link-chip--action"
						onclick={item.onclick}
						title={item.title}
					>
						<Icon size={11} aria-hidden="true" />
						{#if item.kind}<span class="link-chip__kind">{item.kind}</span>{/if}
						{item.text}
					</button>
				{:else}
					<span class="link-chip link-chip--label" title={item.title}>
						<Icon size={11} aria-hidden="true" />
						{#if item.kind}<span class="link-chip__kind">{item.kind}</span>{/if}
						{item.text}
					</span>
				{/if}
			</li>
		{/each}
	</ul>
{/if}

<style>
	.link-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.link-chip {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		text-decoration: none;
		cursor: pointer;
		border: none;
		overflow-wrap: anywhere;
		text-align: left;
	}

	a.link-chip:hover,
	button.link-chip--action:hover {
		--aug-border-bg: var(--interactive-active-border);
		color: var(--text-strong);
	}

	.link-chip--label {
		cursor: default;
		opacity: 0.85;
	}

	.link-chip:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.link-chip__kind {
		white-space: nowrap;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
</style>
