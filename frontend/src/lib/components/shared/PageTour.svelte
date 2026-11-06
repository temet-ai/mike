<script lang="ts" module>
	import type { Placement } from '$lib/tour/tourGeometry';

	export interface TourStep {
		/** CSS selector for the element this step points at. Omit for a centred, unanchored step. */
		target?: string;
		title: string;
		body: string;
		placement?: Placement;
	}
</script>

<script lang="ts">
	import { Portal } from 'bits-ui';
	import { tick } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		centeredPosition,
		placePopover,
		sameRect,
		spotlightRect,
		type PopoverPosition,
		type Rect
	} from '$lib/tour/tourGeometry';

	interface Props {
		steps: TourStep[];
		open?: boolean;
		label?: string;
		onClose?: (completed: boolean) => void;
	}

	let { steps, open = $bindable(false), label = 'Page tour', onClose }: Props = $props();

	const SPOT_PADDING = 8;
	const uid = $props.id();
	const titleId = `${uid}-title`;
	const bodyId = `${uid}-body`;
	const maskId = `${uid}-mask`;

	let active = $state<TourStep[]>([]);
	let index = $state(0);
	let targetRect = $state<Rect | null>(null);
	let popoverEl = $state<HTMLElement | null>(null);
	let popWidth = $state(0);
	let popHeight = $state(0);
	let vpWidth = $state(0);
	let vpHeight = $state(0);
	let returnFocus: HTMLElement | null = null;

	const current = $derived(active[index]);
	const isFirst = $derived(index === 0);
	const isLast = $derived(index === active.length - 1);
	const spot = $derived(targetRect ? spotlightRect(targetRect, SPOT_PADDING) : null);
	const position = $derived.by<PopoverPosition>(() => {
		const pop = { width: popWidth, height: popHeight };
		const vp = { width: vpWidth, height: vpHeight };
		return spot ? placePopover(spot, pop, vp, current?.placement) : centeredPosition(pop, vp);
	});

	function prefersReducedMotion(): boolean {
		return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
	}

	function findTarget(step: TourStep | undefined): HTMLElement | null {
		if (!step?.target) return null;
		return document.querySelector<HTMLElement>(step.target);
	}

	let wasOpen = false;
	$effect(() => {
		if (open && !wasOpen) {
			returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
			active = steps.filter((s) => !s.target || findTarget(s) !== null);
			index = 0;
			if (active.length === 0) finish(false);
		}
		if (!open && wasOpen) {
			targetRect = null;
			returnFocus?.focus();
			returnFocus = null;
		}
		wasOpen = open;
	});

	$effect(() => {
		if (!open) return;
		const el = findTarget(current);
		targetRect = null;
		if (!el) return;
		const tall = el.getBoundingClientRect().height > window.innerHeight * 0.7;
		el.scrollIntoView({
			block: tall ? 'start' : 'center',
			behavior: prefersReducedMotion() ? 'auto' : 'smooth'
		});
		let frame = 0;
		let last: Rect | null = null;
		const track = () => {
			const r = el.getBoundingClientRect();
			const next = { top: r.top, left: r.left, width: r.width, height: r.height };
			if (!sameRect(last, next)) {
				last = next;
				targetRect = next;
			}
			frame = requestAnimationFrame(track);
		};
		track();
		return () => cancelAnimationFrame(frame);
	});

	$effect(() => {
		if (!open || !current || !popoverEl) return;
		const el = popoverEl;
		void tick().then(() => el.focus({ preventScroll: true }));
	});

	function finish(completed: boolean): void {
		open = false;
		onClose?.(completed);
	}
	function next(): void {
		if (isLast) finish(true);
		else index += 1;
	}
	function back(): void {
		if (!isFirst) index -= 1;
	}
	function goTo(i: number): void {
		index = i;
	}

	function trapTab(e: KeyboardEvent): void {
		if (!popoverEl) return;
		const focusable = Array.from(
			popoverEl.querySelectorAll<HTMLElement>('button:not([disabled]), [href], [tabindex="0"]')
		);
		const first = focusable[0];
		const last = focusable[focusable.length - 1];
		if (!first || !last) return;
		const at = document.activeElement;
		if (e.shiftKey && (at === first || at === popoverEl)) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && at === last) {
			e.preventDefault();
			first.focus();
		}
	}

	function handleKeydown(e: KeyboardEvent): void {
		if (!open) return;
		if (e.key === 'Escape') {
			e.preventDefault();
			finish(false);
		} else if (e.key === 'ArrowRight') {
			e.preventDefault();
			next();
		} else if (e.key === 'ArrowLeft') {
			e.preventDefault();
			back();
		} else if (e.key === 'Tab') {
			trapTab(e);
		}
	}
</script>

<svelte:window bind:innerWidth={vpWidth} bind:innerHeight={vpHeight} onkeydown={handleKeydown} />

