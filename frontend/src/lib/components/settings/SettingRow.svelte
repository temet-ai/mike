<script lang="ts">
	import { Switch } from '$lib/components/ui/switch';
	import { Select } from '$lib/components/ui/select';
	import Field from '$lib/components/ui/field/field.svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { SettingMeta, SettingValue } from '$lib/api/settings';
	import StringListControl from './StringListControl.svelte';
	import PromptEditor from './PromptEditor.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { untrack } from 'svelte';

	interface Props {
		setting: SettingMeta;
		loading?: boolean;
		disabled?: boolean;
		error?: string | null;
		onSave?: (key: string, value: SettingValue) => void;
		compact?: boolean;
	}

	let {
		setting,
		loading = false,
		disabled = false,
		error = null,
		onSave,
		compact = false
	}: Props = $props();

	let localValue = $state<SettingValue>(untrack(() => setting.value));

	$effect(() => {
		localValue = setting.value;
	});

	let saved = $state(false);
	let wasSaving = false;
	$effect(() => {
		if (loading) saved = false;
		else if (wasSaving) saved = error === null;
		wasSaving = loading;
	});

	const errorId = $derived(`setting-${setting.key}-error`);

	function handleBoolToggle(): void {
		if (!loading && !disabled) {
			const next = !(localValue as boolean);
			localValue = next;
			onSave?.(setting.key, next);
		}
	}

	function handleNumberChange(event: Event): void {
		const input = event.currentTarget as HTMLInputElement;
		if (input.value.trim() === '' && setting.default === null) {
			localValue = null;
			return;
		}
		const raw = parseFloat(input.value);
		if (!isNaN(raw)) {
			localValue = raw;
		}
	}

	// An optional number (default null) saves as null when emptied; the server then uses its own value.
	function handleNumberBlur(): void {
		if (typeof localValue === 'number' || (localValue === null && setting.default === null)) {
			if (localValue !== setting.value) onSave?.(setting.key, localValue);
		}
	}

	let jsonDraft = $state(untrack(() => JSON.stringify(setting.value ?? {})));
	let jsonError = $state<string | null>(null);
	$effect(() => {
		jsonDraft = JSON.stringify(setting.value ?? {});
		jsonError = null;
	});

	function handleJsonBlur(): void {
		let parsed: unknown;
		try {
			parsed = JSON.parse(jsonDraft);
		} catch {
			jsonError = 'Not valid JSON.';
			return;
		}
		if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
			jsonError = 'Expected a JSON object.';
			return;
		}
		jsonError = null;
		if (JSON.stringify(parsed) !== JSON.stringify(setting.value ?? {}))
			onSave?.(setting.key, parsed as Record<string, number>);
	}

	function handleStringChange(event: Event): void {
		const input = event.currentTarget as HTMLInputElement;
		localValue = input.value;
	}

	function handleStringBlur(): void {
		if (typeof localValue === 'string') {
			onSave?.(setting.key, localValue);
		}
	}

	function handleEnumChange(event: Event): void {
		const select = event.currentTarget as HTMLSelectElement;
		localValue = select.value;
		onSave?.(setting.key, select.value);
	}

	function handleListSave(items: string[]): void {
		localValue = items;
		onSave?.(setting.key, items);
	}

	function handleTextSave(text: string): void {
		localValue = text;
		onSave?.(setting.key, text);
	}

	const scopeLabel = $derived(setting.scope === 'session' ? 'session' : 'global');
	const APPLIES: Record<string, { label: string; detail: string }> = {
		on_restart: { label: 'On restart', detail: 'A change applies when mike restarts.' },
		next_cycle: {
			label: 'Next cycle',
			detail: "A change applies on the worker's next cycle, with no restart."
		},
		next_message: {
			label: 'Next message',
			detail: 'A change applies to the next message you send, with no restart.'
		},
		next_conversation: {
			label: 'Next conversation',
			detail:
				'A change applies to the next conversation you start. One already open keeps what it started with.'
		}
	};
	const applies = $derived(APPLIES[setting.applies] ?? null);

	const boolEnabled = $derived(setting.kind === 'bool' ? (localValue as boolean) : false);

	const numberStep = $derived(setting.kind === 'float' ? 'any' : '1');
	const numberMin = $derived(
		setting.kind === 'int'
			? setting.min_i64?.toString()
			: setting.kind === 'float'
				? setting.min_f64?.toString()
				: undefined
	);
	const numberMax = $derived(
		setting.kind === 'int'
			? setting.max_i64?.toString()
			: setting.kind === 'float'
				? setting.max_f64?.toString()
				: undefined
	);
</script>

<div
	class="cfg-row"
	id={`setting-${setting.key}`}
	aria-busy={loading}
	class:cfg-row--loading={loading}
	class:cfg-row--disabled={disabled && !loading}
	class:cfg-row--error={error !== null}
