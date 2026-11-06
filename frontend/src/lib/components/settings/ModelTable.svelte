<script lang="ts">
	import { Check, Search } from '@lucide/svelte';
	import { untrack } from 'svelte';
	import type { ModelEntry } from '$lib/types';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';

	interface Props {
		models: ModelEntry[];
		currentValue: string;
		loading?: boolean;
		saving?: boolean;
		disabled?: boolean;
		allowInherit?: boolean;
		onSelect?: (modelId: string) => void;
	}

	let {
		models = [],
		currentValue,
		loading = false,
		saving = false,
		disabled = false,
		allowInherit = false,
		onSelect
	}: Props = $props();

	let searchQuery = $state('');

	const normalizedQuery = $derived(searchQuery.trim().toLowerCase());

	function matchesQuery(m: ModelEntry): boolean {
		if (normalizedQuery === '') return true;
		return (
			m.id.toLowerCase().includes(normalizedQuery) || m.name.toLowerCase().includes(normalizedQuery)
		);
	}

	function byId(a: ModelEntry, b: ModelEntry): number {
		return a.id.localeCompare(b.id);
	}

	const currentModel = $derived(
		currentValue === '' ? undefined : models.find((m) => m.id === currentValue)
	);

	function isCurrent(m: ModelEntry): boolean {
		return m.id === currentValue && currentValue !== '';
	}

	const recommendedRows = $derived(
		models
			.filter(
				(m) => m.supports_structured_output && m.known_good && !isCurrent(m) && matchesQuery(m)
			)
			.sort(byId)
	);

	const otherRows = $derived(
		models
			.filter(
				(m) => m.supports_structured_output && !m.known_good && !isCurrent(m) && matchesQuery(m)
			)
			.sort(byId)
	);

	const incompatibleRows = $derived(
		models
			.filter((m) => !m.supports_structured_output && !isCurrent(m) && matchesQuery(m))
			.sort(byId)
	);

	const totalVisibleRows = $derived(
		recommendedRows.length + otherRows.length + incompatibleRows.length
	);

	function modelKind(m: ModelEntry): 'recommended' | 'other' | 'incompatible' {
		if (!m.supports_structured_output) return 'incompatible';
		return m.known_good ? 'recommended' : 'other';
	}

	const currentMissingFromCatalogue = $derived(
		models.length > 0 && currentValue !== '' && !models.some((m) => m.id === currentValue)
	);

	function formatPrice(price: number | null): string {
		if (price === null) return '—';
		if (price === 0) return 'free';
		return `$${price.toFixed(2)}`;
	}

	function formatContext(tokens: number | null): string {
		if (tokens === null) return '—';
		if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(0)}M`;
		if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(0)}K`;
		return String(tokens);
	}

	function handleRowClick(modelId: string, selectable: boolean) {
		if (disabled || loading || !selectable) return;
		onSelect?.(modelId);
	}

	function handleRowKeydown(e: KeyboardEvent, modelId: string, selectable: boolean) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			handleRowClick(modelId, selectable);
		}
	}

	let expandedIds = $state(new Set<string>());

	function toggleDescription(modelId: string, e: MouseEvent | KeyboardEvent) {
		e.stopPropagation();
		e.preventDefault();
		const next = new Set(expandedIds);
		if (next.has(modelId)) {
			next.delete(modelId);
		} else {
			next.add(modelId);
		}
		expandedIds = next;
	}

	function capabilityTags(model: ModelEntry): string[] {
		const tags: string[] = [];
		if (model.supports_tools) tags.push('tools');
		if (model.supports_reasoning) tags.push('reasoning');
		if (model.input_modalities?.includes('image')) tags.push('vision');
		return tags;
	}

	let seenValue = $state<string | null>(null);
	let changeSeq = $state(0);
	let justChanged = $state(false);
	let settleTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		const next = currentValue;
		untrack(() => {
			if (seenValue === null) {
				seenValue = next;
				return;
			}
			if (next === seenValue) return;
			seenValue = next;
			changeSeq += 1;
			justChanged = true;
			if (settleTimer !== null) clearTimeout(settleTimer);
			settleTimer = setTimeout(() => {
				justChanged = false;
				settleTimer = null;
			}, 2000);
		});
	});

	$effect(() => () => {
		if (settleTimer !== null) clearTimeout(settleTimer);
	});
