<script lang="ts">
	import { getContext } from 'svelte';
	import { FILTER_RULES_STORE_KEY, type FilterRulesStore } from '$lib/stores/filterRules.svelte';
	import type { FilterAction, FilterKind, FilterRuleDto } from '$lib/types';
	import { cn } from '$lib/utils';
	import { Button } from '$lib/components/ui/button';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import {
		matches,
		prefillFrom,
		splitBlockIntoLines,
		splitBlockIntoParagraphs,
		PREFILL_TEXT_SNIPPET_MAX
	} from './rule-matching';

	type Preset = 'paragraph' | 'line' | 'custom';

	interface Props {
		blockText?: string;
		candidateBlocks?: readonly string[];
		initialRule?: FilterRuleDto;
		prefillValue?: string;
		prefillAction?: FilterAction;
		onCancel?: () => void;
		onComplete?: (rule: FilterRuleDto) => void;
		density?: 'inline' | 'popover';
		/** The workbench's picked project — a NEW rule is scoped to it (null = Global). Ignored
		 *  when editing an existing rule, whose own project_path is changed only through the
		 *  edit card's explicit "Make global"/"Only <project>" action, never by re-saving the form. */
		defaultProjectPath?: string | null;
	}

	let {
		blockText = '',
		candidateBlocks = [],
		initialRule,
		prefillValue,
		prefillAction,
		onCancel,
		onComplete,
		density = 'inline',
		defaultProjectPath = null
	}: Props = $props();

	const store = getContext<FilterRulesStore | undefined>(FILTER_RULES_STORE_KEY);

	const isEdit = $derived(initialRule !== undefined);

	function computeDefaults(): {
		kind: FilterKind;
		value: string;
		action: FilterAction;
		text: string;
		title: string;
		triggerToken: string;
	} {
		if (initialRule) {
			return {
				kind: initialRule.kind,
				value: initialRule.value,
				action: initialRule.action,
				text: initialRule.text ?? '',
				title: initialRule.title ?? '',
				triggerToken: initialRule.trigger_token ?? ''
			};
		}
		if (prefillValue !== undefined && prefillValue.length > 0) {
			return {
				kind: 'text_substring',
				value: prefillValue,
				action: prefillAction ?? 'drop',
				text: '',
				title: '',
				triggerToken: ''
			};
		}
		const pf = prefillFrom(blockText);
		return {
			kind: pf.kind,
			value: pf.value,
			action: 'drop',
			text: '',
			title: '',
			triggerToken: ''
		};
	}

	const initialDefaults = computeDefaults();
	let kind = $state<FilterKind>(initialDefaults.kind);
	let value = $state<string>(initialDefaults.value);
	let action = $state<FilterAction>(initialDefaults.action);
	let text = $state<string>(initialDefaults.text);
	let triggerToken = $state<string>(initialDefaults.triggerToken);
	let title = $state<string>(initialDefaults.title);
	let submitting = $state(false);
	let submitError = $state<string | null>(null);

	let preset = $state<Preset>('custom');

	$effect(() => {
		if (kind !== 'text_substring' || isEdit) {
			preset = 'custom';
		}
	});

	const presetAvailable = $derived(kind === 'text_substring' && !isEdit);
	const isLazy = $derived(kind === 'lazy_section');
	const isInjectReminder = $derived(kind === 'inject_reminder');

	function selectKind(next: FilterKind): void {
		kind = next;
		if (next === 'lazy_section' || next === 'inject_reminder') {
			action = 'replace';
		}
	}

	function applyPreset(next: Preset): void {
		preset = next;
		if (next === 'custom') {
			return;
		}
		const parts =
			next === 'paragraph' ? splitBlockIntoParagraphs(blockText) : splitBlockIntoLines(blockText);
		const first = parts[0];
		if (first === undefined) {
			preset = 'custom';
			return;
		}
		value = first.text.slice(0, PREFILL_TEXT_SNIPPET_MAX);
	}

	const matchCount = $derived.by(() => {
		if (isEdit) return null;
		if (value.trim().length === 0) return 0;
		let n = 0;
		for (const block of candidateBlocks) {
			if (matches(kind, value, block)) n++;
		}
		return n;
	});

	const textRequired = $derived(action === 'replace' || action === 'append');
	const canSubmit = $derived.by(() => {
		if (submitting) return false;
		if (value.trim().length === 0) return false;
		if (textRequired && text.length === 0) return false;
		if (isLazy) {
			if (action !== 'replace') return false;
			if (triggerToken.trim().length === 0) return false;
		}
		if (isInjectReminder) {
			if (action !== 'replace') return false;
			if (text.length === 0) return false;
			if (triggerToken.trim().length === 0) return false;
		}
		return true;
	});

	const previewLabel = $derived.by(() => {
		if (isInjectReminder) {
			const marker = triggerToken.trim().length > 0 ? triggerToken.trim() : '<marker>';
			return `When the model emits \`${marker}\` in its latest turn, this reminder is injected once into the following turn.`;
		}
		if (isEdit || matchCount === null) return null;
		const n = matchCount;
		const plural = n === 1 ? 'block' : 'blocks';
		if (isLazy) {
			return `Heading found in ${n} ${plural} — the section below it will be replaced with the stub.`;
		}
		if (action === 'drop') {
			return `Will drop ${n} ${plural} in this request.`;
		}
		if (action === 'replace') {
			const tokEst = Math.max(1, Math.round(text.length / 4));
			return `Will replace ${n} ${plural} with your text (${text.length} chars, ~${tokEst} tokens).`;
		}
		const tokEst = Math.max(1, Math.round(text.length / 4));
		return `Will insert your text after ${n} ${plural} (${text.length} chars, ~${tokEst} tokens).`;
	});

	async function onSubmit(e: Event): Promise<void> {
		e.preventDefault();
		if (!store) {
			submitError = 'Filter rules store not mounted.';
			return;
		}
		if (!canSubmit) return;
		submitting = true;
		submitError = null;
		try {
			const payload: {
				kind: FilterKind;
				value: string;
				action: FilterAction;
				text: string | null;
				title: string | null;
				trigger_token?: string | null;
				project_path?: string;
			} = {
				kind,
				value: value.trim(),
				action,
				text: textRequired ? text : null,
				title: title.trim().length > 0 ? title.trim() : null
			};
			if (isLazy || isInjectReminder) {
				payload.trigger_token = triggerToken.trim();
			} else if (isEdit && typeof initialRule?.trigger_token === 'string') {
				payload.trigger_token = null;
			}
			if (!isEdit && defaultProjectPath !== null) {
				payload.project_path = defaultProjectPath;
			}
			const rule =
				isEdit && initialRule
					? await store.update(initialRule.id, payload)
					: await store.create(payload);
			onComplete?.(rule);
		} catch (err) {
			submitError = err instanceof Error ? err.message : String(err);
		} finally {
			submitting = false;
		}
	}

	function handleCancel(): void {
		onCancel?.();
	}

	function onFieldKeydown(e: KeyboardEvent): void {
		if (e.key === 'Escape') {
			e.stopPropagation();
			handleCancel();
		}
	}
