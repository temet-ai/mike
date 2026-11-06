<script lang="ts">
	import { getContext } from 'svelte';
	import { resolve } from '$app/paths';
	import { ArrowRight } from '@lucide/svelte';
	import { formatCount, formatRelative } from '$lib/time';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import type { FilterRuleDto, ObservedSystemBlock } from '$lib/types';
	import type { CatalogueClass, RowStatus } from './promptCatalogue.svelte';
	import {
		buildModelFirstTree,
		isNeverObserved,
		splitReleasesForDisplay,
		type ModelFirstClassGroup,
		type ModelFirstReleaseRow,
		type ModelFirstSourceClass
	} from './promptCatalogue';
	import { countRuleMatches } from './workbench';
	import { deriveRuleTitle } from './rule-matching';

	interface Props {
		orderedRules: FilterRuleDto[];
		classes: CatalogueClass[];
		status: RowStatus;
		error: string | null;
		onRetryCatalogue: () => void;
		onRetryModel: (key: string, modelIndex: number) => void;
		onRetryVersions: (key: string) => void;
	}

	let {
		orderedRules,
		classes,
		status,
		error,
		onRetryCatalogue,
		onRetryModel,
		onRetryVersions
	}: Props = $props();

	const sourceClasses = $derived<ModelFirstSourceClass[]>(
		classes.map((cls) => ({
			key: cls.key,
			lane: cls.lane,
			title: cls.title,
			registrationLabel: cls.registrationLabel,
			noRoute: cls.noRoute,
			neverObserved: isNeverObserved({ first_seen: cls.firstSeen }),
			versionsStatus: cls.versionsStatus,
			versionsError: cls.versionsError,
			versions: cls.versions,
			modelRows: cls.models.map((m) => ({
				model: m.model,
				windowedCount: m.windowedCount,
				lastSeen: m.latestObservation?.createdAt ?? null,
				sampleBlocks: m.sampleBlocks,
				status: m.status,
				error: m.error
			}))
		}))
	);

	const tree = $derived(buildModelFirstTree(sourceClasses));

	const totalEnabledRules = $derived(orderedRules.filter((r) => r.enabled).length);

	interface MatchedRuleChip {
		rule: FilterRuleDto;
		count: number;
	}

	function matchedRules(blocks: readonly ObservedSystemBlock[] | null): MatchedRuleChip[] {
		if (blocks === null) return [];
		const blockTexts = blocks.map((b) => b.text);
		if (blockTexts.length === 0) return [];
		return orderedRules
			.filter((r) => r.enabled)
			.map((rule) => ({ rule, ...countRuleMatches(rule, blockTexts) }))
			.filter((m) => m.count > 0);
	}

	function releaseTitle(release: ModelFirstReleaseRow): string {
		return release.version === null ? 'no version header' : `Claude Code ${release.version}`;
	}

	function releaseRowKey(release: ModelFirstReleaseRow): string {
		return `${release.version ?? '(no version)'}:${release.latestRequestId}`;
	}

	function findModelIndex(classKeyValue: string, model: string | null): number {
		const cls = classes.find((c) => c.key === classKeyValue);
		if (cls === undefined) return -1;
		return cls.models.findIndex((m) => m.model === model);
	}

	let expandedGroups = $state<Set<string>>(new Set());

	function groupKey(model: string, classKeyValue: string): string {
		return `${model}:${classKeyValue}`;
	}

	function toggleGroup(key: string): void {
		const next = new Set(expandedGroups);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		expandedGroups = next;
	}

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);

	let localCollapsedKeys = $state<Set<string>>(new Set());

	function modelCollapseKey(model: string): string {
		return `filter-workbench:model:${model}`;
	}

	const NEVER_OBSERVED_COLLAPSE_KEY = 'filter-workbench:model:never-observed';

	function isModelCollapsed(key: string): boolean {
		if (collapseStore) return !collapseStore.isExpanded(key, true);
		return localCollapsedKeys.has(key);
	}

	function toggleModelCollapse(key: string): void {
		if (collapseStore) {
			collapseStore.toggle(key);
			return;
		}
		const next = new Set(localCollapsedKeys);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		localCollapsedKeys = next;
	}
</script>

