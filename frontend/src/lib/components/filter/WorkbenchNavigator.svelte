<script lang="ts">
	import { resolve } from '$app/paths';
	import { ChevronRight } from '@lucide/svelte';
	import { formatCount, formatRelative } from '$lib/time';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import {
		classKey,
		classTitle,
		extractModelReleases,
		splitReleasesForDisplay,
		type ModelFirstReleaseRow,
		type RequestClassIdentity
	} from './promptCatalogue';
	import type { CatalogueClass, RowStatus } from './promptCatalogue.svelte';
	import type { PromptOutlineHeading } from './blockParagraphs';

	interface Props {
		requestId: number;
		sessionId: string;
		model: string | null;
		backHref: `/sessions/${string}/requests/${number}` | '/';
		blockCount: number;
		totalChars: number;
		classIdentity: RequestClassIdentity | null;
		catalogueClasses: CatalogueClass[];
		catalogueStatus: RowStatus;
		requestRelease: string | null;
		onRetryVersions: (key: string) => void;
		outline: PromptOutlineHeading[];
		onJumpToParagraph: (key: string) => void;
		onToggleCollapse: () => void;
	}

	let {
		requestId,
		sessionId,
		model,
		backHref,
		blockCount,
		totalChars,
		classIdentity,
		catalogueClasses,
		catalogueStatus,
		requestRelease,
		onRetryVersions,
		outline,
		onJumpToParagraph,
		onToggleCollapse
	}: Props = $props();

	const title = $derived(
		classIdentity !== null ? classTitle(classIdentity.lane, classIdentity.persona) : null
	);

	const catalogueClass = $derived.by((): CatalogueClass | undefined => {
		if (classIdentity === null) return undefined;
		const key = classKey(classIdentity.lane, classIdentity.persona);
		return catalogueClasses.find((c) => c.key === key);
	});

	const myReleases = $derived.by((): ModelFirstReleaseRow[] => {
		if (catalogueClass === undefined || model === null) return [];
		return extractModelReleases(catalogueClass.versions, model);
	});

	const releaseSplit = $derived(splitReleasesForDisplay(myReleases, 5));

	const selectedIndex = $derived(
		requestRelease === null ? -1 : myReleases.findIndex((r) => r.version === requestRelease)
	);

	interface OtherModelRow {
		model: string;
		requestId: string;
	}

	const otherModels = $derived.by((): OtherModelRow[] => {
		if (catalogueClass === undefined) return [];
		const seen = new Set<string>();
		for (const v of catalogueClass.versions) {
			for (const m of v.models) {
				if (m.model !== null && m.model !== model) seen.add(m.model);
			}
		}
		const rows: OtherModelRow[] = [];
		for (const otherModel of seen) {
			const current = extractModelReleases(catalogueClass.versions, otherModel)[0];
			if (current !== undefined)
				rows.push({ model: otherModel, requestId: current.latestRequestId });
		}
		return rows.sort((a, b) => a.model.localeCompare(b.model));
	});

	function releaseTitle(release: ModelFirstReleaseRow): string {
		return release.version === null ? 'no version header' : `Claude Code ${release.version}`;
	}

	let expanded = $state(false);
</script>

