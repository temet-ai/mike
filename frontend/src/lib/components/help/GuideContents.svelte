<script lang="ts">
	import { ChevronDown, Check } from '@lucide/svelte';
	import { MediaQuery } from 'svelte/reactivity';
	let {
		items,
		active,
		progress
	}: {
		items: Array<{ slug: string; label: string }>;
		active: string;
		progress: number;
	} = $props();
	const compact = new MediaQuery('(max-width: 60rem)');
	let expanded = $state(false);
	const activeIndex = $derived(items.findIndex((item) => item.slug === active));
</script>

<aside class="guide-contents" aria-label="Section table of contents">
	<div class="reading-status">
		<span>Reading progress</span><span>{Math.round(progress)}%</span>
	</div>
	<progress max="100" value={progress} aria-label="Reading progress"></progress>
	<details
		class="funk-disclosure"
		open={!compact.current || expanded}
		ontoggle={(event) => {
			if (compact.current) expanded = event.currentTarget.open;
		}}
	>
		<summary>On this page <ChevronDown class="size-4" aria-hidden="true" /></summary>
		<nav aria-label="Guide sections">
			<ol>
				{#each items as item, index (item.slug)}
					<li>
						<a
							href="#{item.slug}"
							aria-current={active === item.slug ? 'location' : undefined}
							onclick={() => {
								expanded = false;
							}}
						>
							<span class="step-number" class:complete={index < activeIndex} aria-hidden="true">
								{#if index < activeIndex}<Check class="size-3" />{:else}{String(index + 1).padStart(
										2,
										'0'
									)}{/if}
							</span>
							<span>{item.label}</span>
						</a>
					</li>
				{/each}
			</ol>
		</nav>
	</details>
</aside>

<style>
	.guide-contents {
		position: sticky;
		top: 44px;
		min-width: 0;
		padding: var(--space-4);
		border: var(--hairline) solid var(--border-default);
		background: var(--surface-card);
	}
	.reading-status {
		display: flex;
		justify-content: space-between;
		gap: var(--space-2);
		font: var(--text-xs) var(--font-machine);
		letter-spacing: var(--track-machine);
		text-transform: uppercase;
		color: var(--text-3);
	}
	progress {
		display: block;
		width: 100%;
		height: var(--space-1);
		margin-block: var(--space-3) var(--space-5);
		appearance: none;
		border: 0;
		background: var(--surface-3);
		color: var(--funk-cyan);
	}
	progress::-webkit-progress-bar {
		background: var(--surface-3);
	}
	progress::-webkit-progress-value {
		background: var(--funk-cyan);
		transition: width var(--dur-1) var(--ease-io);
	}
	progress::-moz-progress-bar {
		background: var(--funk-cyan);
	}
	summary {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding-block: var(--space-2);
		cursor: pointer;
		font: var(--text-sm) var(--font-machine);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-1);
		list-style: none;
	}
	summary::-webkit-details-marker {
		display: none;
	}
	summary :global(svg) {
		transition: rotate var(--dur-2) var(--ease-snap);
	}
	details[open] summary :global(svg) {
		rotate: 180deg;
	}
	ol {
		display: flex;
		flex-direction: column;
		list-style: none;
		margin: var(--space-3) 0 0;
		padding: 0;
	}
	a {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-2);
		min-height: calc(var(--control-h-lg) + var(--space-1));
		border-left: var(--hairline) solid var(--border-default);
		color: var(--text-2);
		text-decoration: none;
		font-size: var(--text-sm);
		transition:
			background-color var(--dur-1),
			color var(--dur-1),
			border-color var(--dur-1);
	}
	a:hover {
		background: var(--surface-2);
		color: var(--text-1);
	}
	a[aria-current] {
		border-left-color: var(--funk-cyan);
		background: var(--funk-chip-cyan);
		color: var(--funk-cyan);
	}
	.step-number {
		display: inline-flex;
		justify-content: center;
		align-items: center;
		min-width: var(--space-4);
		font-size: var(--text-xs);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
	.complete {
		color: var(--funk-green);
	}
	@media (max-width: 60rem) {
		.guide-contents {
			position: static;
		}
		progress {
			margin-bottom: var(--space-2);
		}
	}
</style>
