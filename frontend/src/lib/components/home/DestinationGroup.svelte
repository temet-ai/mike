<script lang="ts">
	import type { DestinationGroupAccent } from './DestinationCard.svelte';
	import type { Snippet } from 'svelte';

	interface Props {
		accent: DestinationGroupAccent;
		title: string;
		purpose: string;
		children: Snippet;
	}

	const { accent, title, purpose, children }: Props = $props();

	function measureGridEdges(grid: HTMLElement) {
		const cards = Array.from(grid.children) as HTMLElement[];
		const rects = cards.map((c) => c.getBoundingClientRect());

		cards.forEach((card, i) => {
			const r = rects[i]!;
			let hasLeft = false;
			let hasTop = false;
			let hasRight = false;
			let hasBottom = false;
			rects.forEach((o, j) => {
				if (j === i) return;
				const rowOverlap = o.top < r.bottom - 1 && r.top < o.bottom - 1;
				const colOverlap = o.left < r.right - 1 && r.left < o.right - 1;
				if (rowOverlap && Math.abs(o.right - r.left) < 1) hasLeft = true;
				if (rowOverlap && Math.abs(o.left - r.right) < 1) hasRight = true;
				if (colOverlap && Math.abs(o.bottom - r.top) < 1) hasTop = true;
				if (colOverlap && Math.abs(o.top - r.bottom) < 1) hasBottom = true;
			});
			card.toggleAttribute('data-edge-tl-free', !hasLeft && !hasTop);
			card.toggleAttribute('data-edge-br-free', !hasRight && !hasBottom);
		});
	}

	function gridEdgeAction(grid: HTMLElement) {
		const ro = new ResizeObserver(() => measureGridEdges(grid));
		ro.observe(grid);
		measureGridEdges(grid);
		return { destroy: () => ro.disconnect() };
	}

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
	const acc = $derived(ACCENT_VAR[accent]);

	const SHAPE_FAMILY: Record<DestinationGroupAccent, string> = {
		insight: 'tl-clip tr-2-clip-y br-2-clip-x bl-clip both',
		levers: 'tl-step-inset tr-2-step-inset br-2-step-inset bl-step-inset l-clip both',
		system: 'tl-rect tr-2-rect-y br-2-rect-x bl-rect both'
	};
	const EDGE_SIZE: Record<DestinationGroupAccent, string> = {
		insight: '0px',
		levers: 'var(--edge-md)',
		system: '0px'
	};
</script>

<section
	class="dg"
	data-reveal
	data-slot="destination-group"
	data-accent={accent}
	data-augmented-ui={SHAPE_FAMILY[accent]}
	style="--acc-fg: {acc.fg}; --acc-bg: {acc.bg}; --acc-border: {acc.border}; --aug-border-bg: linear-gradient(90deg, {acc.fg} 0%, {acc.border} 26%, var(--border-default) 55%); --aug-inlay-bg: {acc.bg}; --aug-l: {EDGE_SIZE[
		accent
	]};"
	aria-label={title}
>
	<header class="dg-head">
		<span class="dg-title-backing">
			<h2
				class="dg-title"
				data-augmented-ui="tr-clip bl-clip border"
				style="--aug-border-bg: {acc.border};"
			>
				{title}
			</h2>
		</span>
		<p class="dg-purpose">{purpose}</p>
	</header>
	<div class="dg-grid" use:gridEdgeAction>
		{@render children()}
	</div>
</section>

<style>
	.dg {
		--edge: var(--edge-md);
		--aug-tl: var(--edge);
		--aug-tr1: var(--edge-sm);
		--aug-tr2: var(--edge-lg);
		--aug-br1: var(--edge-lg);
		--aug-br2: var(--edge-sm);
		--aug-bl: var(--edge);
		--aug-border-all: 1.5px;
		--aug-inlay-all: 3px;
		padding: calc(var(--edge) + var(--space-1));
		background: linear-gradient(105deg, var(--acc-bg), transparent 34%), var(--surface-bg);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		min-width: 0;
	}

	.dg-head {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.dg-title-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-bg);
	}

	.dg-title {
		--aug-border-all: 1px;
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: var(--weight-display);
		font-size: var(--text-md);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--acc-fg);
		--edge: var(--edge-sm);
		padding: calc(var(--edge) + var(--space-1));
		background: linear-gradient(0deg, var(--acc-bg), var(--acc-bg)), var(--surface-bg);
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
	}

	.dg-purpose {
		margin: 0;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-2);
	}

	.dg-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 18.5rem), 1fr));
		gap: 0;
		align-items: stretch;
	}
	.dg-grid :global(.dc) {
		--aug-tl: 0px;
		--aug-br: 0px;
	}
	.dg-grid :global(.dc[data-edge-tl-free]) {
		--aug-tl: var(--edge-tl);
	}
	.dg-grid :global(.dc[data-edge-br-free]) {
		--aug-br: var(--edge-br);
	}
</style>
