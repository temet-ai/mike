<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import DestinationGroup from './DestinationGroup.svelte';
	import DestinationCard from './DestinationCard.svelte';
	import { seamMarkersAction } from '$lib/actions/seamMarkers';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Home/DestinationGroup',
		component: DestinationGroup,
		tags: ['autodocs']
	});

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

	function gridEdgeAction(root: HTMLElement) {
		const grids = Array.from(root.querySelectorAll<HTMLElement>('.dg-grid'));
		const ro = new ResizeObserver(() => grids.forEach(measureGridEdges));
		grids.forEach((g) => {
			ro.observe(g);
			measureGridEdges(g);
		});
		return { destroy: () => ro.disconnect() };
	}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByRole('heading', { name: 'Insight' })).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'Levers' })).toBeInTheDocument();
		await expect(canvas.getByRole('heading', { name: 'System' })).toBeInTheDocument();

		await expect(canvas.getAllByRole('link', { name: /open/i })).toHaveLength(13);

		const panels = canvasElement.querySelectorAll<HTMLElement>('.dg');
		await expect(panels).toHaveLength(3);
		const [insightEl, leversEl, systemEl] = panels;
		if (!insightEl || !leversEl || !systemEl) {
			throw new Error('expected exactly 3 .dg panels');
		}

		const insightRect = insightEl.getBoundingClientRect();
		const leversRect = leversEl.getBoundingClientRect();
		const systemRect = systemEl.getBoundingClientRect();
		await expect(Math.abs(leversRect.top - insightRect.bottom)).toBeLessThan(1);
		await expect(Math.abs(systemRect.top - leversRect.bottom)).toBeLessThan(1);

		const seamMarkers = Array.from(canvasElement.querySelectorAll<HTMLElement>('.seam-marker'));
		await expect(seamMarkers).toHaveLength(2);
		await expect(Math.abs(leversRect.top - insightRect.bottom)).toBeLessThan(1);
		await expect(Math.abs(systemRect.top - leversRect.bottom)).toBeLessThan(1);
		await expect(Math.abs(insightRect.width - leversRect.width)).toBeLessThan(1);
		await expect(Math.abs(leversRect.width - systemRect.width)).toBeLessThan(1);

		for (const marker of seamMarkers) {
			await expect(marker.dataset.seamAxis).toBe('horizontal-seam');
			const mRect = marker.getBoundingClientRect();
			const seamY =
				mRect.top < insightRect.bottom + 1 && mRect.top > insightRect.top
					? insightRect.bottom
					: leversRect.bottom;
			await expect(Math.abs(mRect.top + mRect.height / 2 - seamY)).toBeLessThan(2);
			const expectedCenterX = insightRect.left + insightRect.width / 2;
			await expect(Math.abs(mRect.left + mRect.width / 2 - expectedCenterX)).toBeLessThan(2);
			await expect(getComputedStyle(marker).clipPath).not.toBe('none');
			const inner = marker.firstElementChild as HTMLElement | null;
			if (!inner) throw new Error('expected seam marker inner fill element');
			await expect(getComputedStyle(inner).clipPath).not.toBe('none');
		}

		await expect(getComputedStyle(insightEl).getPropertyValue('--aug-bl').trim()).toBe('0px');
		await expect(getComputedStyle(insightEl).getPropertyValue('--aug-br1').trim()).toBe('0px');
		await expect(getComputedStyle(leversEl).getPropertyValue('--aug-tl').trim()).toBe('0px');
		await expect(getComputedStyle(leversEl).getPropertyValue('--aug-bl').trim()).toBe('0px');
		await expect(getComputedStyle(systemEl).getPropertyValue('--aug-tl').trim()).toBe('0px');
		await expect(getComputedStyle(systemEl).getPropertyValue('--aug-tr1').trim()).toBe('0px');

		const proofBoxes = Array.from(canvasElement.querySelectorAll<HTMLElement>('.proof-box'));
		await expect(proofBoxes).toHaveLength(3);
		for (const box of proofBoxes) {
			expect(getComputedStyle(box).clipPath).not.toBe('none');
		}

		await expect(insightEl.getAttribute('data-augmented-ui')).toBe(
			'tl-clip tr-2-clip-y br-2-clip-x bl-clip both'
		);
		await expect(leversEl.getAttribute('data-augmented-ui')).toBe(
			'tl-step-inset tr-2-step-inset br-2-step-inset bl-step-inset l-clip both'
		);
		await expect(systemEl.getAttribute('data-augmented-ui')).toBe(
			'tl-rect tr-2-rect-y br-2-rect-x bl-rect both'
		);

		for (const el of [insightEl, leversEl, systemEl]) {
			expect(el.getAttribute('data-augmented-ui') ?? '').not.toContain('l-clip-y');
			expect(getComputedStyle(el).getPropertyValue('--aug-l-extend1').trim()).toBe('');
		}
		await expect(getComputedStyle(leversEl).getPropertyValue('--aug-l').trim()).toBe(
			getComputedStyle(leversEl).getPropertyValue('--edge-md').trim()
		);

		await expect(getComputedStyle(insightEl).clipPath).not.toBe('none');
		await expect(getComputedStyle(systemEl).clipPath).not.toBe('none');

		const insightGrid = insightEl.querySelector<HTMLElement>('.dg-grid');
		if (!insightGrid) throw new Error('expected Insight .dg-grid');
		const insightCards = Array.from(insightGrid.querySelectorAll<HTMLElement>('.dc'));
		await expect(insightCards).toHaveLength(6);
		const insightRects = insightCards.map((c) => c.getBoundingClientRect());
		const sessions = insightRects[0]!;
		const analysis = insightRects[1]!;
		const prompts = insightRects[2]!;
		const agents = insightRects[3]!;
		const tools = insightRects[4]!;
		const diagnostics = insightRects[5]!;
		await expect(Math.abs(analysis.left - sessions.right)).toBeLessThan(1);
		await expect(Math.abs(prompts.top - sessions.bottom)).toBeLessThan(1);
		await expect(Math.abs(tools.top - prompts.bottom)).toBeLessThan(1);
		await expect(Math.abs(agents.top - analysis.bottom)).toBeLessThan(1);
		await expect(Math.abs(diagnostics.top - agents.bottom)).toBeLessThan(1);

		await expect(insightCards[0]?.hasAttribute('data-edge-tl-free')).toBe(true);
		await expect(insightCards[0]?.hasAttribute('data-edge-br-free')).toBe(false);
		await expect(insightCards[5]?.hasAttribute('data-edge-tl-free')).toBe(false);
		await expect(insightCards[5]?.hasAttribute('data-edge-br-free')).toBe(true);
		await expect(insightCards[1]?.hasAttribute('data-edge-tl-free')).toBe(false);
		await expect(insightCards[1]?.hasAttribute('data-edge-br-free')).toBe(false);
		await expect(
			getComputedStyle(insightCards[0] as HTMLElement)
				.getPropertyValue('--aug-tl')
				.trim()
		).toBe(
			getComputedStyle(insightCards[0] as HTMLElement)
				.getPropertyValue('--edge-sm')
				.trim()
		);
		await expect(
			getComputedStyle(insightCards[0] as HTMLElement)
				.getPropertyValue('--aug-br')
				.trim()
		).toBe('0px');
		await expect(
			getComputedStyle(insightCards[1] as HTMLElement)
				.getPropertyValue('--aug-tl')
				.trim()
		).toBe('0px');

		await expect(insightCards[0]?.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		await expect(insightCards[1]?.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');
		await expect(insightCards[5]?.getAttribute('data-augmented-ui')).toBe('tl-clip br-clip border');

		const browserContext = await import('@vitest/browser/context').catch(() => null);
		if (browserContext) {
			const locator = browserContext.page.elementLocator(insightCards[0] as Element);
			await locator.hover();
			await new Promise((r) => setTimeout(r, 200));
			const hoveredCs = getComputedStyle(insightCards[0] as HTMLElement);
			await expect(hoveredCs.transform).not.toBe('none');
			await expect(hoveredCs.zIndex).toBe('1');
			await expect(hoveredCs.boxShadow).not.toBe('none');
			const analysisWhileHovered = insightCards[1]!.getBoundingClientRect();
			await expect(Math.abs(analysisWhileHovered.left - sessions.right)).toBeLessThan(1);
		}
	}}
