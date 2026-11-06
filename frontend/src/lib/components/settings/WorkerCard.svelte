<script lang="ts">
	import { Switch } from '$lib/components/ui/switch';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import Badge from '$lib/components/shared/Badge.svelte';
	import { toggleAppliesNote } from '$lib/api/settings';
	import type { Snippet } from 'svelte';
	import SectionCard, { type SectionCardAccent } from '$lib/components/detail/SectionCard.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Info } from '@lucide/svelte';

	export interface WorkerModel {
		id: string;
		label?: string;
		released?: string;
		pricing?: string;
		context?: string;
		description?: string;
	}

	export interface WorkerConfigItem {
		label: string;
		value?: string;
		help?: string;
	}

	export type WorkerCardAccent = SectionCardAccent;

	interface Props {
		name: string;
		label: string;
		description?: string;
		enabled?: boolean;
		onToggle?: (value: boolean) => void;
		disabled?: boolean;
		models?: WorkerModel[];
		selectedModelId?: string;
		onSelectModel?: (id: string) => void;
		config?: WorkerConfigItem[];
		children?: Snippet;
		accent?: WorkerCardAccent;
		error?: string | null;
		applies?: string | null;
	}

	let {
		name,
		label,
		description,
		enabled = true,
		onToggle,
		disabled = false,
		models = [],
		selectedModelId,
		onSelectModel,
		config = [],
		children: extraContent,
		accent = 'info',
		error = null,
		applies
	}: Props = $props();

	const hasBody = $derived(models.length > 0 || config.length > 0 || extraContent !== undefined);
	const expandable = $derived(enabled && hasBody);
	const sectionKey = $derived(`settings:worker:${name}`);

	function toggleEnabled(): void {
		if (!disabled) onToggle?.(!enabled);
	}

	const appliesNote = $derived(applies ? toggleAppliesNote(applies) : null);
</script>

<div data-reveal>
	<SectionCard
		title={label}
		{description}
		{accent}
		collapsible={hasBody}
		{expandable}
		{sectionKey}
		defaultCollapsed={false}
	>
		{#snippet headerActions()}
			<div class="wc-actions">
				{#if error !== null}
					<p class="wc__error" role="alert">{error}</p>
				{/if}
				{#if appliesNote}
					<Tooltip text={appliesNote.detail}>
						<Badge variant="tag-default">{appliesNote.label}</Badge>
					</Tooltip>
				{/if}
				<Switch
					bind:checked={() => enabled, () => toggleEnabled()}
					aria-label="Enable {label}"
					{disabled}
				/>
			</div>
		{/snippet}
		{#snippet children()}
			<div class="wc-body">
				{#if models.length > 0}
					<div class="wc-models">
						<span class="wc-models-label">Model</span>
						<div class="edge wc-models-frame">
							<RadioGroup.Root
								name={`${name}-model`}
								value={selectedModelId}
								aria-label="{label} model"
								onValueChange={(id) => onSelectModel?.(id)}
							>
								{#each models as m (m.id)}
									{@const meta = [m.released, m.pricing, m.context].filter(Boolean).join('  ·  ')}
									<RadioGroup.Item
										value={m.id}
										label={m.label ?? m.id}
										description={[meta, m.description].filter(Boolean).join(' — ') || undefined}
									/>
								{/each}
							</RadioGroup.Root>
						</div>
					</div>
				{/if}

				{#if config.length > 0}
					<div class="wc-config">
						{#each config as c (c.label)}
							<div class="wc-cfg">
								<span class="wc-cfg-label">{c.label}</span>
								{#if c.help}
									<Tooltip text={c.help}>
										{#snippet child({ props })}
											<button
												{...props}
												type="button"
												class="wc-cfg-help"
												aria-label={`About ${c.label}`}
												><Info size={13} aria-hidden="true" /></button
											>
										{/snippet}
									</Tooltip>
								{/if}
								{#if c.value !== undefined}<code class="wc-cfg-value">{c.value}</code>{/if}
							</div>
						{/each}
					</div>
				{/if}

				{#if extraContent}
					<div class="wc-config">
						{@render extraContent()}
					</div>
				{/if}
			</div>
		{/snippet}
	</SectionCard>
</div>

<style>
	.wc-actions {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.wc__error {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.wc-body {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.wc-models {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.wc-models-label {
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-strong);
	}

	.wc-config {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.wc-cfg {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.wc-cfg-label {
		font-family: inherit;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
	}
	.wc-cfg-help {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px;
		border: none;
		background: transparent;
		color: var(--text-3);
		cursor: help;
		border-radius: var(--r-tol);
	}
	.wc-cfg-help:hover {
		color: var(--text-1);
	}
	.wc-cfg-help:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}
	.wc-cfg-value {
		margin-left: auto;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-1);
		background: var(--surface-2);
		border: 1px solid var(--border-default);
		border-radius: var(--r-tol);
		padding: 3px 8px;
		white-space: normal;
		overflow-wrap: anywhere;
	}

	.wc-models-frame {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-bg);
	}
</style>
