<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import {
		ApiError,
		exportPromptCompositions,
		getCompositionRevisions,
		getPromptClassMain,
		getPromptClassSubagent,
		importPromptCompositions,
		listPromptAnomalies,
		listPromptClasses,
		UNRESOLVED_PERSONA_SEGMENT,
		type PromptCompositionExport,
		type PromptCompositionImportResult
	} from '$lib/api';
	import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
	import PromptKpiStrip from '$lib/components/prompts/PromptKpiStrip.svelte';
	import PromptCatalogueRow from '$lib/components/prompts/PromptCatalogueRow.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import FilterScopeStatus from '$lib/components/shared/FilterScopeStatus.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import { Download, ListFilter, SearchX, Upload } from '@lucide/svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import {
		classifyAnomalyRows,
		classifyPromptClass,
		compareCatalogueRows,
		hasBlock2Drift,
		type PromptCatalogueRowViewModel,
		type PromptRowState
	} from '$lib/prompts/catalogueRow';
	import type { PromptAnomalyGroup, PromptClassDto, PromptModelBreakdown } from '$lib/types';

	function openHelp(): void {
		shellState.helpDrawerOpen = true;
	}

	type WindowOption = '24h' | '7d' | '30d';
	const WINDOW_MS: Record<WindowOption, number> = {
		'24h': 24 * 60 * 60 * 1000,
		'7d': 7 * 24 * 60 * 60 * 1000,
		'30d': 30 * 24 * 60 * 60 * 1000
	};
	let windowOption = $state<WindowOption>('7d');
	const WINDOW_OPTIONS: readonly WindowOption[] = ['24h', '7d', '30d'];

	type FilterTier = 'all' | PromptRowState;
	let filterTier = $state<FilterTier>('all');

	let now = $state(Date.now());
	let clockTimer: ReturnType<typeof setInterval> | null = null;

	let classes = $state<PromptClassDto[]>([]);
	let anomalies = $state<PromptAnomalyGroup[]>([]);
	let driftedKeys = $state<Set<string>>(new Set());
	let modelBreakdownByKey = $state<Map<string, PromptModelBreakdown[]>>(new Map());
	let loading = $state(true);
	let initialized = $state(false);
	let error = $state<string | null>(null);

	function classKey(dto: PromptClassDto): string {
		return `${dto.lane}:${dto.subagent_persona ?? ''}`;
	}

	async function detectDrift(dtos: PromptClassDto[]): Promise<Set<string>> {
		const candidates = dtos.filter(
			(d) =>
				d.registration.registered &&
				d.observation_count > 0 &&
				d.registration.composition_id !== null
		);
		const results = await Promise.all(
			candidates.map(async (d) => {
				try {
					const [revisions, detail] = await Promise.all([
						getCompositionRevisions(d.registration.composition_id as string),
						d.lane === 'main'
							? getPromptClassMain()
							: getPromptClassSubagent(d.subagent_persona ?? UNRESOLVED_PERSONA_SEGMENT)
					]);
					const current = revisions[0];
					if (current === undefined) return null;
					return hasBlock2Drift(current.blocks, detail.sample_blocks) ? classKey(d) : null;
				} catch {
					return null;
				}
			})
		);
		return new Set(results.filter((k): k is string => k !== null));
	}

	async function loadCatalogue(): Promise<void> {
		loading = true;
		error = null;
		try {
			const dateFrom = new Date(Date.now() - WINDOW_MS[windowOption]).toISOString();
			const [classesResult, anomaliesResult] = await Promise.all([
				listPromptClasses({ date_from: dateFrom }),
				listPromptAnomalies({ limit: 500, offset: 0 })
			]);
			classes = classesResult;
			anomalies = anomaliesResult;
			driftedKeys = await detectDrift(classesResult);
			modelBreakdownByKey = new Map(classesResult.map((d) => [classKey(d), d.model_breakdown]));
			error = null;
		} catch (err) {
			classes = [];
			anomalies = [];
			driftedKeys = new Set();
			modelBreakdownByKey = new Map();
			error = err instanceof ApiError ? err.message : String(err);
		} finally {
			loading = false;
			initialized = true;
		}
	}

	onMount(() => {
		void loadCatalogue();
		now = Date.now();
		clockTimer = setInterval(() => {
			now = Date.now();
		}, 1_000);
		const unsubClasses = onApiCacheUpdate('/api/prompts/classes', () => void loadCatalogue(), {
			prefix: true
		});
		const unsubAnomalies = onApiCacheUpdate('/api/prompts/anomalies', () => void loadCatalogue(), {
			prefix: true
		});
		const unsubCompositions = onApiCacheUpdate(
			'/api/prompts/compositions',
			() => void loadCatalogue(),
			{ prefix: true }
		);
		return () => {
			unsubClasses();
			unsubAnomalies();
			unsubCompositions();
			if (clockTimer !== null) {
				clearInterval(clockTimer);
				clockTimer = null;
			}
		};
	});

	let firstWindowEffect = true;
	$effect(() => {
		void windowOption;
		if (firstWindowEffect) {
			firstWindowEffect = false;
			return;
		}
		void loadCatalogue();
	});

	const rows = $derived.by((): PromptCatalogueRowViewModel[] => {
		const classRows = classes.map((dto) =>
			classifyPromptClass(dto, { isDrifted: driftedKeys.has(classKey(dto)), now })
		);
		const anomalyRows = classifyAnomalyRows(anomalies);
		return [...classRows, ...anomalyRows].sort(compareCatalogueRows);
	});

	const registeredCount = $derived(classes.filter((c) => c.registration.registered).length);
	const observedCount = $derived(classes.filter((c) => c.observation_count > 0).length);
	const unregisteredCount = $derived(rows.filter((r) => r.state === 'unregistered').length);
	const driftedCount = $derived(rows.filter((r) => r.state === 'drifted').length);
	const needsInvestigationCount = $derived(
		rows.filter((r) => r.state === 'needs-investigation').length
	);

	const visibleRows = $derived(
		filterTier === 'all' ? rows : rows.filter((r) => r.state === filterTier)
	);

	const showEmpty = $derived(initialized && error === null && rows.length === 0);
	const showAllClean = $derived(
		initialized &&
			error === null &&
			rows.length > 0 &&
			unregisteredCount === 0 &&
			driftedCount === 0 &&
			needsInvestigationCount === 0
	);

	let exportInFlight = $state(false);
	let importInFlight = $state(false);
	let exportError = $state<string | null>(null);
	let importError = $state<string | null>(null);
	let importResult = $state<PromptCompositionImportResult | null>(null);
	let importFileInput = $state<HTMLInputElement | null>(null);

	async function downloadExport(): Promise<void> {
		if (exportInFlight) return;
		exportInFlight = true;
		exportError = null;
		try {
			const payload = await exportPromptCompositions();
			const json = JSON.stringify(payload, null, 2);
			const blob = new Blob([json], { type: 'application/json' });
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			const stamp = new Date().toISOString().replace(/[:.]/g, '-');
			a.download = `prompt-compositions-${stamp}.json`;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);
		} catch (err) {
			exportError = err instanceof ApiError ? err.message : String(err);
		} finally {
			exportInFlight = false;
		}
	}

	function triggerImportPicker(): void {
		importError = null;
		importResult = null;
		importFileInput?.click();
	}

	async function handleImportFile(ev: Event): Promise<void> {
		const target = ev.currentTarget as HTMLInputElement;
		const file = target.files?.[0] ?? null;
		target.value = '';
		if (file === null) return;
		if (importInFlight) return;
		importInFlight = true;
		importError = null;
		importResult = null;
		try {
			const text = await file.text();
			let parsed: PromptCompositionExport;
			try {
				parsed = JSON.parse(text) as PromptCompositionExport;
			} catch (err) {
				throw new Error(`file is not valid JSON: ${err instanceof Error ? err.message : err}`);
			}
			if (typeof parsed !== 'object' || parsed === null || !Array.isArray(parsed.compositions)) {
				throw new Error('file does not match the export shape (missing `compositions` array)');
			}
			importResult = await importPromptCompositions(parsed);
			await loadCatalogue();
		} catch (err) {
			importError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		} finally {
			importInFlight = false;
		}
	}

	const FILTER_CHIPS: { tier: FilterTier; label: string }[] = [
		{ tier: 'all', label: 'All' },
		{ tier: 'drifted', label: 'Drifted' },
		{ tier: 'needs-investigation', label: 'Needs investigation' },
		{ tier: 'unregistered', label: 'Unregistered' },
		{ tier: 'registered', label: 'Registered' }
	];
	function chipCount(tier: FilterTier): number {
		return tier === 'all' ? rows.length : rows.filter((r) => r.state === tier).length;
	}

	function clearFilters(): void {
		filterTier = 'all';
	}

	function chipLabel(tier: FilterTier): string {
		return FILTER_CHIPS.find((c) => c.tier === tier)?.label ?? tier;
	}

	const scopeLabel = $derived(
		filterTier === 'all'
			? `${windowOption} window, every state`
			: `${windowOption} window, ${chipLabel(filterTier).toLowerCase()} only`
	);