{#snippet versionRow(release: ModelFirstReleaseRow, selected: boolean)}
	<a
		class="wbn-version-row"
		class:wbn-version-row--selected={selected}
		href={resolve(`/filter/workbench?from=request:${release.latestRequestId}`)}
		aria-current={selected ? 'true' : undefined}
	>
		<span class="wbn-version-row__badge" class:wbn-version-row__badge--current={release.isCurrent}>
			{release.isCurrent ? 'current' : 'older'}
		</span>
		<span class="wbn-version-row__version">{releaseTitle(release)}</span>
		<span class="wbn-version-row__meta">
			{formatCount(release.observationCount)} obs · {formatRelative(release.lastSeen)}
		</span>
	</a>
{/snippet}

<section
	class="wb-pane wbn-pane"
	data-augmented-ui="tl-clip bl-clip border"
	aria-label="Prompt navigator"
	data-slot="workbench-navigator"
>
	<header class="wb-pane-head">
		<!-- A real heading (not just a styled span) so this column's title anchors a valid
		     heading order (h1 "Filter workbench" -> h2 here) whether or not the other two
		     columns are mounted — each column collapses independently, so none can assume
		     another supplied the heading level its own content needs (axe heading-order,
		     caught when the outline itself is the one collapsed and "Your edits"' h3 group
		     headings had nothing above them). `display: contents` keeps it out of the
		     layout box model entirely. -->
		<h2 class="wb-pane-head-heading">
			<button
				type="button"
				class="wb-pane-head-toggle"
				aria-expanded="true"
				aria-label="Collapse outline"
				onclick={onToggleCollapse}
			>
				<span class="wb-pane-label">Outline</span>
			</button>
		</h2>
	</header>
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -- keyboard-focusable scroll region (axe scrollable-region-focusable); carries role=region + aria-label -->
	<div
		class="wb-pane-scroll wbn-scroll"
		role="region"
		aria-label="Model, prompt, and version navigation"
		tabindex="0"
	>
		{#if outline.length > 0}
			<section class="wbn-section" aria-label="In this prompt">
				<h2 class="wbn-section__title">In this prompt</h2>
				<nav class="wbn-outline" aria-label="Prompt headings">
					{#each outline as heading (heading.key)}
						<button
							type="button"
							class="wbn-outline-item"
							style={`padding-left: ${8 + (heading.level - 1) * 10}px`}
							onclick={() => onJumpToParagraph(heading.key)}
							aria-label={heading.hasEdits ? `${heading.title} — has edits` : heading.title}
						>
							<span class="wbn-outline-item__title">{heading.title}</span>
							{#if heading.hasEdits}
								<span class="wbn-outline-item__marker" title="You have edits here" aria-hidden="true"
								></span>
							{/if}
						</button>
					{/each}
				</nav>
			</section>
		{/if}

		<section class="wbn-section" aria-label="Model">
			<h2 class="wbn-section__title">Model</h2>
			<p class="wbn-section__value">{model ?? 'unknown model'}</p>
		</section>

		{#if classIdentity === null}
			<p class="wbn-note">
				not catalogued — this request's turn origin isn't part of the prompt catalogue
			</p>
		{:else}
			<section class="wbn-section" aria-label="Prompt">
				<h2 class="wbn-section__title">Prompt</h2>
				<p class="wbn-section__value">
					{title}
					<span class="wbn-lane-badge" data-lane={classIdentity.lane}>{classIdentity.lane}</span>
				</p>
			</section>

			<section class="wbn-section" aria-label="Versions">
				<h2 class="wbn-section__title">Versions</h2>
				{#if catalogueClass === undefined}
					{#if catalogueStatus === 'loading'}
						<p class="wbn-note">loading the prompt catalogue…</p>
					{:else}
						<p class="wbn-note">not catalogued — no matching prompt class found</p>
					{/if}
				{:else}
					{@const cls = catalogueClass}
					{#if cls.versionsStatus === 'loading'}
						<p class="wbn-note">checking prompt versions…</p>
					{:else if cls.versionsStatus === 'error'}
						<div class="wbn-note wbn-note--error">
							<span>
								couldn't load release history{cls.versionsError !== null
									? `: ${cls.versionsError}`
									: ''}
							</span>
							<button type="button" class="wbn-retry-btn" onclick={() => onRetryVersions(cls.key)}>
								Retry
							</button>
						</div>
					{:else if myReleases.length === 0}
						<p class="wbn-note">no releases captured for this model</p>
					{:else}
						{#each releaseSplit.visible as release, i (release.version ?? '(no version)')}
							{@render versionRow(release, i === selectedIndex)}
						{/each}
						{#if releaseSplit.collapsed.length > 0}
							<button
								type="button"
								class="wbn-disclosure"
								aria-expanded={expanded}
								onclick={() => (expanded = !expanded)}
							>
								<DisclosureMarker open={expanded} size={9} />
								{expanded
									? 'Hide older versions'
									: `${releaseSplit.collapsed.length} older version${releaseSplit.collapsed.length === 1 ? '' : 's'}`}
							</button>
							{#if expanded}
								{#each releaseSplit.collapsed as release, i (release.version ?? '(no version)')}
									{@render versionRow(release, i + releaseSplit.visible.length === selectedIndex)}
								{/each}
							{/if}
						{/if}
						{#if selectedIndex === -1}
							<p class="wbn-note">
								{requestRelease !== null
									? `this request carries release ${requestRelease}, outside the captured history above`
									: 'this request carries no version header'}
							</p>
						{/if}
					{/if}
				{/if}
			</section>

			<section class="wbn-section" aria-label="Other models">
				<h2 class="wbn-section__title">Other models</h2>
				{#if otherModels.length === 0}
					<p class="wbn-note">no other observed model for this prompt</p>
				{:else}
					{#each otherModels as row (row.model)}
						<a
							class="wbn-other-model"
							href={resolve(`/filter/workbench?from=request:${row.requestId}`)}
							aria-label={`Open ${row.model}'s current release`}
						>
							{row.model}
							<ChevronRight size={11} aria-hidden="true" />
						</a>
					{/each}
				{/if}
			</section>
		{/if}

		<section class="wbn-section" aria-label="Request">
			<h2 class="wbn-section__title">Request</h2>
			<p class="wbn-section__value">
				#{requestId} · session {sessionId} ·
				{blockCount} block{blockCount === 1 ? '' : 's'} ·
				{formatCount(totalChars)} chars ·
				<a
					href={backHref === '/'
						? resolve('/')
						: resolve(backHref.slice(1) as `sessions/${string}/requests/${number}`)}
					class="wbn-request-link">open request</a
				>
			</p>
		</section>
	</div>
</section>

<style>
	.wb-pane {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.wb-pane-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 8px;
		min-height: 36px;
		padding: 6px 12px;
		border-bottom: 1px solid var(--border-subtle);
		flex-shrink: 0;
		background: var(--surface-card);
	}

	.wb-pane-label {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-3);
	}

	.wb-pane-head-heading {
		display: contents;
	}

	.wb-pane-head-toggle {
		display: flex;
		align-items: center;
		background: transparent;
		border: none;
		padding: 0;
		margin: 0;
		cursor: pointer;
		text-align: left;
	}
	.wb-pane-head-toggle:hover .wb-pane-label {
		color: var(--text-strong);
	}
	.wb-pane-head-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-pane-scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.wbn-section {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.wbn-section__title {
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.07em;
		color: var(--text-3);
	}

	.wbn-section__value {
		margin: 0;
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		font: 500 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		overflow-wrap: break-word;
	}

	.wbn-outline {
		display: flex;
		flex-direction: column;
	}

	.wbn-outline-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		min-height: 26px;
		padding-right: 4px;
		background: transparent;
		border: none;
		color: var(--text-2);
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		text-align: left;
		cursor: pointer;
		transition: color 120ms ease;
	}
	.wbn-outline-item:hover {
		color: var(--text-strong);
	}
	.wbn-outline-item:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 3px;
	}

	.wbn-outline-item__title {
		flex: 1;
		min-width: 0;
		overflow-wrap: break-word;
	}

	.wbn-outline-item__marker {
		flex-shrink: 0;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--funk-green);
	}

	.wbn-lane-badge {
		display: inline-flex;
		align-items: center;
		flex-shrink: 0;
		padding: 1px 7px;
		font-size: 10px;
		letter-spacing: 0.03em;
		border-radius: 3px;
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		color: var(--text-2);
	}

	.wbn-note {
		margin: 0;
		font: italic 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}

	.wbn-note--error {
		color: var(--status-danger-fg);
		font-style: normal;
	}

	.wbn-retry-btn {
		padding: 3px 10px;
		font-size: 10px;
		color: var(--text-2);
		background: transparent;
		border: 1px dashed var(--border-default);
		border-radius: 4px;
		cursor: pointer;
		transition:
			color 120ms ease,
			border-color 120ms ease;
	}
	.wbn-retry-btn:hover {
		color: var(--interactive-active-fg);
		border-color: var(--interactive-active-border);
	}

	.wbn-version-row {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 4px 6px;
		border-left: 2px solid var(--border-subtle);
		text-decoration: none;
		transition:
			color 120ms ease,
			background 120ms ease,
			border-color 120ms ease;
	}
	.wbn-version-row:hover {
		background: var(--surface-2);
	}

	.wbn-version-row__badge {
		display: inline-flex;
		align-items: center;
		align-self: flex-start;
		padding: 1px 6px;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-radius: 3px;
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		color: var(--text-3);
	}
	.wbn-version-row__badge--current {
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg);
		border-color: var(--interactive-active-border);
	}

	.wbn-version-row__version {
		font: 500 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-1);
	}

	.wbn-version-row__meta {
		font-size: 10px;
		color: var(--text-3);
	}

	.wbn-version-row--selected {
		border-left-color: var(--funk-cyan-border);
		background: linear-gradient(0deg, var(--funk-cyan-dim), var(--funk-cyan-dim)), var(--surface-2);
	}
	.wbn-version-row--selected .wbn-version-row__version {
		color: var(--funk-cyan);
		text-shadow: var(--glow-cyan);
	}

	.wbn-disclosure {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		align-self: flex-start;
		padding: 2px 4px;
		font-size: 10px;
		color: var(--text-3);
		background: transparent;
		border: none;
		cursor: pointer;
		transition: color 120ms ease;
	}
	.wbn-disclosure:hover {
		color: var(--text-2);
	}
	.wbn-disclosure:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wbn-other-model {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		padding: 3px 6px;
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		text-decoration: none;
		border-radius: 4px;
		transition:
			color 120ms ease,
			background 120ms ease;
	}
	.wbn-other-model:hover {
		color: var(--text-strong);
		background: var(--surface-2);
	}

	.wbn-request-link {
		color: var(--text-2);
		text-decoration: none;
		transition: color 120ms ease;
	}
	.wbn-request-link:hover {
		color: var(--interactive-active-fg);
	}
</style>
