<script lang="ts">
	import { BookOpen, Compass, ExternalLink, GraduationCap } from '@lucide/svelte';
	import { GUIDES } from '$lib/help/guides';
	import { EXTERNAL_DOCS } from '$lib/help/externalDocs';
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
	<title>Help — mike</title>
</svelte:head>

<section class="help-page" data-slot="help-index">
	<header class="page-head">
		<div class="head-inner">
			<h1 class="page-title">Help</h1>
			<p class="page-sub">
				How-to guides for the capture pipeline's major features, the canonical glossary of terms,
				and installed setup, privacy, and recovery documentation.
			</p>
			<p class="page-sub-meta">
				{GUIDES.length} guides · vocabulary reference · installed docs
			</p>
		</div>
	</header>

	<div class="help-body">
		<section class="block" aria-labelledby="guides-heading">
			<header class="block-head">
				<GraduationCap class="size-4 text-[var(--text-3)]" aria-hidden="true" />
				<h2 id="guides-heading" class="block-title">How-to guides</h2>
				<p class="block-blurb">
					Task-oriented walkthroughs. Pick the one that matches what you're trying to do.
				</p>
			</header>
			<ul class="card-list">
				{#each GUIDES as guide (guide.slug)}
					<li>
						<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- guideHref() resolves each known literal internally; the rule can't see through the function wrapper -->
						<a class="card" href={guideHref(guide.slug)} data-augmented-ui="tl-clip br-clip border">
							<span class="card-title">{guide.title}</span>
							<span class="card-desc">{guide.description}</span>
							<span class="card-meta">
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
						</a>
					</li>
				{/each}
			</ul>
		</section>

		<section class="block" aria-labelledby="glossary-heading">
			<header class="block-head">
				<Compass class="size-4 text-[var(--text-3)]" aria-hidden="true" />
				<h2 id="glossary-heading" class="block-title">Glossary</h2>
				<p class="block-blurb">
					Definitions for every domain term used in the SPA. Every HelpHint popover deep-links here.
				</p>
			</header>
			<ul class="card-list">
				<li>
					<a
						class="card"
						href={resolve('/help/glossary')}
						data-augmented-ui="tl-clip br-clip border"
					>
						<span class="card-title">Open the glossary</span>
						<span class="card-desc">
							Vocabulary reference grouped by domain (capture, lifecycle, compositions, analysis,
							filtering, compose, retention, tokens, content, protocol, agents).
						</span>
					</a>
				</li>
			</ul>
		</section>

		<section class="block" aria-labelledby="external-heading">
			<header class="block-head">
				<BookOpen class="size-4 text-[var(--text-3)]" aria-hidden="true" />
				<h2 id="external-heading" class="block-title">Installed documentation & references</h2>
				<p class="block-blurb">
					User guides are bundled locally. Upstream references need an internet connection.
				</p>
			</header>
			<ul class="card-list">
				{#each EXTERNAL_DOCS as doc (doc.href)}
					<li>
						<!-- eslint-disable svelte/no-navigation-without-resolve -- Shared doc entries are either resolved local routes or external upstream references -->
						<a
							class="card"
							href={doc.internal
								? resolve('/help/docs/[slug]', { slug: doc.href.split('/').at(-1) ?? '' })
								: doc.href}
							target={doc.internal ? undefined : '_blank'}
							rel={doc.internal ? undefined : 'noopener noreferrer'}
							data-augmented-ui="tl-clip br-clip border"
						>
							<span class="card-title">
								{doc.label}
								{#if !doc.internal}<ExternalLink class="size-3" aria-hidden="true" />{/if}
							</span>
							<span class="card-desc">{doc.description}</span>
						</a>
						<!-- eslint-enable svelte/no-navigation-without-resolve -->
					</li>
				{/each}
			</ul>
		</section>
	</div>
</section>

<style>
	.help-page {
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
		line-height: 1.5;
	}
	.page-sub-meta {
		margin: 0;
		font-size: 0.75rem;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.help-body {
		display: flex;
		flex-direction: column;
		gap: 36px;
		max-width: 880px;
	}

	.block {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.block-head {
		display: grid;
		grid-template-columns: auto 1fr;
		grid-template-rows: auto auto;
		column-gap: var(--space-2);
		row-gap: var(--space-1);
		align-items: center;
		padding-bottom: var(--space-2);
		border-bottom: 1px solid var(--border-default);
	}
	.block-head :global(svg) {
		grid-row: span 2;
	}
	.block-title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--text-strong);
		letter-spacing: -0.01em;
	}
	.block-blurb {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--text-2);
		line-height: 1.45;
	}

	.card-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: var(--space-3);
	}
	.card {
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
		flex-direction: column;
		gap: 6px;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 8px);
		background: var(--surface-card);
		text-decoration: none;
	}
	.card {
		transition:
			transform 150ms ease-out,
			box-shadow 150ms ease-out;
	}
	.card:hover {
		transform: translateY(-2px);
	}
	.card:hover {
		box-shadow: var(--shadow-md);
		--aug-border-bg: linear-gradient(135deg, var(--status-info-fg), var(--border-default) 45%);
	}
	.card:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.card-title {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 0.9375rem;
		font-weight: 600;
		color: var(--text-strong);
	}
	.card:hover .card-title {
		color: var(--interactive-active-fg);
	}
	.card-desc {
		font-size: 0.8125rem;
		color: var(--text-1);
		line-height: 1.5;
	}
	.card-meta {
		display: flex;
		gap: 6px;
		margin-top: var(--space-1);
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
