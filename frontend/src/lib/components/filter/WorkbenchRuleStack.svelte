<script lang="ts">
	import { untrack } from 'svelte';
	import { Plus } from '@lucide/svelte';
	import type { FilterAction, FilterRuleDto, ProjectSummaryDto } from '$lib/types';
	import FilterRuleForm from './FilterRuleForm.svelte';
	import WorkbenchRuleCard from './WorkbenchRuleCard.svelte';
	import WorkbenchOtherPromptRow from './WorkbenchOtherPromptRow.svelte';
	import { applicabilityLabel, ruleApplicability } from './promptCatalogue';
	import type { CatalogueClass } from './promptCatalogue.svelte';
	import { countRuleMatches } from './workbench';
	import {
		activeScopeRules,
		firesOnOpenRequest,
		groupFiringRules,
		groupOtherPromptRules,
		groupRulesByOtherProject,
		rulesMatchingNoPrompt
	} from './ruleGrouping';

	interface Props {
		orderedRules: FilterRuleDto[];
		blockTexts: readonly string[];
		/** The dry-run's authoritative firing ids, or null before the first dry run resolves. */
		firingRuleIds: ReadonlySet<string> | null;
		hasSource: boolean;
		loading: boolean;
		error: string | null;
		catalogueClasses: CatalogueClass[];
		catalogueDetailsSettled: boolean;
		createRequest: { value: string; action: FilterAction } | null;
		onCreateConsumed: () => void;
		onAsk: (rule: FilterRuleDto, quote: string) => void;
		onShowInPrompt: (rule: FilterRuleDto) => void;
		onToggleCollapse: () => void;
		/** The workbench's picked project (null = Global) and the known project list, both
		 *  needed to label edits "Global"/"<project name>" and group other-project edits. */
		pickedProject?: string | null;
		pickedProjectName?: string | null;
		projects?: readonly ProjectSummaryDto[];
	}

	let {
		orderedRules,
		blockTexts,
		firingRuleIds,
		hasSource,
		loading,
		error,
		catalogueClasses,
		catalogueDetailsSettled,
		createRequest,
		onCreateConsumed,
		onAsk,
		onShowInPrompt,
		onToggleCollapse,
		pickedProject = null,
		pickedProjectName = null,
		projects = []
	}: Props = $props();

	const OTHER_PROMPTS_COLLAPSE_KEY = 'proxy-inspector:filter-workbench-other-prompts-collapsed';

	let formOpen = $state(false);
	let formKey = $state(0);
	let openRuleId = $state<string | null>(null);
	let otherPromptsOpen = $state(loadOtherPromptsOpen());

	function loadOtherPromptsOpen(): boolean {
		try {
			return localStorage.getItem(OTHER_PROMPTS_COLLAPSE_KEY) === '1';
		} catch {
			return false;
		}
	}

	function toggleOtherPromptsOpen(): void {
		otherPromptsOpen = !otherPromptsOpen;
		try {
			localStorage.setItem(OTHER_PROMPTS_COLLAPSE_KEY, otherPromptsOpen ? '1' : '0');
		} catch {
			// localStorage unavailable (private mode, storage full) — the choice just stops persisting.
		}
	}

	$effect(() => {
		if (createRequest !== null) {
			untrack(() => {
				formOpen = true;
				formKey += 1;
			});
		}
	});

	function openBlankForm(): void {
		if (createRequest !== null) onCreateConsumed();
		formOpen = true;
		formKey += 1;
	}

	function closeForm(): void {
		formOpen = false;
		if (createRequest !== null) onCreateConsumed();
	}

	function toggleOpen(id: string): void {
		openRuleId = openRuleId === id ? null : id;
	}

	function appliesToText(rule: FilterRuleDto): string {
		if (!catalogueDetailsSettled) return 'checking captured prompts…';
		return applicabilityLabel(ruleApplicability(rule, catalogueClasses));
	}

	/**
	 * Enabled edits first (their saved `sort_order`), disabled ones after (G4) — a disabled
	 * edit stays reachable to switch back on rather than vanishing from the list, but doesn't
	 * clutter the top of "Your edits" with rows that currently do nothing.
	 */
	const displayRules = $derived([
		...orderedRules.filter((r) => r.enabled),
		...orderedRules.filter((r) => !r.enabled)
	]);

	/** Global plus the picked project's own edits — the only ones that can ever fire on the
	 *  open request. Anything scoped to a DIFFERENT project is excluded here and handled by
	 *  `otherProjectGroups` instead (spec: "global edits + the picked project's"). */
	const scopedDisplayRules = $derived(activeScopeRules(displayRules, pickedProject));

	/**
	 * The client-side content-match heuristic used ONLY while no dry run has resolved yet
	 * (`firingRuleIds === null`), so "fires on this request" isn't empty during the initial
	 * 300ms debounce. Once a dry run resolves, `firesOnOpenRequest` reads the server's own
	 * `firingRuleIds` instead — the authoritative source (operator report: rules keyed by
	 * "not in this prompt" without server confirmation misled operators before).
	 */
	function firesHereHeuristic(rule: FilterRuleDto): boolean {
		if (rule.kind === 'inject_reminder') return rule.enabled;
		if (!rule.enabled) return false;
		return countRuleMatches(rule, blockTexts).count > 0;
	}

	/**
	 * Rules that fire on the OPEN request (round-4 regrouping, operator report: "Not in this
	 * prompt" edits keyed by anchor text with no regard for which model/prompt they actually
	 * belong to). Empty when there is no open request (`hasSource` false, e.g. the landing
	 * page) — the landing page has no "here" to fire on, so every rule is grouped by
	 * model · prompt instead, same as the "other prompts" section below.
	 */
	const firingRules = $derived.by(() => {
		if (!hasSource) return [];
		if (firingRuleIds !== null) {
			return scopedDisplayRules.filter((r) => firesOnOpenRequest(r, firingRuleIds));
		}
		return scopedDisplayRules.filter(firesHereHeuristic);
	});

	const firingGroups = $derived(groupFiringRules(firingRules, blockTexts));

	const remainderRules = $derived.by(() => {
		const firingIds = new Set(firingRules.map((r) => r.id));
		return scopedDisplayRules.filter((r) => !firingIds.has(r.id));
	});

	const otherGroups = $derived(groupOtherPromptRules(remainderRules, catalogueClasses));
	const noMatchRules = $derived(rulesMatchingNoPrompt(remainderRules, otherGroups));
	const otherProjectGroups = $derived(
		groupRulesByOtherProject(displayRules, pickedProject, projects)
	);
	const otherCount = $derived(
		remainderRules.length + otherProjectGroups.reduce((n, g) => n + g.rules.length, 0)
	);

	/**
	 * "N changes this prompt" (round-4 defect #4): count only rules that actually fire on
	 * this request — `firingRules` already applies that test.
	 */
	const changeCount = $derived(firingRules.length);
