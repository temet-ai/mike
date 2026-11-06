<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		id?: string;
		level?: 'h2' | 'h3';
		badge?: string;
		badgeVariant?: 'session' | 'restart';
		description?: Snippet;
		action?: Snippet;
		testId?: string;
		children: Snippet;
	}

	const {
		title,
		id,
		level = 'h2',
		badge,
		badgeVariant = 'session',
		description,
		action,
		testId,
		children
	}: Props = $props();

	const headingId = $derived(`cfg-sec-${id ?? title.toLowerCase().replace(/\s+/g, '-')}`);
</script>

<section
	class="cfg-sec"
	{id}
	aria-labelledby={headingId}
	data-augmented-ui="tl-clip br-clip border"
	data-reveal
>
	<div class="cfg-sec-head">
		<svelte:element this={level} class="cfg-sec-title" id={headingId}>
			{#if id}
				<a href="#{id}" class="cfg-sec-anchor">{title}</a>
			{:else}
				{title}
			{/if}
		</svelte:element>
		{#if badge}
			<span class="cfg-sec-badge cfg-sec-badge--{badgeVariant}">{badge}</span>
		{/if}
		<span class="cfg-sec-fill"></span>
		{#if action}
			<span class="cfg-sec-action">{@render action()}</span>
		{/if}
	</div>

	<div class="cfg-sec-body" data-testid={testId} role="group" aria-labelledby={headingId}>
		{#if description}
			<p class="cfg-sec-desc">{@render description()}</p>
		{/if}
		{@render children()}
	</div>
</section>

<style>
	.cfg-sec {
		--edge: var(--edge-md);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--border-default);
		display: flex;
		flex-direction: column;
		min-width: 0;
		background: var(--surface-card);
		padding: var(--hairline);
		scroll-margin-top: var(--space-8);
	}
	.cfg-sec-head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3) calc(var(--edge) + var(--space-2));
		background: var(--surface-2);
		border-bottom: var(--hairline) solid var(--border-default);
	}
	.cfg-sec-title {
		font-family: var(--font-machine);
		font-size: var(--text-sm);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-strong);
		margin: 0;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.cfg-sec-anchor {
		color: inherit;
		text-decoration: none;
		text-underline-offset: var(--space-1);
	}
	.cfg-sec-anchor:hover {
		color: var(--interactive-active-fg);
		text-decoration: underline;
	}
	.cfg-sec-anchor:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.cfg-sec-badge {
		display: inline-flex;
		align-items: center;
		padding: var(--space-1) var(--space-2);
		border-radius: var(--r-tol);
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		overflow-wrap: anywhere;
	}
	.cfg-sec-badge--session {
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		border: var(--hairline) solid var(--status-info-border);
	}
	.cfg-sec-badge--restart {
		background: var(--status-warning-bg);
		color: var(--status-warning-fg);
		border: var(--hairline) solid var(--status-warning-border);
	}
	.cfg-sec-fill {
		flex: 1 1 auto;
	}
	.cfg-sec-action {
		display: inline-flex;
		align-items: center;
	}
	.cfg-sec-body {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.cfg-sec-desc {
		margin: 0;
		padding: var(--space-3) calc(var(--edge) + var(--space-2));
		border-bottom: var(--hairline) solid var(--border-subtle);
		font-size: var(--text-sm);
		color: var(--text-2);
		line-height: var(--leading-body);
		overflow-wrap: anywhere;
	}
</style>
