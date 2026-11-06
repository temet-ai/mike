<script lang="ts">
	import { Switch } from '$lib/components/ui/switch';
	import { getContext } from 'svelte';
	import { Trash2 } from '@lucide/svelte';
	import { FILTER_RULES_STORE_KEY, type FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import type { FilterRuleDto } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import FilterRuleForm from './FilterRuleForm.svelte';
	import { deriveRuleTitle, ruleBeforeAfter, ruleLandingLabel } from './rule-matching';

	interface Props {
		rule: FilterRuleDto;
		blockTexts: readonly string[];
		/** "Reach" text — how far this edit's effect extends beyond this one prompt. */
		appliesTo: string;
		open: boolean;
		onToggleOpen: () => void;
		onAsk: (rule: FilterRuleDto, quote: string) => void;
		onShowInPrompt: (rule: FilterRuleDto) => void;
		/** The workbench's picked project (null = Global) — drives the "Only <project>" action's
		 *  target and label; independent of `rule.project_path`, which is this rule's OWN scope. */
		workbenchProjectPath?: string | null;
		workbenchProjectName?: string | null;
	}

	let {
		rule,
		blockTexts,
		appliesTo,
		open,
		onToggleOpen,
		onAsk,
		onShowInPrompt,
		workbenchProjectPath = null,
		workbenchProjectName = null
	}: Props = $props();

	const store = getContext<FilterRulesStore | undefined>(FILTER_RULES_STORE_KEY);

	let rowEl = $state<HTMLDivElement | null>(null);
	let busy = $state(false);
	let highlight = $state(false);
	let editingByHand = $state(false);
	let confirmingDelete = $state(false);
	let highlightTimer: ReturnType<typeof setTimeout> | null = null;
	let scopeChangeError = $state<string | null>(null);

	const title = $derived(deriveRuleTitle(rule));
	const where = $derived(ruleLandingLabel(rule, blockTexts));
	const { before, after } = $derived(ruleBeforeAfter(rule));
	const projectPath = $derived(rule.project_path ?? null);
	/**
	 * This card only ever renders a rule that's already active-in-scope (global, or scoped to
	 * the WORKBENCH's own picked project — see `activeScopeRules` upstream in
	 * `WorkbenchRuleStack.svelte`; a rule scoped to any OTHER project renders through
	 * `WorkbenchOtherPromptRow` instead), so when `projectPath` is set it's always
	 * `workbenchProjectPath` and `workbenchProjectName` is always its real name — never falling
	 * back to the bare path basename except when that name genuinely isn't known yet.
	 */
	const projectLabel = $derived(
		projectPath === null
			? 'Global'
			: (workbenchProjectName ?? projectPath.split('/').pop() ?? projectPath)
	);
	/** Only offered when there's a picked project to scope TO — nothing to narrow down to while
	 *  Global itself is picked. */
	const canScopeToWorkbenchProject = $derived(
		workbenchProjectPath !== null && projectPath !== workbenchProjectPath
	);

	async function setProjectScope(next: string | null): Promise<void> {
		if (!store || busy) return;
		busy = true;
		scopeChangeError = null;
		try {
			await store.update(rule.id, { project_path: next });
		} catch (err) {
			scopeChangeError = err instanceof Error ? err.message : String(err);
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		if (!store) return;
		if (store.scrollTargetRuleId !== rule.id) return;
		if (!open) onToggleOpen();
		rowEl?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		highlight = true;
		if (highlightTimer !== null) clearTimeout(highlightTimer);
		highlightTimer = setTimeout(() => {
			highlight = false;
			highlightTimer = null;
		}, 1200);
		store.scrollTargetRuleId = null;
	});

	async function onToggleEnabled(): Promise<void> {
		if (!store || busy) return;
		busy = true;
		try {
			await store.update(rule.id, { enabled: !rule.enabled });
		} finally {
			busy = false;
		}
	}

	function requestDelete(): void {
		confirmingDelete = true;
	}

	function cancelDelete(): void {
		confirmingDelete = false;
	}

	async function confirmDelete(): Promise<void> {
		if (!store || busy) return;
		busy = true;
		try {
			await store.remove(rule.id);
		} finally {
			busy = false;
			confirmingDelete = false;
		}
	}

	function handleAsk(): void {
		onAsk(rule, after.length > 0 ? after : before);
	}

	function handleEditComplete(): void {
		editingByHand = false;
	}
</script>

<div
	bind:this={rowEl}
	class="wb-edit-row"
	class:wb-edit-row--disabled={!rule.enabled}
	class:wb-edit-row--open={open}
	class:wb-edit-row--highlight={highlight}
	data-slot="workbench-edit-row"
	data-rule-id={rule.id}
>
	<div class="wb-edit-row__header">
		<!-- Deliberate departure from the approved design canvas, which shows a rotating ▸ caret
		     here: this app's binding interaction convention bans expand/collapse carets everywhere
		     (see the agent's own "no chevron" standing feedback). Clicking the row header itself
		     toggles disclosure instead, same as every other collapsible section in this codebase;
		     aria-expanded still carries the state for assistive tech. -->
		<button type="button" class="wb-edit-row__toggle" aria-expanded={open} onclick={onToggleOpen}>
			<span class="wb-edit-row__title-row">
				<span class="wb-edit-row__title">{title}</span>
				<span
					class="wb-edit-row__project"
					class:wb-edit-row__project--global={projectPath === null}
					title={projectPath !== null
						? `Scoped to project: ${projectPath}`
						: 'Applies to every project'}
				>
					{projectLabel}
				</span>
			</span>
			<span class="wb-edit-row__where">{where}</span>
		</button>
		{#if confirmingDelete}
			<div class="wb-edit-row__confirm" role="group" aria-label={`Confirm delete: ${title}`}>
				<span class="wb-edit-row__confirm-label">Delete?</span>
				<button
					type="button"
					class="wb-edit-row__confirm-btn wb-edit-row__confirm-btn--yes"
					onclick={confirmDelete}
					disabled={busy}
				>
					Yes
				</button>
				<button
					type="button"
					class="wb-edit-row__confirm-btn"
					onclick={cancelDelete}
					disabled={busy}
				>
					Cancel
				</button>
			</div>
		{:else}
			<Tooltip text="Delete this rule">
				{#snippet child({ props })}
					<Button
						{...props}
						variant="ghost"
						size="icon"
						class="size-6"
						onclick={requestDelete}
						disabled={busy}
						aria-label={`Delete edit: ${title}`}
					>
						<Trash2 class="size-3 text-[var(--status-danger-fg)]" aria-hidden="true" />
					</Button>
				{/snippet}
			</Tooltip>
		{/if}
		<Tooltip text="Toggle this edit on or off without deleting it">
			{#snippet child({ props })}
				<Switch
					{...props}
					bind:checked={() => rule.enabled, () => onToggleEnabled()}
					aria-label={rule.enabled ? `Turn off: ${title}` : `Turn on: ${title}`}
					disabled={busy}
				/>
			{/snippet}
		</Tooltip>
	</div>

	{#if open}
		<div class="wb-edit-row__body">
			{#if before.length > 0}
				<div class="wb-edit-row__before">{before}</div>
			{/if}
			{#if after.length > 0}
				<div class="wb-edit-row__after">{after}</div>
			{/if}
			<span class="wb-edit-row__reach">{appliesTo}</span>
			<div class="wb-edit-row__actions">
				<button
					type="button"
					class="wb-edit-row__action wb-edit-row__action--assistant"
					onclick={handleAsk}
				>
					Reword with assistant
				</button>
				<button
					type="button"
					class="wb-edit-row__action wb-edit-row__action--jump"
					onclick={() => onShowInPrompt(rule)}
				>
					Show in prompt
				</button>
				<button
					type="button"
					class="wb-edit-row__action wb-edit-row__action--hand"
					aria-expanded={editingByHand}
					onclick={() => (editingByHand = !editingByHand)}
				>
					{editingByHand ? 'Close hand edit' : 'Edit by hand'}
				</button>
				{#if projectPath !== null}
					<button
						type="button"
						class="wb-edit-row__action"
						disabled={busy}
						onclick={() => void setProjectScope(null)}
					>
						Make global
					</button>
				{:else if canScopeToWorkbenchProject}
					<button
						type="button"
						class="wb-edit-row__action"
						disabled={busy}
						onclick={() => void setProjectScope(workbenchProjectPath)}
					>
						Only {workbenchProjectName ?? workbenchProjectPath}
					</button>
				{/if}
			</div>
			{#if scopeChangeError !== null}
				<p class="wb-edit-row__scope-error" role="alert">{scopeChangeError}</p>
			{/if}
			{#if editingByHand}
				<div class="wb-edit-row__form">
					<FilterRuleForm
						initialRule={rule}
						density="inline"
						onCancel={() => (editingByHand = false)}
						onComplete={handleEditComplete}
					/>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.wb-edit-row {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 2px 0;
		border-left: 2px solid transparent;
		transition:
			background 150ms ease,
			border-color 150ms ease,
			opacity 120ms ease;
	}

	.wb-edit-row--disabled {
		opacity: 0.55;
	}

	.wb-edit-row--open {
		background: var(--surface-2);
	}

	.wb-edit-row--highlight {
		border-left-color: var(--funk-cyan);
		background: var(--funk-cyan-dim);
	}

	.wb-edit-row__header {
		display: flex;
		align-items: center;
		gap: 6px;
		min-height: 36px;
		padding: 0 8px 0 10px;
	}

	.wb-edit-row__toggle {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding: 6px 0;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}
	.wb-edit-row__toggle:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
		border-radius: 3px;
	}

	.wb-edit-row__title-row {
		display: flex;
		align-items: baseline;
		gap: 6px;
		flex-wrap: wrap;
	}

	.wb-edit-row__title {
		font: 12px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-strong);
		overflow-wrap: break-word;
	}

	.wb-edit-row__project {
		flex-shrink: 0;
		padding: 1px 6px;
		font: 9px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--text-3);
		border: 1px solid var(--border-default);
		border-radius: 3px;
		white-space: nowrap;
	}
	.wb-edit-row__project--global {
		color: var(--funk-cyan);
		border-color: var(--funk-cyan-border);
	}

	.wb-edit-row__where {
		font-size: 10px;
		color: var(--text-3);
		overflow-wrap: break-word;
	}

	.wb-edit-row__confirm {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.wb-edit-row__confirm-label {
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		color: var(--status-danger-fg);
	}

	.wb-edit-row__confirm-btn {
		height: 22px;
		padding: 0 8px;
		font: 10px var(--font-mono, 'JetBrains Mono Variable', monospace);
		background: transparent;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		cursor: pointer;
		transition:
			color 120ms ease,
			border-color 120ms ease,
			background 120ms ease;
	}
	.wb-edit-row__confirm-btn:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.wb-edit-row__confirm-btn--yes {
		color: var(--status-danger-fg);
		border-color: var(--status-danger-fg);
	}
	.wb-edit-row__confirm-btn--yes:hover {
		background: var(--status-danger-fg);
		color: var(--surface-1);
	}

	.wb-edit-row__body {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 2px 10px 10px 10px;
	}

	.wb-edit-row__before {
		padding: 5px 7px;
		border-left: 2px solid var(--border-strong);
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		line-height: 1.55;
		color: var(--text-3);
		text-decoration: line-through;
		text-decoration-color: var(--border-strong);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.wb-edit-row__after {
		padding: 5px 7px;
		border-left: 2px solid var(--funk-green-border);
		background: var(--funk-green-dim);
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		line-height: 1.55;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
	}

	.wb-edit-row__reach {
		font-size: 10px;
		color: var(--text-3);
		overflow-wrap: break-word;
	}

	.wb-edit-row__actions {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.wb-edit-row__action {
		height: 26px;
		padding: 0 10px;
		font: 11px var(--font-mono, 'JetBrains Mono Variable', monospace);
		background: transparent;
		border: 1px solid var(--border-default);
		color: var(--text-2);
		cursor: pointer;
		transition:
			color 120ms ease,
			border-color 120ms ease,
			background 120ms ease;
	}
	.wb-edit-row__action:hover {
		color: var(--text-strong);
	}
	.wb-edit-row__action:focus-visible {
		outline: none;
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.wb-edit-row__action--assistant {
		color: var(--funk-magenta);
		background: var(--funk-chip-magenta);
		border-color: var(--funk-magenta-border);
	}
	.wb-edit-row__action--jump {
		color: var(--funk-cyan);
		border-color: var(--funk-cyan-border);
	}

	.wb-edit-row__form {
		padding-top: 2px;
	}

	.wb-edit-row__scope-error {
		margin: 0 10px 6px;
		font-size: 10px;
		color: var(--status-danger-fg);
	}
</style>
