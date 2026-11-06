<script lang="ts">
	import Button from '$lib/components/ui/button/button.svelte';
	import { ChevronDown, SearchX } from '@lucide/svelte';
	import * as Popover from '$lib/components/ui/popover';
	import { listModels } from '$lib/api/steering';
	import type { ModelEntry } from '$lib/types';

	interface Props {
		currentValue: string;
		onSelect: (modelId: string) => void;
		allowInherit?: boolean;
		inheritLabel?: string;
		presetModelIds?: string[];
		disabled?: boolean;
		triggerLabel?: string;
	}

	let {
		currentValue,
		onSelect,
		allowInherit = false,
		inheritLabel = '(inherit default model)',
		presetModelIds = [],
		disabled = false,
		triggerLabel = 'Select model'
	}: Props = $props();

	let open = $state(false);
	let models = $state<ModelEntry[] | null>(null);
	let loading = $state(false);
	let loadFailed = $state(false);
	let search = $state('');
	let sortCol = $state<'name' | 'context' | 'in' | 'out'>('in');
	let sortAsc = $state(true);

	const soModels = $derived(models?.filter((m) => m.supports_structured_output) ?? []);

	const currentModel = $derived(
		currentValue === '' ? undefined : soModels.find((m) => m.id === currentValue)
	);

	const presetModels = $derived(
		presetModelIds
			.map((id) => soModels.find((m) => m.id === id))
			.filter((m): m is ModelEntry => m !== undefined && m.id !== currentValue)
	);

	const filteredModels = $derived(
		(() => {
			const q = search.trim().toLowerCase();
			const catalogue = soModels.filter((m) => m.id !== currentValue);
			const filtered = q
				? catalogue.filter(
						(m) => m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q)
					)
				: catalogue;
			return [...filtered].sort((a, b) => {
				if (a.known_good !== b.known_good) return a.known_good ? -1 : 1;
				let cmp = 0;
				if (sortCol === 'name') cmp = a.id.localeCompare(b.id);
				else if (sortCol === 'context') cmp = (a.context_length ?? 0) - (b.context_length ?? 0);
				else if (sortCol === 'in')
					cmp = (a.prompt_price_per_1m ?? 0) - (b.prompt_price_per_1m ?? 0);
				else if (sortCol === 'out')
					cmp = (a.completion_price_per_1m ?? 0) - (b.completion_price_per_1m ?? 0);
				return sortAsc ? cmp : -cmp;
			});
		})()
	);

	async function loadModels(): Promise<void> {
		if (loading) return;
		loading = true;
		loadFailed = false;
		try {
			const response = await listModels();
			if (response === null) throw new Error('Model catalogue unavailable');
			models = response.models;
		} catch {
			loadFailed = true;
		} finally {
			loading = false;
		}
	}

	function handleOpenChange(next: boolean): void {
		open = next;
		if (next && models === null) void loadModels();
	}

	function pick(id: string): void {
		onSelect(id);
		open = false;
	}

	function sortBy(col: typeof sortCol): void {
		sortAsc = sortCol !== col || !sortAsc;
		sortCol = col;
	}

	function formatCost(v: number | null): string {
		if (v === null) return '—';
		if (v === 0) return 'free';
		return `$${v.toFixed(2)}`;
	}

	function formatContext(v: number | null): string {
		if (v === null) return '—';
		if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(0)}M`;
		if (v >= 1_000) return `${(v / 1_000).toFixed(0)}K`;
		return String(v);
	}
</script>

<Popover.Root {open} onOpenChange={handleOpenChange}>
	<Popover.Trigger>
		{#snippet child({ props })}
			<Button {...props} variant="outline" size="sm" aria-label={triggerLabel} {disabled}>
				{triggerLabel}
				<ChevronDown size={12} aria-hidden="true" />
			</Button>
		{/snippet}
	</Popover.Trigger>
	<Popover.Content class="mpp-popover" side="bottom" align="end">
		{#if loadFailed}
			<div class="mpp-error" role="alert">
				<p>Could not load the model list from OpenRouter.</p>
				<Button variant="outline" size="sm" onclick={() => void loadModels()} disabled={loading}
					>Try again</Button
				>
			</div>
		{:else}
			<div class="mpp-search-wrap">
				<input
					class="mpp-search"
					type="search"
					placeholder="Filter by name or provider…"
					aria-label="Filter models"
					bind:value={search}
				/>
			</div>

			{#if currentModel}
				<div class="mpp-section-label mpp-section-label--inuse">In use</div>
				<button
					type="button"
					class="mpp-row mpp-row--selected mpp-row--pinned"
					onclick={() => pick(currentModel.id)}
				>
					<span class="mpp-name">{currentModel.id}</span>
					<span class="mpp-ctx">{formatContext(currentModel.context_length)}</span>
					<span class="mpp-cost">{formatCost(currentModel.prompt_price_per_1m)}</span>
					<span class="mpp-cost">{formatCost(currentModel.completion_price_per_1m)}</span>
				</button>
				<div class="mpp-divider"></div>
			{/if}

			{#if allowInherit}
				<button
					type="button"
					class="mpp-row mpp-row--inherit"
					class:mpp-row--selected={currentValue === ''}
					class:mpp-row--pinned={currentValue === ''}
					onclick={() => pick('')}
				>
					<span class="mpp-name">{inheritLabel}</span>
					{#if currentValue === ''}<span class="mpp-inuse">in use</span>{/if}
				</button>
				<div class="mpp-divider"></div>
			{/if}

			{#if presetModels.length > 0 && !search}
				<div class="mpp-section-label">Common for steering</div>
				{#each presetModels as m (m.id)}
					<button
						type="button"
						class="mpp-row"
						class:mpp-row--selected={currentValue === m.id}
						onclick={() => pick(m.id)}
					>
						<span class="mpp-name">{m.id}</span>
						<span class="mpp-ctx">{formatContext(m.context_length)}</span>
						<span class="mpp-cost">{formatCost(m.prompt_price_per_1m)}</span>
						<span class="mpp-cost">{formatCost(m.completion_price_per_1m)}</span>
					</button>
				{/each}
				<div class="mpp-divider"></div>
			{/if}

			<div class="mpp-thead">
				<button type="button" class="mpp-th mpp-th--name" onclick={() => sortBy('name')}
					>Model</button
				>
				<button type="button" class="mpp-th" onclick={() => sortBy('context')}>Context</button>
				<button type="button" class="mpp-th" onclick={() => sortBy('in')}>$/M in</button>
				<button type="button" class="mpp-th" onclick={() => sortBy('out')}>$/M out</button>
			</div>

			{#if loading}
				<div class="mpp-loading" aria-busy="true">
					<span class="mpp-loading-dot" aria-hidden="true"></span>
					Loading models…
				</div>
			{:else if filteredModels.length === 0}
				<div class="mpp-empty">
					<SearchX size={16} aria-hidden="true" />
					<span
						>{search
							? `No models match “${search}”.`
							: 'No additional structured-output models are available.'}</span
					>
					{#if search}<Button
							variant="ghost"
							size="sm"
							onclick={() => {
								search = '';
							}}>Clear search</Button
						>{/if}
				</div>
			{:else}
				<div class="mpp-list">
					{#each filteredModels as m (m.id)}
						<button
							type="button"
							class="mpp-row"
							class:mpp-row--selected={currentValue === m.id}
							onclick={() => pick(m.id)}
						>
							<span class="mpp-name">{m.id}</span>
							<span class="mpp-ctx">{formatContext(m.context_length)}</span>
							<span class="mpp-cost">{formatCost(m.prompt_price_per_1m)}</span>
							<span class="mpp-cost">{formatCost(m.completion_price_per_1m)}</span>
						</button>
					{/each}
				</div>
			{/if}
		{/if}
	</Popover.Content>
</Popover.Root>

<style>
	:global(.mpp-popover) {
		width: 480px;
		max-width: calc(100dvw - var(--space-6));
		max-height: min(32rem, var(--bits-popover-content-available-height));
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		padding: 0;
	}

	.mpp-search-wrap {
		padding: 8px;
		border-bottom: 1px solid var(--border-subtle);
		flex-shrink: 0;
	}
	.mpp-search {
		width: 100%;
		padding: 5px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-tol);
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		box-sizing: border-box;
	}
	.mpp-search:focus {
		outline: 1px solid var(--status-steering-border);
		border-color: var(--status-steering-border);
	}

	.mpp-section-label {
		padding: 6px 10px 2px;
		font-family: inherit;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		flex-shrink: 0;
	}
	.mpp-divider {
		height: 1px;
		background: var(--border-subtle);
		margin: 4px 0;
		flex-shrink: 0;
	}

	.mpp-thead {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 4rem 3.5rem 3.5rem;
		padding: 4px 10px;
		flex-shrink: 0;
	}
	.mpp-th {
		font-family: inherit;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		color: var(--text-3);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		text-align: left;
	}
	.mpp-th:hover {
		color: var(--text-1);
	}

	.mpp-list {
		min-height: 0;
		overscroll-behavior: contain;
		overflow-y: auto;
		flex: 1 1 auto;
	}

	.mpp-row {
		min-height: var(--control-h);
		align-items: center;
		display: grid;
		grid-template-columns: minmax(0, 1fr) 4rem 3.5rem 3.5rem;
		width: 100%;
		padding: 6px 10px;
		background: none;
		border: none;
		cursor: pointer;
		font-family: inherit;
		font-size: 11px;
		text-align: left;
		color: var(--text-1);
	}
	.mpp-row:hover {
		background: var(--surface-2);
	}
	.mpp-row:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.mpp-row--selected {
		background: var(--interactive-active-bg);
		color: var(--interactive-active-fg);
	}

	.mpp-row--pinned {
		box-shadow: inset 3px 0 0 0 var(--interactive-active-border);
		font-weight: 600;
	}

	.mpp-section-label--inuse {
		color: var(--interactive-active-fg);
	}

	.mpp-inuse {
		justify-self: end;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--interactive-active-fg);
	}
	.mpp-row--inherit {
		grid-template-columns: 1fr auto;
		font-style: italic;
		color: var(--text-2);
	}
	.mpp-row--inherit.mpp-row--selected {
		font-style: normal;
	}

	.mpp-name {
		overflow-wrap: anywhere;
		white-space: normal;
	}
	.mpp-ctx,
	.mpp-cost {
		color: var(--text-2);
	}
	.mpp-row--selected .mpp-ctx,
	.mpp-row--selected .mpp-cost {
		color: inherit;
	}

	.mpp-loading {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 14px 10px;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-3);
	}
	.mpp-loading-dot {
		width: 6px;
		height: 6px;
		border-radius: var(--r-tol);
		background: var(--text-3);
		flex-shrink: 0;
	}

	.mpp-empty {
		flex-wrap: wrap;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 16px 10px;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-3);
	}

	.mpp-error {
		margin: 0;
		padding: 12px 10px;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
	}
</style>
