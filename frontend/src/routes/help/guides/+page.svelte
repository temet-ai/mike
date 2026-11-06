<script lang="ts">
	import { ChevronRight } from '@lucide/svelte';
	import { GUIDES } from '$lib/help/guides';
	import { resolve } from '$app/paths';

	function guideHref(slug: string): string {
		switch (slug) {
			case 'getting-started':
				return resolve('/help/guides/getting-started');
			case 'inspecting-requests':
				return resolve('/help/guides/inspecting-requests');
			case 'filter-rules':
				return resolve('/help/guides/filter-rules');
			case 'anomalies':
				return resolve('/help/guides/anomalies');
			case 'retention':
				return resolve('/help/guides/retention');
			case 'steering-concepts':
				return resolve('/help/guides/steering-concepts');
			default:
				return resolve('/help/guides');
		}
	}
</script>

<svelte:head>
	<title>How-to guides — mike</title>
</svelte:head>

<section class="guides-page" data-slot="guides-index">
	<header class="page-head">
		<div class="head-inner">
			<h1 class="page-title">How-to guides</h1>
			<p class="page-sub">
				Task-oriented walkthroughs for mike's major features. Each guide assumes mike is already
				running; if you haven't set it up yet, start with <a
					class="inline-link"
					href={resolve('/help/guides/getting-started')}>Get Claude Code talking to mike</a
				>.
			</p>
			<p class="page-sub-meta">{GUIDES.length} guides</p>
		</div>
	</header>

	<ul class="guide-list">
		{#each GUIDES as guide (guide.slug)}
			<li>
				<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- guideHref() resolves each known literal internally; the rule can't see through the function wrapper -->
				<a
					class="guide-row"
					href={guideHref(guide.slug)}
					data-augmented-ui="tl-clip br-clip border"
				>
					<div class="row-text">
						<span class="row-title">{guide.title}</span>
						<span class="row-desc">{guide.description}</span>
						<span class="row-meta">
							<span class="meta-pill-backing">
								<span class="meta-pill" data-augmented-ui="tl-clip br-clip border"
									>{guide.audience}</span
								>
							</span>
							<span class="meta-pill-backing">
								<span class="meta-pill" data-augmented-ui="tl-clip br-clip border"
									>{guide.duration}</span
								>
							</span>
						</span>
					</div>
					<ChevronRight class="size-4 shrink-0 text-[var(--text-3)]" aria-hidden="true" />
				</a>
			</li>
		{/each}
	</ul>
</section>

<style>
	.guides-page {
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow-y: auto;
		padding: var(--space-5) var(--space-6) var(--space-7);
	}
	.page-head {
		border-bottom: 1px solid var(--border-subtle);
		padding-bottom: 18px;
		margin-bottom: var(--space-5);
	}
	.head-inner {
		display: flex;
		flex-direction: column;
		gap: 6px;
		max-width: 760px;
	}
	.page-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1.5rem;
		font-weight: 700;
		color: var(--text-strong);
		letter-spacing: -0.02em;
	}
	.page-sub {
		margin: 0;
		font-size: 0.875rem;
		color: var(--text-1);
		line-height: 1.55;
	}
	.page-sub-meta {
		margin: 0;
		font-size: 0.75rem;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.inline-link {
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.guide-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		max-width: 880px;
	}
	.guide-row {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 45%,
			var(--border-default) 80%
		);
		display: flex;
		align-items: center;
		gap: 14px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 8px);
		background: var(--surface-card);
		text-decoration: none;
	}
	.guide-row {
		transition:
			transform 150ms ease-out,
			box-shadow 150ms ease-out;
	}
	.guide-row:hover {
		transform: translateY(-2px);
	}
	.guide-row:hover {
		box-shadow: var(--shadow-md);
		--aug-border-bg: linear-gradient(135deg, var(--status-info-fg), var(--border-default) 45%);
	}
	.guide-row:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.row-text {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		flex: 1 1 auto;
		min-width: 0;
	}
	.row-title {
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.guide-row:hover .row-title {
		color: var(--interactive-active-fg);
	}
	.row-desc {
		font-size: 0.8125rem;
		color: var(--text-1);
		line-height: 1.5;
	}
	.row-meta {
		display: flex;
		gap: 6px;
		margin-top: 2px;
	}
	.meta-pill-backing {
		display: inline-flex;
		background: var(--surface-card);
	}

	.meta-pill {
		--aug-tl: 4px;
		--aug-tr: 0;
		--aug-br: 4px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-info-border);
		display: inline-block;
		padding: 1px var(--space-2);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		color: var(--status-info-fg);
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		background:
			linear-gradient(0deg, var(--status-info-bg), var(--status-info-bg)), var(--surface-card);
	}
</style>