>
	<div class="cfg-row-body">
		<div class="cfg-row-meta">
			<div class="cfg-row-label-line">
				<span
					class="cfg-row-label"
					class:cfg-row-label--dim={setting.kind === 'bool' && !boolEnabled}
				>
					{setting.label}
				</span>
				{#if !compact}
					<Badge variant={setting.scope === 'session' ? 'tag-amber' : 'tag-blue'}
						>{scopeLabel}</Badge
					>
				{/if}
				{#if applies}
					<Tooltip text={applies.detail}>
						<Badge variant="tag-default">{applies.label}</Badge>
					</Tooltip>
				{/if}
			</div>

			<p class="cfg-row-desc">{setting.description}</p>

			<p
				class="cfg-save-status"
				class:cfg-save-status--saved={saved}
				role="status"
				aria-live="polite"
			>
				{loading
					? 'Saving changes'
					: error !== null
						? 'Changes not saved'
						: saved
							? 'Changes saved'
							: ''}
			</p>

			{#if error !== null}
				<p class="cfg-row-error" id={errorId} role="alert">{error}</p>
			{/if}
			{#if jsonError !== null}
				<p class="cfg-row-error" role="alert">{jsonError}</p>
			{/if}
		</div>

		<div class="cfg-row-control">
			{#if setting.kind === 'bool'}
				<Switch
					bind:checked={() => boolEnabled, () => handleBoolToggle()}
					aria-label="{setting.label}: {setting.description}"
					aria-describedby={error !== null ? errorId : undefined}
					disabled={loading || disabled}
				/>
				<span
					class="cfg-switch-label"
					class:cfg-switch-label--on={boolEnabled}
					class:cfg-switch-label--off={!boolEnabled}
				>
					{boolEnabled ? 'ON' : 'OFF'}
				</span>
			{:else if setting.kind === 'int' || setting.kind === 'float'}
				<Field
					type="number"
					class="cfg-control cfg-control--number"
					value={localValue === null ? '' : String(localValue)}
					step={numberStep}
					min={numberMin}
					max={numberMax}
					disabled={loading || disabled}
					aria-label={setting.label}
					aria-describedby={error !== null ? errorId : undefined}
					onchange={handleNumberChange}
					onblur={handleNumberBlur}
				/>
			{:else if setting.kind === 'string'}
				<Field
					class="cfg-control"
					value={localValue as string}
					maxlength={setting.max_len}
					disabled={loading || disabled}
					aria-label={setting.label}
					aria-describedby={error !== null ? errorId : undefined}
					onchange={handleStringChange}
					onblur={handleStringBlur}
				/>
			{:else if setting.kind === 'enum'}
				<Select
					class="cfg-control"
					value={localValue as string}
					options={(setting.enum_variants ?? []).map((variant) => ({ value: variant }))}
					disabled={loading || disabled}
					aria-label={setting.label}
					aria-describedby={error !== null ? errorId : undefined}
					onchange={handleEnumChange}
				/>
			{/if}
		</div>
	</div>

	{#if setting.kind === 'string_list'}
		<div class="cfg-row-list">
			<StringListControl
				items={localValue as string[]}
				maxLen={setting.max_len}
				disabled={loading || disabled}
				onSave={handleListSave}
			/>
		</div>
	{/if}

	{#if setting.kind === 'json'}
		<div class="cfg-row-list">
			<Field
				multiline
				rows={2}
				bind:value={jsonDraft}
				disabled={loading || disabled}
				aria-label={setting.label}
				onblur={handleJsonBlur}
			/>
		</div>
	{/if}

	{#if setting.kind === 'text'}
		<div class="cfg-row-list">
			<PromptEditor
				value={localValue as string}
				defaultValue={typeof setting.default === 'string' ? setting.default : ''}
				maxLen={setting.max_len}
				{loading}
				{disabled}
				label="{setting.group}: {setting.label}"
				onSave={handleTextSave}
			/>
		</div>
	{/if}
</div>

<style>
	.cfg-row {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 14px 20px;
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-card);
		transition: background var(--dur-1) var(--ease-snap);
		container-type: inline-size;
	}

	.cfg-row:last-child {
		border-bottom: none;
	}

	.cfg-row:hover {
		background: var(--surface-2);
	}

	.cfg-row--loading {
		pointer-events: none;
	}

	.cfg-row--disabled {
		pointer-events: none;
		cursor: not-allowed;
	}

	.cfg-row--error {
		background: var(--status-danger-bg);
		box-shadow: inset 2px 0 0 0 var(--status-danger-border);
	}

	.cfg-row-body {
		display: flex;
		align-items: flex-start;
		gap: 16px;
	}

	.cfg-row-meta {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1 1 0;
		min-width: 0;
	}

	.cfg-row-control {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
		padding-top: 2px;
	}

	@container (max-width: 460px) {
		.cfg-row-body {
			flex-direction: column;
			align-items: stretch;
			gap: 8px;
		}
		.cfg-row-control {
			padding-top: 0;
		}
		.cfg-row-control :global(.cfg-control) {
			inline-size: 100%;
		}
	}

	.cfg-row-label-line {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.cfg-row-label {
		font-family: inherit;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}

	.cfg-row-label--dim {
		color: var(--text-2);
	}

	.cfg-row-desc {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.6;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.cfg-row-error {
		margin: 4px 0 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.cfg-row-list {
		padding-top: 4px;
	}

	.cfg-save-status {
		min-height: 1.55em;
		margin: 0;
		color: var(--text-3);
		font-size: var(--text-xs);
		font-family: var(--font-machine);
	}
	.cfg-save-status--saved {
		color: var(--status-success-fg);
	}

	.cfg-row-control :global(.cfg-control) {
		inline-size: 20rem;
		max-inline-size: 100%;
	}

	.cfg-row-control :global(.cfg-control--number) {
		inline-size: 7rem;
	}
</style>