{#if open && current}
	<Portal>
		<div class="tour-root" data-slot="page-tour">
			<svg
				class="tour-overlay"
				width={vpWidth}
				height={vpHeight}
				aria-hidden="true"
				data-slot="tour-overlay"
			>
				<defs>
					<mask id={maskId}>
						<rect x="0" y="0" width={vpWidth} height={vpHeight} fill="white" />
						{#if spot}
							<rect
								class="tour-cutout"
								x={spot.left}
								y={spot.top}
								width={spot.width}
								height={spot.height}
								rx="4"
								fill="black"
							/>
						{/if}
					</mask>
				</defs>
				<rect
					class="tour-dim"
					x="0"
					y="0"
					width={vpWidth}
					height={vpHeight}
					mask="url(#{maskId})"
				/>
			</svg>

			{#if spot}
				<div
					class="tour-ring"
					data-slot="tour-spotlight"
					style:top="{spot.top}px"
					style:left="{spot.left}px"
					style:width="{spot.width}px"
					style:height="{spot.height}px"
				></div>
			{/if}

			<div
				bind:this={popoverEl}
				bind:offsetWidth={popWidth}
				bind:offsetHeight={popHeight}
				class="tour-popover"
				role="dialog"
				aria-modal="true"
				aria-roledescription="tour step"
				aria-labelledby={titleId}
				aria-describedby={bodyId}
				tabindex="-1"
				data-slot="tour-popover"
				data-side={position.side}
				data-augmented-ui="tl-clip br-clip border"
				style:top="{position.top}px"
				style:left="{position.left}px"
			>
				<div class="tour-meta">
					<span class="tour-kicker">{label}</span>
					<span class="tour-progress" data-slot="tour-progress" aria-live="polite">
						Step {index + 1} of {active.length}
					</span>
				</div>
				<h2 class="tour-title" id={titleId}>{current.title}</h2>
				<p class="tour-body" id={bodyId}>{current.body}</p>
				<div class="tour-footer">
					<div class="tour-dots" aria-hidden="true">
						{#each active as step, i (step.title)}
							<button
								type="button"
								tabindex="-1"
								class="tour-dot"
								class:tour-dot--on={i === index}
								onclick={() => goTo(i)}
								aria-label="Go to step {i + 1}"
							></button>
						{/each}
					</div>
					<div class="tour-actions">
						{#if !isLast}
							<Button variant="ghost" size="sm" onclick={() => finish(false)}>Skip tour</Button>
						{/if}
						<Button variant="outline" size="sm" onclick={back} disabled={isFirst}>Back</Button>
						<Button size="sm" onclick={next}>{isLast ? 'Done' : 'Next'}</Button>
					</div>
				</div>
			</div>

			{#if position.arrow}
				<div
					class="tour-arrow"
					data-side={position.side}
					aria-hidden="true"
					style:top="{position.arrow.y}px"
					style:left="{position.arrow.x}px"
				></div>
			{/if}
		</div>
	</Portal>
{/if}

<style>
	.tour-root {
		position: fixed;
		inset: 0;
		z-index: 60;
	}
	.tour-overlay {
		position: absolute;
		inset: 0;
	}
	.tour-dim {
		fill: var(--background);
		fill-opacity: 0.8;
	}
	.tour-ring {
		position: fixed;
		pointer-events: none;
		border: 2px solid var(--focus);
		border-radius: 4px;
		box-shadow: 0 0 0 4px color-mix(in oklab, var(--focus) 25%, transparent);
		transition:
			top var(--dur-2) var(--ease-snap),
			left var(--dur-2) var(--ease-snap),
			width var(--dur-2) var(--ease-snap),
			height var(--dur-2) var(--ease-snap);
	}
	.tour-popover {
		position: fixed;
		width: min(360px, calc(100vw - 24px));
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-4);
		background: var(--card);
		color: var(--card-foreground);
		--aug-border-bg: var(--focus);
		--aug-tl: 10px;
		--aug-br: 10px;
		outline: none;
		transition:
			top var(--dur-2) var(--ease-snap),
			left var(--dur-2) var(--ease-snap);
	}
	.tour-meta {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-2);
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: var(--text-xs);
		letter-spacing: 0.04em;
		color: var(--text-3);
	}
	.tour-kicker {
		text-transform: uppercase;
		color: var(--focus);
	}
	.tour-title {
		margin: 0;
		font: var(--weight-display) var(--text-lg) / var(--leading-tight) var(--font-display);
		letter-spacing: var(--track-display);
		color: var(--text-strong);
	}
	.tour-body {
		margin: 0;
		font-size: var(--text-md);
		line-height: var(--leading-body);
		color: var(--text-2);
	}
	.tour-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		margin-top: var(--space-2);
	}
	.tour-dots {
		display: inline-flex;
		gap: 6px;
	}
	.tour-dot {
		width: 6px;
		height: 6px;
		padding: 0;
		border: 0;
		border-radius: 999px;
		background: var(--border-strong);
		cursor: pointer;
	}
	.tour-dot--on {
		width: 16px;
		background: var(--focus);
	}
	.tour-actions {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}
	.tour-arrow {
		position: fixed;
		width: 12px;
		height: 12px;
		pointer-events: none;
		background: var(--focus);
		transform: translate(-50%, -50%) rotate(45deg);
		transition:
			top var(--dur-2) var(--ease-snap),
			left var(--dur-2) var(--ease-snap);
	}
	@media (prefers-reduced-motion: reduce) {
		.tour-ring,
		.tour-popover,
		.tour-arrow {
			transition: none;
		}
	}
</style>