>
	{#snippet template()}
		<div class="map-preview">
			<div class="groups" use:gridEdgeAction use:seamMarkersAction>
				<DestinationGroup
					accent="insight"
					title="Insight"
					purpose="Understand the traffic — what was captured, explained."
				>
					<DestinationCard
						group="insight"
						tag="Sessions"
						whatItIs="Every session captured — the full log behind the live rows up top, searchable and replayable."
						why={[
							'Reopen a session from last week and read its turns',
							'Search across sessions for a prompt or model'
						]}
						stat={{ value: '812', label: 'sessions stored' }}
						openHref="/sessions"
						howItWorksHref="/help/guides/inspecting-requests"
					/>
					<DestinationCard
						group="insight"
						tag="Analysis"
						whatItIs="Automated findings over captured traffic — overselling, blame-deflection, faked understanding, by category."
						why={[
							'Audit yesterday’s sessions for weasel patterns',
							'Trace a finding to the exact request that produced it'
						]}
						openHref="/analysis"
					/>
					<DestinationCard
						group="insight"
						tag="Prompts"
						whatItIs="The system prompts actually sent — composition layers, anomalies, and how they change over time."
						why={[
							'See exactly what was injected into a turn',
							'Diff a prompt against the previous session’s'
						]}
						openHref="/prompts"
						howItWorksHref="/help/guides/anomalies"
					/>
					<DestinationCard
						group="insight"
						tag="Agents"
						whatItIs="Subagent dispatches — which agents ran, under whom, and what they returned."
						why={[
							'Follow a dispatch from parent turn to returned result',
							'Spot the agent that never reported back'
						]}
						openHref="/agents"
					/>
					<DestinationCard
						group="insight"
						tag="Tools"
						whatItIs="Tool-call traffic — usage stats, errors, and per-session drill-downs."
						why={[
							'Find the tool that fails most across sessions',
							'Replay a failing call’s exact input and result'
						]}
						openHref="/tools"
					/>
					<DestinationCard
						group="insight"
						tag="Diagnostics"
						whatItIs="Health findings and capture-store diagnostics."
						why={['Check capture integrity after an upgrade', 'Find what is bloating the store']}
						openHref="/diagnostics"
					/>
				</DestinationGroup>

				<DestinationGroup
					accent="levers"
					title="Levers"
					purpose="Change the traffic — these act on requests as they pass through."
				>
					<DestinationCard
						group="levers"
						tag="Filters"
						whatItIs="Rewrite rules applied to requests before they reach Anthropic — redact, replace, or re-inject content on the wire."
						why={[
							'Strip a leaked secret from every outgoing request',
							'Dry-run a rule against captured traffic before enabling it'
						]}
						openHref="/filter/workbench"
						howItWorksHref="/help/guides/filter-rules"
					/>
					<DestinationCard
						group="levers"
						tag="Steering"
						whatItIs="Judges that watch live sessions and inject a correction when a session claims done falsely or drifts off task."
						why={[
							'A session said it finished but didn’t → catch it in the act',
							'A session drifted off task → nudge it back onto its actual ask'
						]}
						openHref="/steering"
						howItWorksHref="/help/guides/steering-concepts"
					/>
					<DestinationCard
						group="levers"
						tag="Steerer monitor"
						whatItIs="The fleet view of every steerer — what fired, on which session, with what verdict and confidence."
						why={[
							'See which steerer fired overnight, and why',
							'Tune a judge that fires too often on honest turns'
						]}
						openHref="/steerers"
					/>
					<DestinationCard
						group="levers"
						tag="Fleet"
						whatItIs="Multi-session coordination — the workspace roster, peer messaging, and who is working on what."
						why={[
							'Find which peer session is holding a file you need',
							'Message a running session without touching its terminal'
						]}
						openHref="/fleet"
					/>
				</DestinationGroup>

				<DestinationGroup
					accent="system"
					title="System"
					purpose="Keep it running — memory, configuration, and help."
				>
					<DestinationCard
						group="system"
						tag="Memory"
						whatItIs="Lessons and recalls stored here and re-injected into future sessions."
						why={[
							'See which lessons fired in a session',
							'Record a correction so it sticks next time'
						]}
						openHref="/diagnostics/memory"
					/>
					<DestinationCard
						group="system"
						tag="Settings"
						whatItIs="Runtime feature toggles — the live source of truth for configuration."
						why={['Flip a feature without a restart', 'Check what the boot config resolved to']}
						stat={{ value: 'All', label: 'toggles on' }}
						openHref="/settings"
					/>
					<DestinationCard
						group="system"
						tag="Help"
						whatItIs="Guides and the glossary for everything above — start here if a term is new."
						why={[
							'Read the getting-started guide end to end',
							'Look up a term you just met on another page'
						]}
						stat={{ value: '9', label: 'guides · glossary' }}
						openHref="/help"
					/>
				</DestinationGroup>
			</div>

			<div class="shape-family-proof" aria-hidden="true">
				<div
					class="proof-box"
					data-augmented-ui="tl-clip tr-2-clip-y br-2-clip-x bl-clip both"
					style="--edge: 14px; --aug-tl: var(--edge); --aug-tr1: 8px; --aug-tr2: 18px; --aug-br1: 18px; --aug-br2: 8px; --aug-bl: var(--edge); --aug-border-all: 1.5px; --aug-border-bg: linear-gradient(90deg, var(--data-teal-fg) 0%, var(--data-teal-border) 26%, var(--border-default) 55%); --aug-inlay-all: 3px; --aug-inlay-bg: var(--data-teal-bg);"
				></div>
				<div
					class="proof-box"
					data-augmented-ui="tl-step-inset tr-2-step-inset br-2-step-inset bl-step-inset both"
					style="--edge: 14px; --aug-tl: var(--edge); --aug-tr1: 8px; --aug-tr2: 18px; --aug-br1: 18px; --aug-br2: 8px; --aug-bl: var(--edge); --aug-border-all: 1.5px; --aug-border-bg: linear-gradient(90deg, var(--status-danger-fg) 0%, var(--status-danger-border) 26%, var(--border-default) 55%); --aug-inlay-all: 3px; --aug-inlay-bg: var(--status-danger-bg);"
				></div>
				<div
					class="proof-box"
					data-augmented-ui="tl-rect tr-2-rect-y br-2-rect-x bl-rect both"
					style="--edge: 14px; --aug-tl: var(--edge); --aug-tr1: 8px; --aug-tr2: 18px; --aug-br1: 18px; --aug-br2: 8px; --aug-bl: var(--edge); --aug-border-all: 1.5px; --aug-border-bg: linear-gradient(90deg, var(--status-info-fg) 0%, var(--status-info-border) 26%, var(--border-default) 55%); --aug-inlay-all: 3px; --aug-inlay-bg: var(--status-info-bg);"
				></div>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.map-preview {
		width: 720px;
	}

	.shape-family-proof {
		display: flex;
		gap: 24px;
		margin-top: 24px;
		padding-top: 24px;
		border-top: 1px dashed var(--border-subtle, #333);
	}
	.proof-box {
		width: 120px;
		height: 80px;
		background: var(--surface-bg, #161b22);
	}

	.groups {
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	.groups > :global(.dg:not(:last-child)) {
		--aug-bl: 0px;
		--aug-br1: 0px;
		--aug-br2: 0px;
	}
	.groups > :global(.dg:not(:first-child)) {
		--aug-tl: 0px;
		--aug-tr1: 0px;
		--aug-tr2: 0px;
	}

	.groups :global(.dg-grid) {
		gap: 0;
	}
	.groups :global(.dc) {
		--aug-tl: 0px;
		--aug-br: 0px;
	}
	.groups :global(.dc[data-edge-tl-free]) {
		--aug-tl: var(--edge-tl);
	}
	.groups :global(.dc[data-edge-br-free]) {
		--aug-br: var(--edge-br);
	}

	.groups :global(.dc) {
		transition:
			transform 150ms ease-out,
			box-shadow 150ms ease-out;
	}
	.groups :global(.dc:hover) {
		transform: translateY(-2px);
	}
	.groups :global(.dc:hover) {
		z-index: 1;
		box-shadow: var(--shadow-md);
	}
</style>
