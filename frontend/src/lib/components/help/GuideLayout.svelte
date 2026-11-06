<script lang="ts">
	import { ChevronLeft, ArrowRight } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import GuideContents from './GuideContents.svelte';
	import { revealSections } from '$lib/actions/reveal';
	import { Button } from '$lib/components/ui/button';

	interface TocItem {
		slug: string;
		label: string;
	}

	interface Props {
		title: string;
		summary: string;
		audience: string;
		duration: string;
		toc?: TocItem[];
		children: Snippet;
	}

	function computeTriggers(natural: number[], remaining: number): number[] {
		const clamped = natural.map((v) => Math.max(0, v));
		for (let i = 1; i < clamped.length; i++) {
			if (clamped[i]! <= clamped[i - 1]!) clamped[i] = clamped[i - 1]! + 1;
		}
		if (clamped[clamped.length - 1]! <= remaining) return clamped;
		let lastSafe = -1;
		for (let i = 0; i < clamped.length; i++) {
			if (clamped[i]! > remaining) break;
			lastSafe = i;
		}
		const base = lastSafe >= 0 ? clamped[lastSafe]! : 0;
		const overflowCount = clamped.length - 1 - lastSafe;
		const step = (remaining - base) / overflowCount;
		const result = clamped.filter((_, i) => i <= lastSafe);
		for (let j = 1; j <= overflowCount; j++) result.push(base + step * j);
		return result;
	}

	const { title, summary, audience, duration, toc, children }: Props = $props();
	let article = $state<HTMLElement>();
	let activeSection = $state('');
	let progress = $state(0);
	const ACTIVE_LINE_OFFSET = 76;
	$effect(() => {
		const body = article;
		const items = toc;
		if (!body || !items?.length) return;
		let frame = 0;
		function measure() {
			if (!body || !items) return;
			const doc = document.documentElement;
			const remaining = doc.scrollHeight - window.innerHeight;
			progress =
				remaining > 0 ? Math.min(100, Math.max(0, (window.scrollY / remaining) * 100)) : 100;

			if (remaining <= 0) {
				activeSection = items[0]!.slug;
				return;
			}

			const natural = items.map((item) => {
				const heading = body.querySelector(`#${CSS.escape(item.slug)}`);
				return heading
					? heading.getBoundingClientRect().top + window.scrollY - ACTIVE_LINE_OFFSET
					: 0;
			});
			const triggers = computeTriggers(natural, remaining);

			let current = items[0]!.slug;
			for (let i = 0; i < items.length; i++) {
				if (window.scrollY >= triggers[i]!) current = items[i]!.slug;
			}
			activeSection = current;
		}
		function schedule() {
			cancelAnimationFrame(frame);
			frame = requestAnimationFrame(measure);
		}
		const resize = new ResizeObserver(schedule);
		resize.observe(body);
		window.addEventListener('scroll', schedule, { passive: true });
		window.addEventListener('resize', schedule, { passive: true });
		measure();
		return () => {
			cancelAnimationFrame(frame);
			resize.disconnect();
			window.removeEventListener('scroll', schedule);
			window.removeEventListener('resize', schedule);
		};
	});
</script>

<svelte:head>
	<title>{title} — Help — mike</title>
</svelte:head>