</script>

<section
	class="wb-pane wb-edits-pane"
	aria-label="Your edits"
	data-slot="workbench-edits-panel"
>
	<header class="wb-pane-head">
		<!-- A real heading, not just a styled span — this column collapses independently of the
		     outline, so its own h2 -> the group headings below (h3) can't rely on the outline
		     having supplied an h2 first (axe heading-order). See WorkbenchNavigator.svelte for
		     the matching comment. -->
		<h2 class="wb-pane-head-heading">
			<button
				type="button"
				class="wb-pane-head-toggle"
				aria-expanded="true"
				aria-label="Collapse your edits"
				onclick={onToggleCollapse}
			>
				<span class="wb-pane-label">Your edits</span>
			</button>
		</h2>
		<span class="wb-pane-meta">
			{changeCount} change{changeCount === 1 ? '' : 's'} this prompt
		</span>
		<button type="button" class="wb-add-btn" aria-label="Add edit manually" onclick={openBlankForm}>
			<Plus size={10} aria-hidden="true" />
			Add edit
		</button>
	</header>

	<div class="wb-pane-scroll">
		{#if error}
			<p class="wb-rules-error" role="alert">{error}</p>
		{/if}

		{#if formOpen}
			{#key formKey}
				<div class="wb-create-form" data-slot="workbench-create-form">
					<FilterRuleForm
						prefillValue={createRequest?.value}
						prefillAction={createRequest?.action}
						candidateBlocks={blockTexts}
						density="inline"
						defaultProjectPath={pickedProject}
						onCancel={closeForm}
						onComplete={closeForm}
					/>
				</div>
			{/key}
		{/if}

		{#if loading && orderedRules.length === 0}
			<p class="wb-rules-loading">Loading edits…</p>
		{:else if orderedRules.length === 0}
			{#if !formOpen}
				<div class="wb-empty-state">
					<p class="wb-empty-title">No edits yet</p>
					<p class="wb-empty-hint">
						Add a rule, or ask the prompt assistant to propose a reword for a part of this prompt.
					</p>
				</div>
			{/if}
		{:else}
			{#if firingGroups.length === 0 && hasSource}
				<p class="wb-empty-hint">No edits touch this prompt.</p>
			{:else}
				{#each firingGroups as group (group.heading)}
					<div class="wb-landing-group" data-slot="workbench-firing-group">
						<h3 class="wb-landing-group__heading">{group.heading}</h3>
						<div role="list" aria-label={group.heading}>
							{#each group.rules as rule (rule.id)}
								<div role="listitem">
									<WorkbenchRuleCard
										{rule}
										{blockTexts}
										appliesTo={appliesToText(rule)}
										open={openRuleId === rule.id}
										onToggleOpen={() => toggleOpen(rule.id)}
										{onAsk}
										{onShowInPrompt}
										workbenchProjectPath={pickedProject}
										workbenchProjectName={pickedProjectName}
									/>
								</div>
							{/each}
						</div>
					</div>
				{/each}
			{/if}

			{#if otherCount > 0}
				<button
					type="button"
					class="wb-other-toggle"
					aria-expanded={otherPromptsOpen}
					onclick={toggleOtherPromptsOpen}
				>
					Edits for other prompts ({otherCount})
				</button>
				{#if otherPromptsOpen}
					{#each otherProjectGroups as group (group.projectPath)}
						<div class="wb-other-group" data-slot="workbench-other-project-group">
							<h4 class="wb-other-group__heading">{group.projectName}</h4>
							<div role="list" aria-label={group.projectName}>
								{#each group.rules as rule (rule.id)}
									<div role="listitem">
										<WorkbenchOtherPromptRow {rule} latestRequestId={null} />
									</div>
								{/each}
							</div>
						</div>
					{/each}
					{#each otherGroups as group (group.key)}
						<div class="wb-other-group" data-slot="workbench-other-prompt-group">
							<h4 class="wb-other-group__heading">{group.model} · {group.classTitle}</h4>
							<div role="list" aria-label={`${group.model} · ${group.classTitle}`}>
								{#each group.rules as rule (rule.id)}
									<div role="listitem">
										<WorkbenchOtherPromptRow {rule} latestRequestId={group.latestRequestId} />
									</div>
								{/each}
							</div>
						</div>
					{/each}
					{#if noMatchRules.length > 0}
						<div class="wb-other-group" data-slot="workbench-no-match-group">
							<h4 class="wb-other-group__heading">Matches no recent prompt</h4>
							<div role="list" aria-label="Matches no recent prompt">
								{#each noMatchRules as rule (rule.id)}
									<div role="listitem">
										<WorkbenchOtherPromptRow {rule} latestRequestId={null} />
									</div>
								{/each}
							</div>
						</div>
					{/if}
				{/if}
			{/if}
		{/if}
	</div>
</section>

<style>
	.wb-pane {
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	/* The edits column is now a full-height sibling of the prompt/assistant columns (not a
	   fixed-pixel-height div stacked above the assistant), so its root needs to actively fill
	   the aside `.wb-workspace`'s row cross-axis stretch already gives it, the same way
	   `WorkbenchAssistantPanel`'s own `.wb-assistant` root does. */
	.wb-edits-pane {
		height: 100%;
	}

	.wb-pane-head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 10px;
		min-height: 38px;
		padding: 0 10px 0 12px;
		border-bottom: 1px solid var(--border-subtle);
		flex-shrink: 0;
	}

	.wb-pane-label {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-1);
	}

	.wb-pane-meta {
		font-size: 11px;
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
		padding: 4px 0 10px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.wb-add-btn {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		background: transparent;
		border: 1px dashed var(--border-default);
		border-radius: 4px;
		cursor: pointer;
		transition:
			color 120ms ease,
			border-color 120ms ease;
	}
	.wb-add-btn:hover {
		color: var(--funk-cyan);
		border-color: var(--funk-cyan-border);
	}
	.wb-add-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-create-form {
		padding: 8px 10px 4px;
	}

	.wb-rules-error {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-danger-fg);
		margin: 0;
		padding: 6px 12px;
	}

	.wb-rules-loading {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		margin: 0;
		padding: 6px 12px;
	}

	.wb-empty-state {
		padding: 24px 12px;
		text-align: center;
	}
	.wb-empty-title {
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		margin: 0 0 6px;
	}
	.wb-empty-hint {
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		margin: 0;
		padding: 8px 12px;
		line-height: 1.6;
	}

	.wb-landing-group {
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding-top: 4px;
	}

	.wb-landing-group__heading {
		margin: 0;
		padding: 3px 12px;
		font: 700 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}

	.wb-other-toggle {
		margin: 6px 10px 2px;
		align-self: flex-start;
		padding: 3px 8px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-2);
		background: transparent;
		border: 1px solid var(--border-default);
		border-radius: 4px;
		cursor: pointer;
		transition:
			color 120ms ease,
			border-color 120ms ease;
	}
	.wb-other-toggle:hover {
		color: var(--text-strong);
		border-color: var(--border-strong);
	}
	.wb-other-toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-other-group {
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding-top: 4px;
	}

	.wb-other-group__heading {
		margin: 0;
		padding: 3px 12px;
		font: 700 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
</style>