</script>

<svelte:head>
	<title>Prompts — mike</title>
</svelte:head>

<section class="flex flex-col gap-6 overflow-y-auto p-6" data-slot="prompts-page">
	<div class="flex flex-wrap items-baseline justify-between gap-4">
		<PageIntro title="Prompt catalogue">
			<span
				>Every system prompt your fleet has registered, plus everything new it's sent — persona,
				traffic, and its</span
			>
			<span class="inline-flex items-center gap-1">
				<span>registration status</span>
				<HelpHint slug="prompt-registration" /><span>.</span>
			</span>
		</PageIntro>

		<div class="flex flex-wrap items-center gap-2" data-slot="prompts-toolbar">
			<Tooltip text="Import compositions from a JSON backup.">
				{#snippet child({ props })}
					<span class="pr-toolbar-btn-backing">
						<button
							{...props}
							type="button"
							class="pr-toolbar-btn"
							data-augmented-ui="tr-clip bl-clip border"
							disabled={importInFlight}
							onclick={triggerImportPicker}
						>
							<Upload class="size-3.5" aria-hidden="true" />
							<span>{importInFlight ? 'importing…' : 'import'}</span>
						</button>
					</span>
				{/snippet}
			</Tooltip>
			<Tooltip text="Download all registered compositions as JSON.">
				{#snippet child({ props })}
					<span class="pr-toolbar-btn-backing">
						<button
							{...props}
							type="button"
							class="pr-toolbar-btn"
							data-augmented-ui="tr-clip bl-clip border"
							disabled={exportInFlight || registeredCount === 0}
							onclick={() => void downloadExport()}
						>
							<Download class="size-3.5" aria-hidden="true" />
							<span>{exportInFlight ? 'exporting…' : 'export'}</span>
						</button>
					</span>
				{/snippet}
			</Tooltip>
			<Tooltip text="Reload the catalogue now, without waiting for the next automatic refresh.">
				{#snippet child({ props })}
					<span class="pr-toolbar-btn-backing">
						<button
							{...props}
							type="button"
							class="pr-toolbar-btn"
							data-augmented-ui="tr-clip bl-clip border"
							disabled={loading}
							onclick={() => void loadCatalogue()}
							data-slot="prompts-refresh"
						>
							<span>{loading ? 'refreshing…' : 'refresh'}</span>
						</button>
					</span>
				{/snippet}
			</Tooltip>
			<span class="pr-toolbar-divider" aria-hidden="true"></span>
			<Tooltip
				text="Volatile-line patterns — strings normalised before the prompt class hash is computed."
				slug="volatile-lines"
			>
				{#snippet child({ props })}
					<span class="pr-toolbar-btn-backing">
						<a
							{...props}
							href={resolve('/prompts/volatile-lines')}
							class="pr-toolbar-btn"
							data-augmented-ui="tr-clip bl-clip border"
							data-slot="prompts-volatile-lines-link"
						>
							<ListFilter class="size-3.5" aria-hidden="true" />
							<span>volatile lines</span>
						</a>
					</span>
				{/snippet}
			</Tooltip>
			<input
				bind:this={importFileInput}
				type="file"
				accept="application/json,.json"
				class="hidden"
				aria-hidden="true"
				tabindex="-1"
				onchange={handleImportFile}
			/>
		</div>
	</div>

	{#if exportError}
		<div
			class="rounded border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-label text-[var(--status-danger-fg)]"
			role="alert"
			data-slot="prompts-export-error"
		>
			export failed: {exportError}
		</div>
	{/if}

	{#if importError}
		<div
			class="rounded border border-[var(--status-danger-border)] bg-[var(--status-danger-bg)] px-3 py-2 text-label text-[var(--status-danger-fg)]"
			role="alert"
			data-slot="prompts-import-error"
		>
			import failed: {importError}
		</div>
	{/if}

	{#if importResult}
		<div
			class="rounded border border-[var(--border-default)] bg-[var(--surface-2)] px-3 py-2 text-label text-[var(--text-1)]"
			data-slot="prompts-import-result"
		>
			Imported {importResult.imported}, skipped {importResult.skipped}
			{importResult.skipped === 1 ? 'collision' : 'collisions'}, {importResult.errors}
			{importResult.errors === 1 ? 'error' : 'errors'}.
		</div>
	{/if}

	{#if loading && !initialized}
		<div
			class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4"
			data-slot="prompts-kpi-skeleton"
		>
			{#each Array.from({ length: 4 }) as _, i (i)}
				<div
					class="kpi-skeleton rounded-md border border-[var(--border-default)] bg-[var(--surface-card)] p-[18px]"
					style="animation-delay: {i * 0.05}s"
				>
					<div class="skeleton-bar" style="width: 60%; height: 10px;"></div>
					<div class="skeleton-bar mt-3" style="width: 35%; height: 24px;"></div>
				</div>
			{/each}
		</div>
	{:else}
		<PromptKpiStrip
			registered={registeredCount}
			observed={observedCount}
			unregistered={unregisteredCount}
			drifted={driftedCount}
			windowLabel={windowOption}
		/>
	{/if}

	<section
		class="pr-panel"
		data-augmented-ui="tl-clip br-clip border"
		aria-label="Prompt catalogue"
		data-slot="prompt-catalogue-section"
	>
		<header class="pr-panel-head">
			<span class="pr-panel-tag-backing">
				<h3 class="pr-panel-tag" data-augmented-ui="tl-clip br-clip border">Prompt catalogue</h3>
			</span>
			<span class="pr-panel-count">{visibleRows.length}</span>
		</header>

		<div class="pr-panel-controls">
			<div class="pr-window-control" role="group" aria-label="Time window">
				{#each WINDOW_OPTIONS as opt (opt)}
					<button
						type="button"
						class="pr-window-btn"
						class:pr-window-btn--active={windowOption === opt}
						aria-pressed={windowOption === opt}
						onclick={() => (windowOption = opt)}
					>
						{opt}
					</button>
				{/each}
			</div>
			<div class="pr-filter-chips" role="group" aria-label="Filter by state">
				{#each FILTER_CHIPS as chip (chip.tier)}
					<button
						type="button"
						class="pr-filter-chip"
						class:pr-filter-chip--active={filterTier === chip.tier}
						aria-pressed={filterTier === chip.tier}
						onclick={() => (filterTier = chip.tier)}
					>
						{chip.label} · {chipCount(chip.tier)}
					</button>
				{/each}
			</div>
			<FilterScopeStatus
				{scopeLabel}
				shownCount={error !== null ? undefined : visibleRows.length}
				totalCount={error !== null ? undefined : rows.length}
				itemNoun="prompt shape"
			/>
		</div>

		{#if error !== null}
			<LoadErrorBanner
				title="Couldn't load the prompt catalogue"
				detail={error}
				onRetry={() => void loadCatalogue()}
			/>
		{:else if showEmpty}
			<div data-slot="prompts-empty-clean">
				<EmptyStateOnboarding
					headline="No prompt traffic captured yet"
					body="Every distinct system prompt your fleet sends will appear here automatically — nothing to configure first."
					command="ANTHROPIC_BASE_URL=http://localhost:9988 claude"
					confirmation="Prompts will appear here as soon as traffic is captured."
					onOpenHelp={openHelp}
				/>
			</div>
		{:else}
			{#if showAllClean}
				<div class="pr-all-clean" data-slot="prompts-all-clean">
					<span class="badge-nx badge-active">
						<span class="badge-nx-dot"></span>
						✓
					</span>
					All {rows.length} prompt shapes are registered and match their stored composition.
				</div>
			{/if}
			<div class="flex flex-col gap-2" data-slot="prompts-list">
				{#if loading}
					{#each Array.from({ length: 5 }) as _, i (i)}
						<div class="row-skeleton" style="animation-delay: {i * 0.05}s">
							<div class="skeleton-bar" style="width: 30%; height: 12px;"></div>
							<div class="skeleton-bar" style="width: 12%; height: 12px; margin-left: 12px;"></div>
							<div class="skeleton-bar" style="width: 10%; height: 12px; margin-left: auto;"></div>
						</div>
					{/each}
				{:else if visibleRows.length === 0}
					<div data-slot="prompts-empty-filtered">
						<FilteredEmptyState
							icon={SearchX}
							filtered={true}
							emptyText="No prompt shapes captured yet"
							filteredText="No prompt shapes match this filter"
							filteredSubtext="Try a different state chip, widen the time window, or clear filters to see every captured shape."
							onClearFilters={clearFilters}
						/>
					</div>
				{:else}
					{#each visibleRows as row (row.key)}
						<PromptCatalogueRow {row} modelBreakdown={modelBreakdownByKey.get(row.key)} />
					{/each}
				{/if}
			</div>
		{/if}
	</section>
</section>

<style>
	.pr-panel {
		--aug-tl: 10px;
		--aug-br: 10px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
		background: var(--surface-card);
	}
	.pr-panel-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}
	.pr-panel-tag-backing {
		display: inline-flex;
		align-self: flex-start;
		background: var(--surface-card);
	}
	.pr-panel-tag {
		--aug-tl: 6px;
		--aug-tr: 0;
		--aug-br: 6px;
		--aug-bl: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--data-teal-border);
		margin: 0;
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 800;
		font-size: var(--text-label);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--data-teal-fg);
		padding: 3px 8px;
		background:
			linear-gradient(0deg, var(--data-teal-bg), var(--data-teal-bg)), var(--surface-card);
	}
	.pr-panel-count {
		font-family: 'JetBrains Mono Variable', ui-monospace, monospace;
		font-size: var(--text-label);
		color: var(--text-3);
	}

	.pr-panel-controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.pr-window-control,
	.pr-filter-chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px;
	}
	.pr-window-btn,
	.pr-filter-chip {
		border: 1px solid transparent;
		background: transparent;
		border-radius: 4px;
		padding: 3px 8px;
		font-size: var(--text-label);
		font-family: 'JetBrains Mono Variable', ui-monospace, monospace;
		color: var(--text-3);
		cursor: pointer;
		transition:
			color 0.12s ease,
			border-color 0.12s ease;
	}
	.pr-window-btn:hover,
	.pr-filter-chip:hover {
		color: var(--text-1);
	}
	.pr-window-btn--active,
	.pr-filter-chip--active {
		color: var(--interactive-active-fg);
		border-color: var(--interactive-active-border);
	}

	.pr-all-clean {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 4px;
		font-size: var(--text-label);
		color: var(--text-2);
	}

	.pr-toolbar-btn-backing {
		display: inline-flex;
		background: var(--surface-bg);
	}
	.pr-toolbar-btn {
		--edge: 8px;
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		background: var(--surface-card);
		color: var(--text-2);
		font-size: var(--text-label);
		font-weight: 600;
		text-decoration: none;
		cursor: pointer;
		transition: color 0.12s ease;
	}
	.pr-toolbar-btn:hover:not(:disabled),
	.pr-toolbar-btn:focus-visible {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.pr-toolbar-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.pr-toolbar-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.pr-toolbar-divider {
		align-self: stretch;
		width: 1px;
		margin: 2px 2px;
		background: var(--border-subtle);
	}

	.kpi-skeleton {
		animation: rise 0.4s ease both;
	}
	.row-skeleton {
		display: flex;
		align-items: center;
		gap: 12px;
		border-radius: 6px;
		border: 1px solid var(--border-subtle);
		background: var(--surface-bg);
		padding: 10px 12px;
		animation: rise 0.4s ease both;
	}
	.skeleton-bar {
		background: var(--surface-2);
		border-radius: 3px;
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
