<!--
@component
Renders text through the app's markdown renderer (`parseSimpleMarkdown`): headings, ordered and
bullet items, fenced code, inline code, bold and links. Nothing is emitted as raw HTML — every
segment is a Svelte text node, and a link only becomes an anchor for an http(s)/mailto target
(`isSafeHref`), so model-written text cannot inject markup or a script URL.

Size and line height inherit from the host, so a chat column and a reading modal can share it;
everything inside scales in `em`. Re-parsing on every streamed delta is cheap and keyed by
position, so a growing reply updates in place, and an unclosed ``` fence renders as a code
block until its closing fence arrives.
-->
<script lang="ts">
	import { parseSimpleMarkdown, type MdSegment } from '$lib/markdown/simpleMarkdown';
	import { cn } from '$lib/utils';

	interface Props {
		text: string;
		class?: string;
	}

	let { text, class: klass = '' }: Props = $props();

	const blocks = $derived(parseSimpleMarkdown(text));
</script>

{#snippet segments(list: MdSegment[])}
	{#each list as seg, j (j)}
		{#if seg.href !== null}
			<a class="md-link" href={seg.href} target="_blank" rel="noopener noreferrer">{seg.text}</a>
		{:else if seg.code}
			<code class="md-code">{seg.text}</code>
		{:else if seg.bold}
			<strong class="md-bold">{seg.text}</strong>
		{:else}
			{seg.text}
		{/if}
	{/each}
{/snippet}

<div class={cn('md-root', klass)} data-slot="markdown-text">
	{#each blocks as block, i (i)}
		{#if block.kind === 'heading'}
			<p class="md-heading">{@render segments(block.segments)}</p>
		{:else if block.kind === 'ordered'}
			<div class="md-item">
				<span class="md-marker md-marker--num" aria-hidden="true">{block.marker}</span>
				<span>{@render segments(block.segments)}</span>
			</div>
		{:else if block.kind === 'bullet'}
			<div class="md-item">
				<span class="md-marker md-marker--dot" aria-hidden="true"></span>
				<span>{@render segments(block.segments)}</span>
			</div>
		{:else if block.kind === 'code'}
			<pre class="md-codeblock">{block.text}</pre>
		{:else}
			<p class="md-p">{@render segments(block.segments)}</p>
		{/if}
	{/each}
</div>

<style>
	.md-root {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
		font-family: var(--font-mono);
		color: var(--text-1);
		white-space: normal;
	}

	.md-p {
		margin: 0;
		overflow-wrap: anywhere;
		white-space: pre-wrap;
	}

	.md-heading {
		margin: var(--space-1) 0 0;
		padding-bottom: var(--space-1);
		border-bottom: var(--hairline) solid var(--border-subtle);
		font-family: var(--font-display);
		font-size: 1.08em;
		font-weight: 800;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-2);
	}

	.md-item {
		display: grid;
		grid-template-columns: 1.6em minmax(0, 1fr);
		gap: var(--space-1);
		overflow-wrap: anywhere;
		white-space: pre-wrap;
	}

	.md-marker {
		color: var(--text-3);
	}
	.md-marker--num {
		font-weight: 600;
	}
	.md-marker--dot::before {
		content: '';
		display: inline-block;
		width: 0.4em;
		height: 0.4em;
		vertical-align: 0.2em;
		background: currentColor;
	}

	.md-codeblock {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		background: var(--surface-code);
		border: var(--hairline) solid var(--border-subtle);
		font-family: var(--font-mono);
		font-size: 0.92em;
		line-height: 1.55;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.md-code {
		font-family: var(--font-mono);
		font-size: 0.92em;
		padding: 0 var(--space-1);
		background: var(--surface-2);
		color: var(--data-teal-fg);
		box-shadow: inset 0 0 0 var(--hairline) var(--border-default);
	}

	.md-bold {
		font-weight: 700;
		color: var(--text-strong);
	}

	.md-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
		overflow-wrap: anywhere;
	}
	.md-link:hover {
		color: var(--text-strong);
	}
	.md-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
</style>
