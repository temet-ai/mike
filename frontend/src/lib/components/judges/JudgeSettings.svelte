<!--
@component
A judge's settings, one `SettingRow` per key, worded in plain language. While a configuration is in
use it supplies the model and instructions, so those two sit behind a disclosure that says they are
not used; with no configuration in use they are ordinary rows at the top of the list.
-->
<script lang="ts">
	import type { SettingMeta, SettingValue } from '$lib/api/settings';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import ModelPickerPopover from '$lib/components/settings/ModelPickerPopover.svelte';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { SETTING_TEXT } from './judges';

	interface Props {
		keys: string[];
		modelKey: string;
		promptKey: string;
		settings: SettingMeta[];
		liveId: number;
		loading: Record<string, boolean>;
		errors: Record<string, string | null>;
		disabled?: boolean;
		inheritLabel?: string | null;
		onSave: (key: string, value: SettingValue) => void;
	}

	let {
		keys,
		modelKey,
		promptKey,
		settings,
		liveId,
		loading,
		errors,
		disabled = false,
		inheritLabel = null,
		onSave
	}: Props = $props();

	let open = $state(false);

	const byKey = $derived(new Map(settings.map((s) => [s.key, s])));
	const modelValue = $derived.by(() => {
		const v = byKey.get(modelKey)?.value;
		return typeof v === 'string' ? v : '';
	});

	function worded(s: SettingMeta): SettingMeta {
		const text = SETTING_TEXT[s.key];
		return text
			? { ...s, label: text.label ?? s.label, description: text.description ?? s.description }
			: s;
	}
</script>

{#snippet row(key: string)}
	{@const s = byKey.get(key)}
	{#if s}
		<SettingRow
			setting={worded(s)}
			loading={loading[key] ?? false}
			error={errors[key] ?? null}
			{disabled}
			onSave={(k, v) => onSave(k, v)}
			compact
		/>
	{/if}
{/snippet}

{#snippet modelAndPrompt()}
	<div class="jst-model">
		<span class="machine jst-k">Model</span>
		{#if SETTING_TEXT[modelKey]?.description}<p class="jst-why">
				{SETTING_TEXT[modelKey]?.description}
			</p>{/if}
		<div class="jst-pick">
			<span class="jst-current"
				>{modelValue || (inheritLabel ? `Inherited: ${inheritLabel}` : 'Not set')}</span
			>
			<ModelPickerPopover
				currentValue={modelValue}
				allowInherit={inheritLabel !== null}
				inheritLabel={inheritLabel ? `(inherit ${inheritLabel})` : undefined}
				{disabled}
				triggerLabel="Choose the model"
				onSelect={(id) => onSave(modelKey, id)}
			/>
		</div>
		{#if errors[modelKey]}<p class="jst-err" role="alert">
				Could not save the model: {errors[modelKey]}
			</p>{/if}
	</div>
	<div class="jst-list">{@render row(promptKey)}</div>
{/snippet}

<section class="jst edge mech-group" aria-label="Settings" data-testid="judge-settings">
	<div class="band-hd band-hd--static">
		<h2 class="machine jst-title">Settings</h2>
	</div>
	<div class="band-body jst-body">
		<p class="jst-why" data-testid="settings-note">
			Each setting applies to the whole install, not to one session. A change marked Next cycle
			takes effect the next time the judge runs, with no restart; one marked On restart waits for
			mike to restart.
		</p>
		{#if liveId === 0}{@render modelAndPrompt()}{/if}
		<div class="jst-list">
			{#each keys as key (key)}{@render row(key)}{/each}
		</div>
		{#if liveId > 0}
			<button type="button" class="jst-toggle" aria-expanded={open} onclick={() => (open = !open)}>
				<DisclosureMarker {open} size={10} /><span
					>Not used while configuration {liveId} is in use: model, instructions</span
				>
			</button>
			{#if open}
				<div class="jst-unused" data-testid="not-used">{@render modelAndPrompt()}</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.jst {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.jst-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--text-1);
	}
	.jst-body {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.jst-list {
		display: flex;
		flex-direction: column;
		border: var(--hairline) solid var(--border-default);
		background: var(--surface-card);
	}
	.jst-model {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.jst-k {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.jst-why {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.jst-pick {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.jst-current {
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.jst-err {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.jst-unused {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}
	.jst-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-sm);
		color: var(--text-1);
		cursor: pointer;
	}
	.jst-toggle:hover {
		color: var(--interactive-active-fg);
	}
	.jst-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