{#snippet releaseRow(
	release: ModelFirstReleaseRow,
	classGroup: ModelFirstClassGroup,
	model: string
)}
	{@const matches = release.isCurrent ? matchedRules(classGroup.currentSampleBlocks) : []}
	<div class="wbl-row" data-slot="workbench-landing-row" class:wbl-row--current={release.isCurrent}>
		<div class="wbl-row__identity">
			<span class="wbl-release-badge" class:wbl-release-badge--current={release.isCurrent}>
				{release.isCurrent ? 'current' : 'older'}
			</span>
			<a
				class="wbl-release-link"
				href={resolve(`/filter/workbench?from=request:${release.latestRequestId}`)}
				aria-label={`Open ${classGroup.title} (${model}, ${releaseTitle(release)}) in the workbench`}
			>
				<span class="wbl-release-version">{releaseTitle(release)}</span>
				<span class="wbl-row__count">
					{formatCount(release.observationCount)} obs · first {formatRelative(release.firstSeen)} · last
					{formatRelative(release.lastSeen)}
				</span>
				<span class="wbl-release-arrow"><ArrowRight size={11} aria-hidden="true" /></span>
			</a>
		</div>

		{#if release.isCurrent}
			<div class="wbl-rules">
				{#if classGroup.currentSampleStatus === 'loading'}
					<p class="wbl-rules__note">checking prompt…</p>
				{:else if classGroup.currentSampleStatus === 'error'}
					<div class="wbl-rules__note wbl-rules__note--error">
						<span
							>couldn't load prompt content{classGroup.currentSampleError !== null
								? `: ${classGroup.currentSampleError}`
								: ''}</span
						>
						<button
							type="button"
							class="wbl-retry-btn"
							onclick={() =>
								onRetryModel(classGroup.classKey, findModelIndex(classGroup.classKey, model))}
						>
							Retry
						</button>
					</div>
				{:else if classGroup.currentSampleBlocks === null || classGroup.currentSampleBlocks.length === 0}
					<p class="wbl-rules__note">
						no prompt content sampled — open in workbench to match rules against the last captured
						prompt
					</p>
				{:else if matches.length > 0}
					<span class="wbl-match-chip">
						{matches.length} edit{matches.length === 1 ? '' : 's'}
						{matches.length === 1 ? 'changes' : 'change'} this
					</span>
					<span class="wbl-match-titles"
						>{matches.map((m) => deriveRuleTitle(m.rule)).join(' · ')}</span
					>
				{:else if totalEnabledRules === 0 && orderedRules.length > 0}
					<span class="wbl-rules__note">
						{orderedRules.length} rule{orderedRules.length === 1 ? '' : 's'}
						{orderedRules.length === 1 ? 'is' : 'are'} disabled
					</span>
				{:else}
					<span class="wbl-match-chip wbl-match-chip--none">0 rules match</span>
				{/if}
			</div>
		{/if}
	</div>
{/snippet}

<section
	class="wb-pane wbl-pane"
	data-augmented-ui="tl-clip bl-clip border"
	aria-label="Captured system prompts"
	data-slot="workbench-landing"
>
	<header class="wb-pane-head">
		<span class="wb-pane-label">Captured system prompts</span>
		<span class="wb-pane-hint">
			Every model, its captured prompt classes, and their Claude Code release history.
		</span>
	</header>
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -- keyboard-focusable scroll region (axe scrollable-region-focusable); carries role=region + aria-label -->
	<div
		class="wb-pane-scroll wbl-scroll"
		role="region"
		aria-label="Prompt classes by model"
		tabindex="0"
	>
		{#if status === 'loading'}
			<p class="wbl-status">Loading captured prompts…</p>
		{:else if status === 'error'}
			<div class="wbl-status wbl-status--error" role="alert">
				<p class="wbl-status__title">Couldn't load the prompt catalogue</p>
				<p class="wbl-status__detail">{error}</p>
				<button type="button" class="wbl-retry-btn" onclick={onRetryCatalogue}> Retry </button>
			</div>
		{:else if classes.length === 0}
			<p class="wbl-status">
				No prompts captured yet. Every distinct system prompt your fleet sends will appear here
				automatically once traffic is captured — nothing to configure first.
			</p>
		{:else}
			{#each tree.models as modelBlock (modelBlock.model)}
				{@const modelKey = modelCollapseKey(modelBlock.model)}
				{@const modelCollapsed = isModelCollapsed(modelKey)}
				<article class="wbl-model" data-slot="workbench-landing-model">
					<header class="wbl-model__header">
						<button
							type="button"
							class="wbl-model__toggle"
							aria-expanded={!modelCollapsed}
							onclick={() => toggleModelCollapse(modelKey)}
						>
							<DisclosureMarker open={!modelCollapsed} size={11} />
							<h2 class="wbl-model__title">{modelBlock.model}</h2>
						</button>
						<span class="wbl-model__meta">
							{formatCount(modelBlock.observationCount)} obs (7d) · last seen
							{modelBlock.lastSeen !== null ? formatRelative(modelBlock.lastSeen) : 'never'}
						</span>
					</header>

					{#if !modelCollapsed}
						{#each modelBlock.classes as classGroup (classGroup.classKey)}
							{@const key = groupKey(modelBlock.model, classGroup.classKey)}
							{@const split = splitReleasesForDisplay(classGroup.releases, 5)}
							{@const expanded = expandedGroups.has(key)}
							<div class="wbl-class" data-slot="workbench-landing-class">
								<h3 class="wbl-class__title">
									{classGroup.title}
									<span class="wbl-lane-badge" data-lane={classGroup.lane}>{classGroup.lane}</span>
								</h3>

								{#if classGroup.versionsStatus === 'loading'}
									<p class="wbl-rules__note">checking prompt versions…</p>
								{:else if classGroup.versionsStatus === 'error'}
									<div class="wbl-rules__note wbl-rules__note--error">
										<span>
											couldn't load release history{classGroup.versionsError !== null
												? `: ${classGroup.versionsError}`
												: ''}
										</span>
										<button
											type="button"
											class="wbl-retry-btn"
											onclick={() => onRetryVersions(classGroup.classKey)}
										>
											Retry
										</button>
									</div>
								{:else if classGroup.releases.length === 0}
									<p class="wbl-rules__note">no releases captured for this model</p>
								{:else}
									{#each split.visible as release (releaseRowKey(release))}
										{@render releaseRow(release, classGroup, modelBlock.model)}
									{/each}
									{#if split.collapsed.length > 0}
										<button
											type="button"
											class="wbl-disclosure"
											aria-expanded={expanded}
											onclick={() => toggleGroup(key)}
										>
											<DisclosureMarker open={expanded} size={9} />
											{expanded
												? 'Hide older versions'
												: `${split.collapsed.length} older version${split.collapsed.length === 1 ? '' : 's'}`}
										</button>
										{#if expanded}
											{#each split.collapsed as release (releaseRowKey(release))}
												{@render releaseRow(release, classGroup, modelBlock.model)}
											{/each}
										{/if}
									{/if}
								{/if}
							</div>
						{/each}
					{/if}
				</article>
			{/each}

			{#if tree.neverObserved.length > 0}
				{@const neverObservedCollapsed = isModelCollapsed(NEVER_OBSERVED_COLLAPSE_KEY)}
				<article
					class="wbl-model wbl-model--never-observed"
					data-slot="workbench-landing-never-observed"
				>
					<header class="wbl-model__header">
						<button
							type="button"
							class="wbl-model__toggle"
							aria-expanded={!neverObservedCollapsed}
							onclick={() => toggleModelCollapse(NEVER_OBSERVED_COLLAPSE_KEY)}
						>
							<DisclosureMarker open={!neverObservedCollapsed} size={11} />
							<h2 class="wbl-model__title">Registered, never observed</h2>
						</button>
					</header>
					{#if !neverObservedCollapsed}
						{#each tree.neverObserved as row (row.classKey)}
							<div class="wbl-never-observed-row" data-slot="workbench-landing-never-observed-row">
								<span class="wbl-class__title">{row.title}</span>
								<span class="wbl-lane-badge" data-lane={row.lane}>{row.lane}</span>
								{#if row.registrationLabel !== null}
									<span class="wbl-registration-badge">registered · {row.registrationLabel}</span>
								{/if}
							</div>
						{/each}
					{/if}
				</article>
			{/if}
		{/if}
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

	.wb-pane-hint {
		font: italic 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
	}

	.wb-pane-scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.wbl-status {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		line-height: 1.6;
		margin: 12px 4px;
	}

	.wbl-status--error {
		display: flex;
		flex-direction: column;
		gap: 6px;
		align-items: flex-start;
	}

	.wbl-status__title {
		font-weight: 600;
		color: var(--status-danger-fg);
		margin: 0;
	}

	.wbl-status__detail {
		color: var(--text-2);
		margin: 0;
	}

	.wbl-retry-btn {
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
	.wbl-retry-btn:hover {
		color: var(--interactive-active-fg);
		border-color: var(--interactive-active-border);
	}

	.wbl-model {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.wbl-model__header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 6px;
		padding-bottom: 4px;
		border-bottom: 1px solid var(--border-default);
	}

	.wbl-model__toggle {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		padding: 0;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.wbl-model__toggle:hover .wbl-model__title {
		color: var(--interactive-active-fg);
	}
	.wbl-model__toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 3px;
	}

	.wbl-model__title {
		margin: 0;
		font: 700 13px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		overflow-wrap: break-word;
		transition: color 120ms ease;
	}

	.wbl-model__meta {
		font-size: 10px;
		color: var(--text-3);
	}

	.wbl-model--never-observed .wbl-model__header {
		border-bottom-color: var(--border-subtle);
	}

	.wbl-never-observed-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		padding: 3px 2px;
	}

	.wbl-class {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 6px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 5px;
		background: var(--surface-bg);
	}

	.wbl-class__title {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font: 500 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		overflow-wrap: break-word;
	}

	.wbl-lane-badge {
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

	.wbl-registration-badge {
		display: inline-flex;
		align-items: center;
		padding: 1px 7px;
		font-size: 10px;
		letter-spacing: 0.03em;
		border-radius: 3px;
		background: var(--surface-2);
		border: 1px dashed var(--border-default);
		color: var(--text-3);
	}

	.wbl-row {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 5px 8px;
		border-left: 2px solid var(--border-subtle);
	}

	.wbl-row--current {
		border-left-color: var(--interactive-active-border);
	}

	.wbl-row__identity {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}

	.wbl-release-badge {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-radius: 3px;
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		color: var(--text-3);
	}

	.wbl-release-badge--current {
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg);
		border-color: var(--interactive-active-border);
	}

	.wbl-release-version {
		font: 500 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-1);
	}

	.wbl-row__count {
		font-size: 10px;
		color: var(--text-3);
	}

	.wbl-rules {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}

	.wbl-match-chip {
		display: inline-flex;
		align-items: center;
		padding: 1px 6px;
		font-size: 10px;
		border-radius: 3px;
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg);
		border: 1px solid var(--interactive-active-border);
	}
	.wbl-match-chip--none {
		color: var(--text-3);
		background: var(--surface-2);
		border-color: var(--border-default);
	}

	.wbl-match-titles {
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		overflow-wrap: break-word;
	}

	.wbl-rules__note {
		margin: 0;
		font-size: 10px;
		font-style: italic;
		color: var(--text-3);
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.wbl-rules__note--error {
		color: var(--status-danger-fg);
		font-style: normal;
	}

	.wbl-release-link {
		display: flex;
		align-items: center;
		flex: 1 1 auto;
		min-width: 0;
		gap: 6px;
		text-decoration: none;
		transition: color 120ms ease;
	}
	.wbl-release-link:hover .wbl-release-version,
	.wbl-release-link:focus-visible .wbl-release-version,
	.wbl-release-link:hover .wbl-release-arrow,
	.wbl-release-link:focus-visible .wbl-release-arrow {
		color: var(--interactive-active-fg);
	}
	.wbl-release-link:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 3px;
	}

	.wbl-release-arrow {
		display: inline-flex;
		align-items: center;
		margin-left: auto;
		flex-shrink: 0;
		color: var(--text-3);
		transition: color 120ms ease;
	}

	.wbl-disclosure {
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
	.wbl-disclosure:hover {
		color: var(--text-2);
	}
	.wbl-disclosure:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
</style>