<section class="guide-page" data-slot="guide-page" use:revealSections>
	<header class="page-head" data-reveal>
		<div class="head-inner">
			<a class="back-link" href={resolve('/help/guides')}>
				<ChevronLeft class="size-3" aria-hidden="true" />
				<span>All guides</span>
			</a>
			<h1 class="page-title">{title}</h1>
			<p class="page-sub">{summary}</p>
			<p class="page-sub-meta">
				<span class="meta-pill-backing">
					<span class="meta-pill" data-augmented-ui="tl-clip br-clip border">{audience}</span>
				</span>
				<span class="meta-pill-backing">
					<span class="meta-pill" data-augmented-ui="tl-clip br-clip border">{duration}</span>
				</span>
			</p>
		</div>
	</header>

	<div class="guide-body">
		{#if toc && toc.length > 0}
			<GuideContents items={toc} active={activeSection} {progress} />
		{/if}

		<article
			class="article"
			class:has-toc={Boolean(toc?.length)}
			bind:this={article}
			use:revealSections={{ selector: 'h2, pre, .callout' }}
		>
			{@render children()}
			<nav class="guide-next" aria-label="Guide navigation">
				<div>
					<span class="next-label">Put it into practice</span>
					<p>Explore your captured sessions, or pick your next guide.</p>
				</div>
				<div class="next-actions">
					<Button href={resolve('/help/guides')} variant="outline">All guides</Button>
					<Button href={resolve('/sessions')}
						>Explore sessions <ArrowRight class="size-4" aria-hidden="true" /></Button
					>
				</div>
			</nav>
		</article>
	</div>
</section>

<style>
	.guide-page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		padding: var(--space-5) var(--space-6) var(--space-7);
		flex: 1;
	}
	.page-head {
		border-bottom: 1px solid var(--border-subtle);
		padding-bottom: 18px;
		margin-bottom: 24px;
	}
	.head-inner {
		display: flex;
		flex-direction: column;
		gap: 6px;
		max-width: 760px;
	}
	.back-link {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.75rem;
		color: var(--text-3);
		text-decoration: none;
		margin-bottom: 4px;
	}
	.back-link:hover {
		color: var(--interactive-active-fg);
	}
	.page-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: var(--text-2xl);
		font-weight: var(--weight-display);
		color: var(--text-strong);
		letter-spacing: -0.02em;
	}
	.page-sub {
		margin: 0;
		font-size: 0.9375rem;
		color: var(--text-1);
		line-height: 1.55;
	}
	.page-sub-meta {
		display: flex;
		gap: 8px;
		margin: 2px 0 0;
	}
	.meta-pill-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}

	.meta-pill {
		--aug-tl: 4px;
		--aug-tr: 0;
		--aug-br: 4px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-info-border);
		display: inline-block;
		padding: 2px 9px;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: var(--weight-display);
		color: var(--status-info-fg);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		background:
			linear-gradient(0deg, var(--status-info-bg), var(--status-info-bg)), var(--surface-card);
	}

	.guide-body {
		display: grid;
		grid-template-columns: minmax(12rem, 16rem) minmax(0, 1fr);
		gap: var(--space-6);
		align-items: start;
	}

	.article {
		max-width: 760px;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.article:not(.has-toc) {
		grid-column: 1 / -1;
	}
	.article :global(h2) {
		margin: 14px 0 0;
		font-size: 1.125rem;
		font-weight: 600;
		color: var(--text-strong);
		scroll-margin-top: 16px;
	}
	.article :global(h3) {
		margin: 8px 0 0;
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.article :global(p) {
		margin: 0;
		font-size: 0.875rem;
		color: var(--text-1);
		line-height: 1.6;
	}
	.article :global(ol),
	.article :global(ul) {
		margin: 0;
		padding-left: 22px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.article :global(li) {
		font-size: 0.875rem;
		color: var(--text-1);
		line-height: 1.6;
	}
	.article :global(code) {
		padding: 1px 5px;
		border-radius: var(--r-tol);
		background: var(--surface-2);
		color: var(--text-strong);
		font-size: 0.8125rem;
	}
	.article :global(pre) {
		margin: 0;
		padding: 12px 14px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-tol);
		background: var(--surface-2);
		overflow-x: auto;
		font-size: 0.8125rem;
		line-height: 1.5;
	}
	.article :global(pre code) {
		padding: 0;
		background: transparent;
	}
	.article :global(a) {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.article :global(.callout) {
		--edge: var(--edge-sm);
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-warning-fg) 0%,
			var(--status-warning-border) 45%,
			var(--border-default) 80%
		);
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: calc(var(--edge) + 4px) calc(var(--edge) + 6px);
		background: var(--surface-2);
		font-size: 0.8125rem;
		color: var(--text-1);
		line-height: 1.55;
	}
	.article :global(.callout-title) {
		align-self: flex-start;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: var(--weight-display);
		color: var(--status-warning-fg);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	@media (max-width: 60rem) {
		.guide-page {
			padding: var(--space-4);
		}
		.guide-body {
			grid-template-columns: 1fr;
			gap: 18px;
		}
	}

	.guide-next {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding-block: var(--space-5);
		margin-top: var(--space-5);
		border-top: var(--hairline) solid var(--funk-cyan-border);
	}
	.next-label {
		font: var(--text-sm) var(--font-machine);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--funk-cyan);
	}
	.next-actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
	}
</style>