</script>

{#snippet modelRow(
	model: ModelEntry,
	kind: 'recommended' | 'other' | 'incompatible',
	pinned: boolean
)}
	{@const selectable = kind !== 'incompatible'}
	<tr
		class="mt-row"
		class:mt-row--selected={model.id === currentValue}
		class:mt-row--pinned={pinned}
		class:mt-row--landed={pinned && justChanged}
		class:mt-row--disabled={disabled}
		class:mt-row--incompatible={!selectable}
		aria-current={model.id === currentValue ? 'true' : undefined}
		aria-disabled={!selectable ? 'true' : undefined}
		tabindex={disabled || !selectable ? -1 : 0}
		onclick={() => handleRowClick(model.id, selectable)}
		onkeydown={(e) => handleRowKeydown(e, model.id, selectable)}
	>
		<td class="mt-td mt-td--model">
			<div class="mt-model-head">
				<span class="mt-model-id">{model.id}</span>
				{#if model.description}
					<button
						type="button"
						class="mt-desc-toggle"
						aria-expanded={expandedIds.has(model.id)}
						aria-label={expandedIds.has(model.id) ? 'Hide description' : 'Show description'}
						onclick={(e) => toggleDescription(model.id, e)}
						onkeydown={(e) => e.stopPropagation()}
					>
						<DisclosureMarker open={expandedIds.has(model.id)} size={12} />
					</button>
				{/if}
			</div>
			{#if model.name && model.name !== model.id}
				<span class="mt-model-name">{model.name}</span>
			{/if}
			{#if pinned || kind !== 'other'}
				<div class="mt-badge-row">
					{#if pinned}
						<span class="mt-badge mt-badge--inuse"
							><Check size={10} aria-hidden="true" />In use</span
						>
					{/if}
					{#if kind === 'recommended'}
						<span class="mt-badge mt-badge--recommended">Recommended</span>
					{:else if kind === 'incompatible'}
						<span class="mt-badge mt-badge--incompatible">No structured output</span>
					{/if}
				</div>
			{/if}
			{#if capabilityTags(model).length > 0}
				<div class="mt-cap-tags">
					{#each capabilityTags(model) as tag (tag)}
						<span class="mt-cap-tag">{tag}</span>
					{/each}
				</div>
			{/if}
		</td>
		<td class="mt-td mt-td--context">{formatContext(model.context_length)}</td>
		<td class="mt-td mt-td--price">{formatPrice(model.prompt_price_per_1m)}</td>
		<td class="mt-td mt-td--price">{formatPrice(model.completion_price_per_1m)}</td>
		<td class="mt-td mt-td--so">
			{#if model.supports_structured_output}
				<Check size={12} class="mt-so-check" aria-hidden="true" />
				<span class="mt-sr-only">Supports structured output</span>
			{:else}
				<span aria-hidden="true">—</span>
				<span class="mt-sr-only">Does not support structured output</span>
			{/if}
		</td>
	</tr>
	{#if model.description && expandedIds.has(model.id)}
		<tr class="mt-desc-row">
			<td class="mt-desc-cell" colspan="5">{model.description}</td>
		</tr>
	{/if}
{/snippet}

<div
	class="mt-tbl-wrap"
	class:mt-tbl-wrap--disabled={disabled}
	role="group"
	aria-label="Model selection"
	aria-disabled={disabled}
>
	{#if !loading}
		<div class="mt-now" class:mt-now--landed={justChanged}>
			{#key changeSeq}
				<span class="mt-now__sweep" aria-hidden="true"></span>
			{/key}
			<div class="mt-now__body" aria-live="polite">
				<span class="mt-now__label">Model in use</span>
				{#if currentValue === ''}
					<span class="mt-now__id mt-now__id--unset">
						{allowInherit
							? 'Inheriting the fleet default model'
							: 'None chosen yet — pick one from the list below'}
					</span>
				{:else}
					<span class="mt-now__id">{currentValue}</span>
					{#if currentModel}
						{#if currentModel.name && currentModel.name !== currentModel.id}
							<span class="mt-now__name">{currentModel.name}</span>
						{/if}
						<span class="mt-now__facts">
							{formatContext(currentModel.context_length)} context · {formatPrice(
								currentModel.prompt_price_per_1m
							)} in · {formatPrice(currentModel.completion_price_per_1m)} out
							<span class="mt-now__unit">per 1M tokens</span>
						</span>
					{:else if models.length > 0}
						<span class="mt-now__missing">
							No longer in the OpenRouter catalogue — pick a replacement below.
						</span>
					{/if}
				{/if}
			</div>
			<div class="mt-now__state">
				{#if saving}
					<span class="mt-now__saving">Saving…</span>
				{:else if justChanged}
					{#key changeSeq}
						<span class="mt-now__saved"><Check size={12} aria-hidden="true" />Saved</span>
					{/key}
				{/if}
			</div>
		</div>
	{/if}

	{#if loading}
		<div class="mt-loading" aria-busy="true">
			<span class="mt-loading-dot" aria-hidden="true"></span>
			Loading models…
		</div>
	{:else if models.length === 0 && !allowInherit}
		<div class="mt-empty">
			<p class="mt-empty-msg">
				No models available. Verify your OpenRouter key to load the model list.
			</p>
		</div>
	{:else}
		{#if models.length > 0}
			<div class="mt-search">
				<Search size={13} class="mt-search-icon" aria-hidden="true" />
				<input
					type="text"
					bind:value={searchQuery}
					placeholder="Filter models by id or name…"
					aria-label="Filter models"
					autocomplete="off"
					autocapitalize="off"
					spellcheck="false"
					disabled={disabled || loading}
				/>
			</div>
		{/if}

		{#if totalVisibleRows === 0 && !allowInherit && !currentMissingFromCatalogue && !currentModel}
			<div class="mt-empty">
				<p class="mt-empty-msg">No models match &ldquo;{searchQuery}&rdquo;.</p>
			</div>
		{:else}
			<div class="mt-scroll">
				<table class="mt-table">
					<thead>
						<tr>
							<th class="mt-th mt-th--model" scope="col">Model</th>
							<th class="mt-th mt-th--context" scope="col">Context</th>
							<th class="mt-th mt-th--price" scope="col">Input $/1M</th>
							<th class="mt-th mt-th--price" scope="col">Output $/1M</th>
							<th class="mt-th mt-th--so" scope="col">SO</th>
						</tr>
					</thead>
					<tbody>
						{#if allowInherit}
							<tr
								class="mt-row"
								class:mt-row--selected={currentValue === ''}
								class:mt-row--disabled={disabled}
								aria-current={currentValue === '' ? 'true' : undefined}
								tabindex={disabled ? -1 : 0}
								onclick={() => handleRowClick('', true)}
								onkeydown={(e) => handleRowKeydown(e, '', true)}
							>
								<td class="mt-td mt-td--model" colspan="5">
									<span class="mt-inherit-label">(inherit default model)</span>
								</td>
							</tr>
						{/if}

						{#if currentModel}
							<tr class="mt-group-row mt-group-row--current">
								<td class="mt-group-label mt-group-label--inuse" colspan="5">In use</td>
							</tr>
							{@render modelRow(currentModel, modelKind(currentModel), true)}
						{/if}

						{#if recommendedRows.length > 0}
							<tr class="mt-group-row">
								<td class="mt-group-label" colspan="5">Recommended</td>
							</tr>
							{#each recommendedRows as model (model.id)}
								{@render modelRow(model, 'recommended', false)}
							{/each}
						{/if}

						{#if otherRows.length > 0}
							<tr class="mt-group-row">
								<td class="mt-group-label" colspan="5">Other models</td>
							</tr>
							{#each otherRows as model (model.id)}
								{@render modelRow(model, 'other', false)}
							{/each}
						{/if}

						{#if incompatibleRows.length > 0}
							<tr class="mt-group-row">
								<td class="mt-group-label" colspan="5">Not compatible</td>
							</tr>
							{#each incompatibleRows as model (model.id)}
								{@render modelRow(model, 'incompatible', false)}
							{/each}
						{/if}

						{#if totalVisibleRows === 0 && currentModel}
							<tr class="mt-no-match-row">
								<td class="mt-no-match-cell" colspan="5">
									No other models match &ldquo;{searchQuery}&rdquo;.
								</td>
							</tr>
						{/if}

						{#if currentMissingFromCatalogue}
							<tr
								class="mt-row mt-row--fallback"
								class:mt-row--selected={true}
								class:mt-row--disabled={disabled}
								aria-current="true"
								tabindex={disabled ? -1 : 0}
								onclick={() => handleRowClick(currentValue, true)}
								onkeydown={(e) => handleRowKeydown(e, currentValue, true)}
							>
								<td class="mt-td mt-td--model" colspan="5">
									<span class="mt-model-id">{currentValue}</span>
									<span class="mt-badge mt-badge--fallback"
										>(current — not found in model list)</span
									>
								</td>
							</tr>
						{/if}
					</tbody>
				</table>
			</div>
		{/if}
	{/if}
</div>

<style>
	.mt-sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}

	.mt-tbl-wrap {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--surface-card);
		overflow: hidden;
	}

	.mt-tbl-wrap--disabled {
		opacity: 0.45;
		pointer-events: none;
	}

	.mt-now {
		position: relative;
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 10px 12px;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
		overflow: hidden;
		flex-shrink: 0;
	}

	.mt-now__body {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1 1 auto;
		min-width: 0;
	}

	.mt-now__label {
		font-family: inherit;
		font-size: var(--text-label, 10px);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider, 0.06em);
		color: var(--text-3);
	}

	.mt-now__id {
		font-family: inherit;
		font-size: 13px;
		font-weight: 600;
		color: var(--interactive-active-fg);
		word-break: break-all;
		overflow-wrap: anywhere;
	}

	.mt-now__id--unset {
		font-weight: 400;
		color: var(--text-2);
	}

	.mt-now__name {
		font-family: inherit;
		font-size: 11px;
		color: var(--text-2);
	}

	.mt-now__facts {
		font-family: inherit;
		font-size: 11px;
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}

	.mt-now__unit {
		color: var(--text-3);
	}

	.mt-now__missing {
		font-family: inherit;
		font-size: 11px;
		color: var(--status-warning-fg);
	}

	.mt-now__state {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		flex-shrink: 0;
		width: 76px;
		min-height: 20px;
	}

	.mt-now__saving {
		font-family: inherit;
		font-size: 11px;
		color: var(--text-3);
	}

	.mt-now__saved {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		border-radius: 3px;
		font-family: inherit;
		font-size: 11px;
		font-weight: 600;
		white-space: nowrap;
		color: var(--status-success-fg);
		background: var(--status-success-bg);
		border: 1px solid var(--status-success-border);
	}

	.mt-now__sweep {
		position: absolute;
		inset: 0;
		opacity: 0;
		pointer-events: none;
		background: linear-gradient(
			90deg,
			transparent 0%,
			var(--status-success-bg) 45%,
			var(--status-success-border) 55%,
			transparent 100%
		);
	}

	.mt-now--landed .mt-now__sweep {
		animation: mt-sweep 620ms var(--ease-io, ease) 1 both;
	}

	.mt-now--landed .mt-now__id {
		animation: mt-rise 320ms var(--ease-snap, ease-out) 1 both;
	}

	.mt-now__saved {
		animation: mt-chip 2000ms var(--ease-snap, ease-out) 1 both;
	}

	@keyframes mt-sweep {
		0% {
			opacity: 0.9;
			transform: translateX(-100%);
		}
		100% {
			opacity: 0;
			transform: translateX(100%);
		}
	}

	@keyframes mt-rise {
		0% {
			opacity: 0.25;
			transform: translateY(4px);
		}
		100% {
			opacity: 1;
			transform: translateY(0);
		}
	}

	@keyframes mt-chip {
		0% {
			opacity: 0;
			transform: translateX(8px);
		}
		12% {
			opacity: 1;
			transform: translateX(0);
		}
		80% {
			opacity: 1;
			transform: translateX(0);
		}
		100% {
			opacity: 0;
			transform: translateX(8px);
		}
	}

	.mt-search {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-card);
		flex-shrink: 0;
	}

	.mt-search :global(.mt-search-icon) {
		color: var(--text-3);
		flex-shrink: 0;
	}

	.mt-search input {
		flex: 1 1 auto;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-strong);
		padding: 0;
	}

	.mt-search input::placeholder {
		color: var(--text-3);
	}

	.mt-search input:disabled {
		cursor: not-allowed;
	}

	.mt-scroll {
		overflow-x: auto;
		overflow-y: auto;
		max-height: 360px;
		width: 100%;
	}

	.mt-table {
		width: 100%;
		border-collapse: collapse;
		table-layout: auto;
		font-family: inherit;
		font-size: 12px;
	}

	.mt-th {
		padding: 7px 12px;
		text-align: left;
		font-family: inherit;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-2);
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
		user-select: none;
		position: sticky;
		top: 0;
	}

	.mt-th--model {
		min-width: 200px;
	}
	.mt-th--context {
		min-width: 80px;
		text-align: right;
	}
	.mt-th--price {
		min-width: 90px;
		text-align: right;
	}
	.mt-th--so {
		min-width: 40px;
		text-align: center;
	}

	.mt-group-row {
		border-bottom: 1px solid var(--border-subtle);
	}

	.mt-group-label {
		padding: 5px 12px;
		font-family: inherit;
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		color: var(--text-3);
		background: var(--surface-2);
	}

	.mt-row {
		border-bottom: 1px solid var(--border-subtle);
		cursor: pointer;
		transition: background 80ms ease-out;
		outline: none;
	}

	.mt-row:last-child {
		border-bottom: none;
	}

	.mt-row:hover:not(.mt-row--disabled):not(.mt-row--incompatible) {
		background: var(--surface-2);
	}

	.mt-row:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.mt-row--selected {
		background: var(--interactive-active-bg);
	}

	.mt-row--pinned {
		background: var(--interactive-active-bg);
		box-shadow: inset 3px 0 0 0 var(--interactive-active-border);
	}

	.mt-row--pinned:hover:not(.mt-row--disabled) {
		background: var(--interactive-active-bg);
	}

	.mt-row--landed {
		background-image: linear-gradient(
			90deg,
			transparent 0%,
			var(--status-success-bg) 45%,
			var(--status-success-border) 55%,
			transparent 100%
		);
		background-size: 220% 100%;
		background-repeat: no-repeat;
		animation: mt-row-sweep 620ms var(--ease-io, ease) 1 both;
	}

	@keyframes mt-row-sweep {
		0% {
			background-position: -120% 0;
		}
		100% {
			background-position: 220% 0;
		}
	}

	.mt-row--selected:hover:not(.mt-row--disabled):not(.mt-row--incompatible) {
		background: var(--interactive-active-bg);
	}

	.mt-row--disabled {
		cursor: not-allowed;
	}

	.mt-row--incompatible {
		cursor: not-allowed;
		opacity: 0.6;
	}

	.mt-row--fallback {
		border-top: 1px dashed var(--border-subtle);
	}

	.mt-td {
		padding: 9px 12px;
		color: var(--text-1);
		vertical-align: top;
		word-break: break-all;
		overflow-wrap: anywhere;
	}

	.mt-td--context,
	.mt-td--price {
		text-align: right;
		white-space: nowrap;
		color: var(--text-2);
		font-variant-numeric: tabular-nums;
	}

	.mt-td--so {
		text-align: center;
		color: var(--text-3);
	}

	.mt-td--model {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.mt-model-head {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.mt-desc-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: 16px;
		height: 16px;
		padding: 0;
		border: none;
		border-radius: 3px;
		background: transparent;
		color: var(--text-3);
		cursor: pointer;
	}

	.mt-desc-toggle:hover {
		color: var(--text-strong);
	}

	.mt-desc-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.mt-model-id {
		font-family: inherit;
		font-size: 12px;
		color: var(--text-strong);
		word-break: break-all;
		overflow-wrap: anywhere;
	}

	.mt-row--selected .mt-model-id {
		color: var(--interactive-active-fg);
	}

	.mt-model-name {
		font-family: inherit;
		font-size: 11px;
		color: var(--text-2);
		word-break: break-word;
		overflow-wrap: break-word;
	}

	.mt-group-label--inuse {
		color: var(--interactive-active-fg);
	}

	.mt-no-match-cell {
		padding: 8px 12px;
		font-family: inherit;
		font-size: 11px;
		color: var(--text-3);
	}

	.mt-badge-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px;
		align-self: flex-start;
		margin-top: 2px;
	}

	.mt-badge-row .mt-badge {
		margin-top: 0;
	}

	.mt-badge {
		display: inline-block;
		align-self: flex-start;
		margin-top: 2px;
		padding: 1px 5px;
		border-radius: 3px;
		font-size: 10px;
		border: 1px solid transparent;
		white-space: nowrap;
	}

	.mt-badge--recommended {
		color: var(--status-success-fg);
		background: var(--status-success-bg);
		border-color: var(--status-success-border);
	}

	.mt-badge--incompatible {
		color: var(--status-muted-fg);
		background: var(--status-muted-bg);
		border-color: var(--status-muted-border);
	}

	.mt-badge--inuse {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-weight: 600;
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg);
		border-color: var(--interactive-active-border);
	}

	.mt-badge--fallback {
		color: var(--status-warning-fg);
		background: var(--status-warning-bg);
		border-color: var(--status-warning-border);
	}

	.mt-cap-tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-top: 2px;
	}

	.mt-cap-tag {
		display: inline-block;
		padding: 1px 5px;
		border-radius: 3px;
		font-size: 10px;
		color: var(--text-3);
		background: transparent;
		border: 1px solid var(--border-subtle);
		white-space: nowrap;
	}

	.mt-desc-row {
		border-bottom: 1px solid var(--border-subtle);
	}

	.mt-desc-cell {
		padding: 8px 12px 10px 12px;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: inherit;
		font-size: 11px;
		line-height: 1.5;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.mt-inherit-label {
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
		font-style: italic;
	}

	.mt-row--selected .mt-inherit-label {
		color: var(--interactive-active-fg);
		font-style: normal;
	}

	:global(.mt-so-check) {
		color: var(--status-success-fg);
		display: inline-block;
		vertical-align: middle;
	}

	.mt-loading {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 14px 16px;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-3);
	}

	.mt-loading-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--text-3);
		flex-shrink: 0;
	}

	.mt-empty {
		padding: 16px 20px;
	}

	.mt-empty-msg {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.6;
		overflow-wrap: break-word;
		word-break: break-word;
	}
</style>
