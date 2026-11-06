<script lang="ts">
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { ArrowRight } from '@lucide/svelte';

	export type DestinationGroupAccent = 'levers' | 'insight' | 'system';

	type DestinationOpenHref =
		| '/sessions'
		| '/analysis'
		| '/prompts'
		| '/agents'
		| '/tools'
		| '/diagnostics'
		| '/projects'
		| '/filter'
		| '/filter/workbench'
		| '/memory'
		| '/skills'
		| '/steering'
		| '/steerers'
		| '/fleet'
		| '/diagnostics/memory'
		| '/settings'
		| '/help';
	type DestinationHowItWorksHref =
		| '/help/guides/inspecting-requests'
		| '/help/guides/anomalies'
		| '/help/guides/filter-rules'
		| '/help/guides/steering-concepts';

	interface LiveStat {
		value: string;
		label: string;
	}

	interface Props {
		group: DestinationGroupAccent;
		tag: string;
		whatItIs: string;
		why: [string, string];
		stat?: LiveStat;
		openHref: DestinationOpenHref;
		howItWorksHref?: DestinationHowItWorksHref;
		children?: Snippet;
	}

	const { group, tag, whatItIs, why, stat, openHref, howItWorksHref }: Props = $props();

	const ACCENT_VAR: Record<DestinationGroupAccent, { fg: string; bg: string; border: string }> = {
		levers: {
			fg: 'var(--status-danger-fg)',
			bg: 'var(--status-danger-bg)',
			border: 'var(--status-danger-border)'
		},
		insight: {
			fg: 'var(--data-teal-fg)',
			bg: 'var(--data-teal-bg)',
			border: 'var(--data-teal-border)'
		},
		system: {
			fg: 'var(--status-info-fg)',
			bg: 'var(--status-info-bg)',
			border: 'var(--status-info-border)'
		}
	};
	const accent = $derived(ACCENT_VAR[group]);
</script>

<div
	class="dc"
	data-slot="destination-card"
	data-motion="hover"
	data-augmented-ui="tl-clip br-clip border"
	style="--acc-fg: {accent.fg}; --acc-bg: {accent.bg}; --acc-border: {accent.border}; --aug-border-bg: linear-gradient(135deg, {accent.fg} 0%, {accent.border} 35%, var(--border-default) 70%);"
>
	<span class="dc-tag-backing">
		<span class="dc-tag" data-augmented-ui="tl-clip br-clip border">{tag}</span>
	</span>
	<div class="dc-rule" aria-hidden="true"></div>
	<p class="dc-what">{whatItIs}</p>
	<ul class="dc-why">
		{#each why as bullet (bullet)}
			<li>{bullet}</li>
		{/each}
	</ul>
	{#if stat}
		<p class="dc-stat"><strong>{stat.value}</strong> {stat.label}</p>
	{/if}
	<div class="dc-learn">
		<a
			class="dc-learn-link dc-learn-link--open"
			data-motion="hover"
			href={resolve(openHref)}
			aria-label={`Open ${tag}`}
		>
			Open <ArrowRight class="dc-open-arrow size-3" data-motion="hover" aria-hidden="true" />
		</a>
		{#if howItWorksHref}
			<a
				class="dc-learn-link"
				data-motion="hover"
				href={resolve(howItWorksHref)}
				aria-label={`How ${tag} works`}>How it works</a
			>
		{/if}
	</div>
</div>

<style>
	.dc {
		--edge-tl: var(--edge-sm);
		--edge-br: var(--edge-md);
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
		background: var(--surface-card);
		padding: calc(var(--edge-tl) + var(--space-1)) calc(var(--edge-br) + var(--space-1))
			calc(var(--edge-br) + var(--space-1)) calc(var(--edge-tl) + var(--space-1));
	}
	.dc:focus-within {
		transform: translateY(calc(-1 * var(--space-1)));
	}
	.dc-learn-link:hover :global(.dc-open-arrow),
	.dc-learn-link:focus-visible :global(.dc-open-arrow) {
		transform: translateX(var(--space-1));
	}
	.dc:hover,
	.dc:focus-within {
		z-index: 1;
		--aug-border-bg: linear-gradient(135deg, var(--acc-fg), var(--border-default) 45%);
	}

	.dc-tag-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-card);
	}

	.dc-tag {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-tr: 0;
		--aug-br: var(--edge);
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--acc-border);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--acc-fg);
		padding: var(--space-1) calc(var(--edge) + var(--space-1));
		background: linear-gradient(0deg, var(--acc-bg), var(--acc-bg)), var(--surface-card);
	}

	.dc-rule {
		height: 1px;
		background: linear-gradient(90deg, var(--acc-border), transparent);
	}

	.dc-what {
		margin: 0;
		font-size: var(--text-xs, 0.75rem);
		line-height: 1.5;
		color: var(--text-1);
	}

	.dc-why {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.dc-why li {
		position: relative;
		padding-left: 14px;
		font-size: var(--text-label);
		line-height: 1.5;
		color: var(--text-2);
	}
	.dc-why li::before {
		content: '';
		position: absolute;
		left: 0;
		top: 0.45em;
		width: 6px;
		height: 6px;
		background: var(--acc-fg);
		clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
	}

	.dc-stat {
		margin: 0;
		font-size: var(--text-label);
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}
	.dc-stat strong {
		color: var(--text-strong);
		font-weight: 700;
	}

	.dc-learn {
		margin-top: auto;
		padding-top: 4px;
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex-wrap: wrap;
		border-top: 1px solid var(--border-subtle);
	}
	.dc-learn-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: var(--control-h);
		font-size: var(--text-label);
		color: var(--text-3);
		text-decoration: none;
		outline: none;
	}
	.dc-learn-link:hover,
	.dc-learn-link:focus-visible {
		color: var(--text-strong);
	}
	.dc-learn-link:focus-visible {
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 2px;
	}
	.dc-learn-link--open {
		color: var(--acc-fg);
		font-weight: 600;
	}
	.dc-learn-link--open:hover,
	.dc-learn-link--open:focus-visible {
		color: var(--acc-fg);
		text-decoration: underline;
	}
</style>
