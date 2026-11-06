<script lang="ts">
	import { ArrowUpRight, ScanSearch, SlidersHorizontal, Network } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	const paths = [
		{
			title: 'Understand a session',
			detail: 'Follow the conversation, inspect a request and see what changed.',
			href: resolve('/sessions'),
			icon: ScanSearch,
			label: 'Inspect'
		},
		{
			title: 'Shape what happens next',
			detail: 'Build a rewrite rule and preview its effect before applying it.',
			href: resolve('/filter/workbench'),
			icon: SlidersHorizontal,
			label: 'Create'
		},
		{
			title: 'See the team at work',
			detail: 'Explore connected agents, their roles and the work moving between them.',
			href: resolve('/fleet'),
			icon: Network,
			label: 'Explore'
		}
	];
</script>

<section class="launchpad" data-slot="home-launchpad" aria-labelledby="launchpad-title">
	<div class="launchpad-heading" data-reveal>
		<h2 id="launchpad-title">Where do you want to go?</h2>
		<span>Pick a task. Follow the trail.</span>
	</div>
	<div class="launchpad-grid">
		{#each paths as path, index (path.href)}
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- paths contains resolved literal routes -->
			<a
				href={path.href}
				class="launch-card"
				data-reveal
				data-motion="hover"
				data-augmented-ui="tl-clip br-clip border"
			>
				<div class="launch-meta">
					<span class="launch-icon"><path.icon class="size-5" aria-hidden="true" /></span><span
						>0{index + 1} / {path.label}</span
					><ArrowUpRight class="launch-arrow size-4" data-motion="hover" aria-hidden="true" />
				</div>
				<h3>{path.title}</h3>
				<p>{path.detail}</p>
			</a>
		{/each}
	</div>
</section>

<style>
	.launchpad {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.launchpad-heading {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
	}
	h2 {
		margin: 0;
		font: var(--weight-display) var(--text-xl) var(--font-display);
		color: var(--text-1);
	}
	.launchpad-heading > span {
		font-size: var(--text-sm);
		color: var(--text-3);
	}
	.launchpad-grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
	}
	.launch-card {
		--aug-tl: var(--edge-md);
		--aug-br: var(--edge-md);
		--aug-border-all: var(--hairline);
		--aug-border-bg: var(--border-strong);
		display: flex;
		position: relative;
		flex-direction: column;
		gap: var(--space-3);
		padding: calc(var(--edge-md) + var(--space-2));
		color: var(--text-1);
		text-decoration: none;
		background: var(--surface-card);
	}
	.launch-card::after {
		content: '';
		position: absolute;
		inset: auto var(--space-4) 0;
		height: var(--space-1);
		background: var(--funk-cyan);
		transform: scaleX(0);
		transform-origin: left;
	}
	.launch-card:is(:hover, :focus-visible) {
		background: var(--surface-2);
		--aug-border-bg: var(--funk-cyan);
		outline-offset: -2px;
	}
	.launch-card:is(:hover, :focus-visible)::after {
		transform: scaleX(1);
	}
	.launch-meta {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		color: var(--text-3);
		font: var(--text-xs) var(--font-machine);
		letter-spacing: var(--track-machine);
		text-transform: uppercase;
	}
	.launch-icon {
		display: grid;
		place-items: center;
		width: var(--control-h-lg);
		height: var(--control-h-lg);
		background: var(--funk-chip-cyan);
		color: var(--funk-cyan);
	}
	.launch-meta :global(.launch-arrow) {
		margin-left: auto;
	}
	.launch-card:is(:hover, :focus-visible) :global(.launch-arrow) {
		color: var(--funk-cyan);
		translate: var(--space-1) calc(-1 * var(--space-1));
	}
	h3 {
		margin: 0;
		font: var(--weight-display) var(--text-lg) var(--font-display);
	}
	p {
		margin: 0;
		color: var(--text-2);
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		max-width: 48ch;
	}
	@media (max-width: 64rem) {
		.launchpad-grid {
			grid-template-columns: 1fr;
		}
		.launch-card {
			gap: var(--space-2);
		}
	}
</style>