</script>

<form
	onsubmit={onSubmit}
	class={cn('flex flex-col gap-3', density === 'popover' ? '' : 'filter-rule-form--inline')}
	data-slot="filter-rule-form"
	data-augmented-ui={density === 'popover' ? undefined : 'tl-clip br-clip border'}
>
	<div class="flex items-center justify-between">
		<h3 class="filter-rule-form__title text-label uppercase tracking-kpi text-[var(--text-3)]">
			{isEdit ? 'Edit filter rule' : 'Add filter rule'}
		</h3>
	</div>

	<div class="flex flex-col gap-1">
		<span
			class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
			id="filter-rule-form-title-label"
		>
			<Tooltip
				text="A plain-language name for this edit, shown instead of a rule id anywhere the edit list appears. Leave blank to use a title derived from what the rule does."
				slug="filter-rule"
			>
				<span>Title</span>
			</Tooltip>
		</span>
		<input
			type="text"
			bind:value={title}
			autocomplete="off"
			spellcheck="false"
			placeholder="e.g. Removes the browser-automation reminder"
			aria-labelledby="filter-rule-form-title-label"
			onkeydown={onFieldKeydown}
			class={cn(
				'rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none',
				'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
			)}
		/>
	</div>

	<fieldset class="flex flex-col gap-1.5">
		<legend
			class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
		>
			<Tooltip
				text="How the rule’s value is matched. xml_tag matches a wrapping <tag>…</tag>; text_substring matches anywhere in the block text; lazy_section matches a heading line and lazily swaps its section for a stub."
				slug="filter-rule-kind"
			>
				<span>Match shape</span>
			</Tooltip>
		</legend>
		<div class="flex items-center gap-1.5">
			<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
				<input
					type="radio"
					name="kind"
					value="text_substring"
					checked={kind === 'text_substring'}
					onchange={() => selectKind('text_substring')}
					class="accent-[var(--interactive-active-fg)]"
					onkeydown={onFieldKeydown}
				/>
				Text substring
			</label>
			<HelpHint slug="kind-text-substring" />
		</div>
		<div class="flex items-center gap-1.5">
			<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
				<input
					type="radio"
					name="kind"
					value="xml_tag"
					checked={kind === 'xml_tag'}
					onchange={() => selectKind('xml_tag')}
					class="accent-[var(--interactive-active-fg)]"
					onkeydown={onFieldKeydown}
				/>
				XML tag wrapping
			</label>
			<HelpHint slug="kind-xml-tag" />
		</div>
		<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
			<input
				type="radio"
				name="kind"
				value="lazy_section"
				checked={kind === 'lazy_section'}
				onchange={() => selectKind('lazy_section')}
				class="accent-[var(--interactive-active-fg)]"
				onkeydown={onFieldKeydown}
			/>
			Lazy section (heading)
		</label>
		<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
			<input
				type="radio"
				name="kind"
				value="inject_reminder"
				checked={kind === 'inject_reminder'}
				onchange={() => selectKind('inject_reminder')}
				class="accent-[var(--interactive-active-fg)]"
				onkeydown={onFieldKeydown}
			/>
			Inject reminder (on trigger)
		</label>
	</fieldset>

	{#if isInjectReminder}
		<div class="flex flex-col gap-1">
			<span
				class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
				id="filter-rule-form-label-label"
			>
				<Tooltip
					text="A short name for this reminder — shown in the workbench rule card. Not matched against anything."
					slug="filter-rule"
				>
					<span>Label</span>
				</Tooltip>
			</span>
			<input
				type="text"
				bind:value
				autocomplete="off"
				spellcheck="false"
				placeholder="e.g. browser-guidance"
				aria-labelledby="filter-rule-form-label-label"
				aria-describedby="filter-rule-form-label-help"
				onkeydown={onFieldKeydown}
				class={cn(
					'rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none',
					'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
				)}
			/>
			<p id="filter-rule-form-label-help" class="text-caption text-[var(--text-3)]">
				A short name for this reminder — used in the workbench rule card. Not matched against
				anything.
			</p>
		</div>

		<label class="flex flex-col gap-1">
			<span
				class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
			>
				<Tooltip
					text="The full reminder content injected into the next request when the trigger fires. The model sees it verbatim."
					slug="filter-rule"
				>
					<span>Reminder text</span>
				</Tooltip>
			</span>
			<textarea
				bind:value={text}
				rows="5"
				placeholder="The full reminder content to inject — e.g. Use browser_action for web interaction."
				autocomplete="off"
				spellcheck="false"
				onkeydown={onFieldKeydown}
				title="Reminder text. Required for inject_reminder."
				class={cn(
					'min-h-[7rem] max-h-[24rem] resize-y rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none',
					'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
				)}></textarea>
			<p class="text-caption text-[var(--text-3)]">
				Content injected into the system field once per context window when the trigger fires. Write
				it as a complete, self-contained reminder — the model will see it verbatim.
			</p>
		</label>

		<div class="flex flex-col gap-1">
			<span
				class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
				id="filter-rule-form-marker-label"
			>
				<Tooltip
					text="When the model emits this token in an assistant turn, the reminder is injected into the next request."
					slug="filter-rule"
				>
					<span>Trigger marker</span>
				</Tooltip>
			</span>
			<input
				type="text"
				bind:value={triggerToken}
				autocomplete="off"
				spellcheck="false"
				placeholder="e.g. <chrome>"
				aria-labelledby="filter-rule-form-marker-label"
				aria-describedby="filter-rule-form-marker-help"
				onkeydown={onFieldKeydown}
				class={cn(
					'rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none',
					'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
				)}
			/>
			<p id="filter-rule-form-marker-help" class="text-caption text-[var(--text-3)]">
				Required. When the model emits this token in an assistant turn, the reminder is injected
				into the next request. Pick a distinctive marker the model can emit intentionally (e.g.
				&lt;chrome&gt;).
			</p>
		</div>
	{:else}
		<div class="flex flex-col gap-1">
			<div class="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
				<span
					class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
					id="filter-rule-form-value-label"
				>
					<Tooltip
						text={isLazy
							? 'The heading line that opens the section to strip — e.g. “# Claude in Chrome browser automation”. The section runs to the next “# ” heading or the end of the block.'
							: 'What to match. For xml_tag, just the tag name (no angle brackets). For text_substring, a literal substring.'}
						slug="filter-rule"
					>
						<span>{isLazy ? 'Heading line' : 'Value'}</span>
					</Tooltip>
				</span>
				{#if presetAvailable}
					<fieldset
						class="flex flex-wrap items-center gap-x-3 gap-y-1"
						data-slot="filter-rule-form-preset"
					>
						<legend class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] mr-1">
							Preset
						</legend>
						<label class="flex items-center gap-1.5 text-xs text-[var(--text-1)]">
							<input
								type="radio"
								name="preset"
								value="paragraph"
								checked={preset === 'paragraph'}
								onchange={() => applyPreset('paragraph')}
								class="accent-[var(--interactive-active-fg)]"
								onkeydown={onFieldKeydown}
							/>
							Paragraph
						</label>
						<label class="flex items-center gap-1.5 text-xs text-[var(--text-1)]">
							<input
								type="radio"
								name="preset"
								value="line"
								checked={preset === 'line'}
								onchange={() => applyPreset('line')}
								class="accent-[var(--interactive-active-fg)]"
								onkeydown={onFieldKeydown}
							/>
							Line
						</label>
						<label class="flex items-center gap-1.5 text-xs text-[var(--text-1)]">
							<input
								type="radio"
								name="preset"
								value="custom"
								checked={preset === 'custom'}
								onchange={() => applyPreset('custom')}
								class="accent-[var(--interactive-active-fg)]"
								onkeydown={onFieldKeydown}
							/>
							Custom
						</label>
					</fieldset>
				{/if}
			</div>
			<textarea
				bind:value
				autocomplete="off"
				spellcheck="false"
				aria-labelledby="filter-rule-form-value-label"
				aria-describedby="filter-rule-form-value-help"
				onkeydown={onFieldKeydown}
				rows="4"
				class={cn(
					'min-h-[5.5rem] max-h-[24rem] resize-y rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none',
					'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
				)}></textarea>
			<p id="filter-rule-form-value-help" class="text-caption text-[var(--text-3)]">
				{#if isLazy}
					The heading line, verbatim — e.g. “# Claude in Chrome browser automation”. Blocks
					containing this exact line are matched and the section below it is replaced with the stub.
				{:else}
					Defaults to the full block text — any block containing this substring will be matched.
					Edit to narrow the match (use the Paragraph or Line preset to grab a smaller slice).
				{/if}
			</p>
		</div>

		<fieldset class="flex flex-col gap-1.5">
			<legend
				class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
			>
				<Tooltip
					text="What to do when the match fires (drop / replace / append)."
					slug="filter-rule-action"
				>
					<span>Action</span>
				</Tooltip>
			</legend>
			<div class="flex items-center gap-1.5">
				<label
					class="flex items-center gap-2 text-xs text-[var(--text-1)]"
					class:opacity-50={isLazy}
				>
					<input
						type="radio"
						name="action"
						value="drop"
						checked={action === 'drop'}
						disabled={isLazy}
						onchange={() => (action = 'drop')}
						class="accent-[var(--interactive-active-fg)]"
						onkeydown={onFieldKeydown}
					/>
					Drop block
				</label>
				<HelpHint slug="action-drop" />
			</div>
			<div class="flex items-center gap-1.5">
				<label class="flex items-center gap-2 text-xs text-[var(--text-1)]">
					<input
						type="radio"
						name="action"
						value="replace"
						checked={action === 'replace'}
						onchange={() => (action = 'replace')}
						class="accent-[var(--interactive-active-fg)]"
						onkeydown={onFieldKeydown}
					/>
					{isLazy ? 'Replace section with the stub' : 'Replace block with'}
				</label>
				<HelpHint slug="action-replace" />
			</div>
			<div class="flex items-center gap-1.5">
				<label
					class="flex items-center gap-2 text-xs text-[var(--text-1)]"
					class:opacity-50={isLazy}
				>
					<input
						type="radio"
						name="action"
						value="append"
						checked={action === 'append'}
						disabled={isLazy}
						onchange={() => (action = 'append')}
						class="accent-[var(--interactive-active-fg)]"
						onkeydown={onFieldKeydown}
					/>
					Insert text after block
				</label>
				<HelpHint slug="action-append" />
			</div>
			{#if isLazy}
				<p class="text-caption text-[var(--text-3)]">
					lazy_section rules always replace — the stub stands in for the section until the model
					outputs the trigger token.
				</p>
			{/if}
		</fieldset>

		{#if textRequired}
			<label class="flex flex-col gap-1">
				{#if isLazy}
					<span
						class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
					>
						<Tooltip
							text="The stub that replaces the section in every request. It should tell the model how to get the full section back — i.e. carry the trigger token."
							slug="filter-rule"
						>
							<span>Stub</span>
						</Tooltip>
					</span>
				{:else}
					<span class="sr-only">Replacement or appended text</span>
				{/if}
				<textarea
					bind:value={text}
					rows="4"
					placeholder={isLazy
						? 'Stub text that replaces the section — should mention the trigger token…'
						: action === 'replace'
							? 'Replacement text…'
							: 'Text to append after each matching block…'}
					onkeydown={onFieldKeydown}
					title={isLazy
						? 'Stub text. Required for lazy_section.'
						: 'Replacement / appended text. Required for replace and append.'}
					class={cn(
						'rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none resize-y',
						'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
					)}></textarea>
			</label>
		{/if}

		{#if isLazy}
			<div class="flex flex-col gap-1">
				<span
					class="text-label font-mono uppercase tracking-kpi text-[var(--text-3)] inline-flex items-center gap-1"
					id="filter-rule-form-trigger-label"
				>
					<Tooltip
						text="When the model outputs this token in an assistant turn, the stripped section is injected into the next request as a system-reminder."
						slug="filter-rule"
					>
						<span>Trigger token</span>
					</Tooltip>
				</span>
				<input
					type="text"
					bind:value={triggerToken}
					autocomplete="off"
					spellcheck="false"
					placeholder="e.g. !!CHROME!!"
					aria-labelledby="filter-rule-form-trigger-label"
					aria-describedby="filter-rule-form-trigger-help"
					onkeydown={onFieldKeydown}
					class={cn(
						'rounded border border-[var(--border-default)] bg-[var(--surface-bg)] px-2 py-1.5 font-mono text-xs text-[var(--text-strong)] outline-none',
						'focus-visible:border-[var(--interactive-focus-ring)] focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]'
					)}
				/>
				<p id="filter-rule-form-trigger-help" class="text-caption text-[var(--text-3)]">
					Required. Pick a distinctive token (e.g. !!CHROME!!) and mention it in the stub so the
					model knows to emit it before using the stripped guidance.
				</p>
			</div>
		{/if}
	{/if}

	{#if previewLabel}
		<p class="text-label font-mono tabular-nums text-[var(--text-2)]">
			{previewLabel}
		</p>
	{/if}

	{#if submitError}
		<div
			class="flex items-start gap-2 rounded border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] p-2 text-xs text-[var(--status-danger-fg)]"
			role="alert"
			aria-live="assertive"
		>
			<span aria-hidden="true" class="font-mono font-semibold">!</span>
			<span class="font-mono">{submitError}</span>
		</div>
	{/if}

	<div class="flex items-center justify-end gap-2">
		<Button variant="outline" size="sm" type="button" onclick={handleCancel}>Cancel</Button>
		<Button
			type="submit"
			size="sm"
			disabled={!canSubmit}
			aria-label={isEdit ? 'Save rule' : 'Add rule'}
		>
			{isEdit ? 'Save' : 'Add rule'}
		</Button>
	</div>
</form>

<style>
	.filter-rule-form--inline {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		background: var(--surface-2);
		padding: calc(var(--edge) + 6px);
	}

	.filter-rule-form__title {
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
	}
</style>
