<script lang="ts">
	import { getContext } from 'svelte';
	import { Search } from '@lucide/svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';
	import { formatRelative } from '$lib/time';
	import {
		TURN_GUARD_POINT,
		modeIsSetting,
		offReason,
		type DecisionMode,
		type DecisionPointRow,
		type DecisionsCalibration
	} from '$lib/api/decisions';
	import { groupByArea } from '$lib/decisions/areas';
	import {
		NO_VALUE,
		failOpenPercent,
		formatMs,
		formatSpend,
		groupTotals,
		hasHistory,
		matchesFilter,
		rightCell,
		rowLabel
	} from '$lib/decisions/calibration';
	import { formatInterval, wilson } from '$lib/decisions/wilson';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import SearchField from '$lib/components/shared/SearchField.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Select, type SelectOption } from '$lib/components/ui/select';
	import SettingsSection from './SettingsSection.svelte';

	interface Props {
		calibration: DecisionsCalibration | null;
		loadError?: string | null;
		onRetry?: () => void;
		onModeChange?: (point: string, mode: DecisionMode) => void;
		savingPoints?: string[];
		rowErrors?: Record<string, string | null>;
		now?: number;
	}

	const {
		calibration,
		loadError = null,
		onRetry,
		onModeChange,
		savingPoints = [],
		rowErrors = {},
		now = Date.now()
	}: Props = $props();

	const MODE_OPTIONS: SelectOption[] = [
		{ value: 'on', label: 'on' },
		{ value: 'shadow', label: 'shadow' },
		{ value: 'off', label: 'off' }
	];

	const COLUMNS: { key: string; label: string; tip: string; numeric: boolean }[] = [
		{
			key: 'point',
			label: 'Point',
			tip: 'The point id, for example turn_guard_precheck.',
			numeric: false
		},
		{
			key: 'mode',
			label: 'Mode',
			tip: 'On acts on the answer. Shadow records it only. Off does not call Jev.',
			numeric: false
		},
		{ key: 'calls', label: 'Calls', tip: 'Calls in the window.', numeric: true },
		{
			key: 'applied',
			label: 'Applied',
			tip: 'Calls whose answer changed what mike did.',
			numeric: true
		},
		{
			key: 'fail',
			label: 'Fail-open',
			tip: 'Share of calls that failed and fell back to the off behaviour.',
			numeric: true
		},
		{
			key: 'auroc',
			label: 'AUROC',
			tip: 'How well the score separates right from wrong outcomes. 0.5 is chance, 1 is perfect.',
			numeric: true
		},
		{
			key: 'right',
			label: 'Right',
			tip: 'Applied answers that the labeller judged right, with a 95% interval. For the turn-guard pre-check, held nudges that were labelled wrong, so the hold was correct.',
			numeric: true
		},
		{ key: 'spend', label: 'Spend', tip: 'Spend in the window.', numeric: true },
		{
			key: 'findings',
			label: 'Findings',
			tip: 'Problems the review found. Open the row to read them.',
			numeric: true
		}
	];

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	let localCollapsed = $state<Record<string, boolean>>({});
	let query = $state('');
	let openPoint = $state<string | null>(null);

	function groupKey(key: string): string {
		return `decisions:group:${key}`;
	}
	function groupOpen(key: string): boolean {
		if (collapseStore) return collapseStore.isExpanded(groupKey(key), true);
		return !(localCollapsed[key] ?? false);
	}
	function toggleGroup(key: string): void {
		if (collapseStore) {
			collapseStore.toggle(groupKey(key));
			return;
		}
		localCollapsed = { ...localCollapsed, [key]: !(localCollapsed[key] ?? false) };
	}
	function toggleRow(point: string): void {
		openPoint = openPoint === point ? null : point;
	}

	const filtering = $derived(query.trim() !== '');
	const rows = $derived(calibration?.points ?? []);
	const shown = $derived(rows.filter((r) => matchesFilter(r, query)));
	const groups = $derived(groupByArea(shown));
	const layerOff = $derived(calibration !== null && !calibration.layer.on);
	const showTable = $derived(calibration !== null && (!layerOff || hasHistory(rows)));
	const lastReview = $derived(
		calibration?.last_loop_at ? formatRelative(calibration.last_loop_at, now) : 'none yet'
	);

	function isSaving(point: string): boolean {
		return savingPoints.includes(point);
	}
	function isOpen(row: DecisionPointRow): boolean {
		return openPoint === row.point || (rowErrors[row.point] ?? null) !== null;
	}
	function modeOf(row: DecisionPointRow): DecisionMode {
		return (row.mode === 'off' || row.mode === 'shadow' ? row.mode : 'on') as DecisionMode;
	}
	function isOff(row: DecisionPointRow): boolean {
		return row.mode === 'off';
	}
	function modeTip(row: DecisionPointRow): string {
		return row.point === TURN_GUARD_POINT
			? 'Set in decisions.turn_guard.mode.'
			: `Set by ${row.mode_source}.`;
	}
	function intervalOf(wins: number, total: number): string {
		return formatInterval(wilson(wins, total));
	}
	function fixed(value: number | null, digits = 2): string {
		return value === null ? NO_VALUE : value.toFixed(digits);
	}
</script>

<SettingsSection title="Decision points" id="decision-points" testId="decisions-calibration">
	{#snippet action()}
		<SearchField label="Filter decision points" bind:value={query} placeholder="Filter" />
	{/snippet}
	{#snippet description()}
		What Jev answered at each decision point over the last {calibration?.window_days ?? '—'} days, and
		how often the applied answer was right. Last review: {lastReview}.
	{/snippet}

	{#if loadError !== null}
		<div class="dcp-pad">
			<LoadErrorBanner title="Couldn't load decision points" detail={loadError} {onRetry} />
		</div>
	{:else if calibration === null}
		<div class="dcp-skeleton" aria-hidden="true">
			<div class="dcp-skel" data-motion="wait"></div>
			<div class="dcp-skel" data-motion="wait"></div>
			<div class="dcp-skel" data-motion="wait"></div>
		</div>
	{:else}
		{#if layerOff}
			<div class="dcp-off" data-slot="decisions-off" role="status">
				<span class="machine dcp-off__label">Decisions off</span>
				<span class="dcp-off__reason">{offReason(calibration.layer)}</span>
				{#if showTable}
					<span class="dcp-off__note">Earlier calls in the window are shown below.</span>
				{/if}
			</div>
		{/if}

		{#if showTable}
			{#if filtering && groups.length === 0}
				<div class="dcp-pad">
					<FilteredEmptyState
						icon={Search}
						filtered={true}
						emptyText="No decision points"
						filteredText="No decision points match"
						clearLabel="Clear filter"
						onClearFilters={() => (query = '')}
					/>
				</div>
			{:else}
				<div class="dcp-scroll" data-dim={layerOff ? '' : undefined}>
					<div class="dcp-table" role="table" aria-label="Decision points">
						<div class="dcp-row dcp-head" role="row">
							{#each COLUMNS as col (col.key)}
								<span
									class="dcp-cell dcp-head__cell machine"
									class:dcp-num={col.numeric}
									role="columnheader"
								>
									<Tooltip text={col.tip}>
										{#snippet child({ props })}
											<span class="dcp-tip" {...props}>{col.label}</span>
										{/snippet}
									</Tooltip>
								</span>
							{/each}
						</div>

						{#each groups as group (group.area.key)}
							{@const totals = groupTotals(group.rows)}
							{@const open = groupOpen(group.area.key)}
							<div role="rowgroup">
								<div role="row">
									<div role="cell">
										<button
											type="button"
											class="dcp-row dcp-group"
											data-open={open ? '' : undefined}
											aria-expanded={open}
											title="Click to {open ? 'hide' : 'show'} {group.area.label} points"
											onclick={() => toggleGroup(group.area.key)}
										>
											<span class="dcp-cell dcp-group__label machine">{group.area.label}</span>
											<span class="dcp-cell dcp-group__count">{group.rows.length} points</span>
											<span class="dcp-cell dcp-num">{totals.calls}</span>
											<span class="dcp-cell"></span>
											<span class="dcp-cell"></span>
											<span class="dcp-cell"></span>
											<span class="dcp-cell"></span>
											<span class="dcp-cell dcp-num">{formatSpend(totals.spend)}</span>
											<span class="dcp-cell"></span>
										</button>
									</div>
								</div>

								{#if open}
									{#each group.rows as row (row.point)}
										{@const score = row.score}
										{@const right = rightCell(row)}
										{@const rowOpen = isOpen(row)}
										{@const err = rowErrors[row.point] ?? null}
										<div
											class="dcp-row dcp-data"
											class:dcp-row--open={rowOpen}
											class:dcp-row--off={isOff(row)}
											role="row"
											data-point={row.point}
										>
											<span class="dcp-cell dcp-point" role="cell">
												<button
													type="button"
													class="dcp-point__btn"
													aria-expanded={rowOpen}
													onclick={() => toggleRow(row.point)}
												>
													<Tooltip text={row.point}>
														{#snippet child({ props })}
															<span class="dcp-point__name" {...props}>{rowLabel(row)}</span>
														{/snippet}
													</Tooltip>
												</button>
											</span>
											<span class="dcp-cell dcp-mode" role="cell">
												{#if modeIsSetting(row)}
													<Select
														class="dcp-select"
														aria-label="{rowLabel(row)} mode"
														value={modeOf(row)}
														options={MODE_OPTIONS}
														disabled={isSaving(row.point)}
														data-invalid={err !== null ? '' : undefined}
														onchange={(event) =>
															onModeChange?.(
																row.point,
																(event.currentTarget as HTMLSelectElement).value as DecisionMode
															)}
													/>
												{:else}
													<Tooltip text={modeTip(row)}>
														{#snippet child({ props })}
															<span class="dcp-fleet-mode" {...props}>per fleet</span>
														{/snippet}
													</Tooltip>
												{/if}
											</span>
											{#if score === null}
												<span class="dcp-cell dcp-notrun" role="cell">
													{isOff(row) ? 'off' : 'not run'}
												</span>
											{:else}
												<span class="dcp-cell dcp-num" role="cell">{score.counts.calls}</span>
												<span class="dcp-cell dcp-num" role="cell">{score.counts.applied_on}</span>
												<span class="dcp-cell dcp-num" role="cell">{failOpenPercent(row)}</span>
												<span class="dcp-cell dcp-num" role="cell">{fixed(score.auroc)}</span>
												<span class="dcp-cell dcp-num" role="cell">
													{#if right.holdSide}
														<Tooltip
															text="Held nudges that were labelled wrong, so the hold was correct."
														>
															{#snippet child({ props })}
																<span class="dcp-tip" {...props}
																	>{right.counts}{right.interval === ''
																		? ''
																		: ` ${right.interval}`}</span
																>
															{/snippet}
														</Tooltip>
													{:else}
														{right.counts}{right.interval === '' ? '' : ` ${right.interval}`}
													{/if}
												</span>
												<span class="dcp-cell dcp-num" role="cell">
													{formatSpend(score.counts.cost_usd)}
												</span>
												<span
													class="dcp-cell dcp-num"
													class:dcp-findings={score.findings.length > 0}
													role="cell"
												>
													{score.findings.length === 0 ? '' : score.findings.length}
												</span>
											{/if}
										</div>

										{#if rowOpen}
											<div role="row">
												<div role="cell" class="dcp-detail" data-point-detail={row.point}>
													{#if err !== null}
														<p class="dcp-detail__error" role="alert">
															Could not save the mode of {rowLabel(row)}: {err}
														</p>
													{/if}
													{#if score === null}
														<p class="dcp-detail__line">No calls in the window.</p>
													{:else}
														<div class="dcp-findings-list">
															{#each score.findings as finding (finding)}
																<p class="dcp-finding">{finding}</p>
															{:else}
																<p class="dcp-detail__line">No findings</p>
															{/each}
														</div>
														<dl class="dcp-facts">
															<dt>Latency p50 / p95</dt>
															<dd>
																{formatMs(score.counts.latency_p50_ms)} / {formatMs(
																	score.counts.latency_p95_ms
																)}
															</dd>
															<dt>Labelled in the window</dt>
															<dd>{score.counts.labelled}</dd>
															<dt>Labelled in the last 7 days</dt>
															<dd>{score.counts.labelled_last_7d}</dd>
															<dt>Labels per day</dt>
															<dd>{score.labels_per_day.toFixed(1)}</dd>
															<dt>Days to the minimum label count</dt>
															<dd>{fixed(score.days_to_min_labels, 1)}</dd>
															<dt>Schema errors</dt>
															<dd>{score.counts.schema_errors}</dd>
															<dt>Refused</dt>
															<dd>{score.counts.refused}</dd>
															<dt>Question refusals</dt>
															<dd>{score.counts.question_refusals}</dd>
															{#if row.point === TURN_GUARD_POINT}
																<dt>Held, labelled</dt>
																<dd>{score.held_labelled}</dd>
																<dt>Held, labelled wrong</dt>
																<dd>
																	{score.held_wrong}
																	{intervalOf(score.held_wrong, score.held_labelled)}
																</dd>
																<dt>Sent, labelled</dt>
																<dd>{score.sent_labelled}</dd>
																<dt>Sent, labelled right</dt>
																<dd>
																	{score.sent_right}
																	{intervalOf(score.sent_right, score.sent_labelled)}
																</dd>
															{/if}
														</dl>
													{/if}
												</div>
											</div>
										{/if}
									{/each}
								{/if}
							</div>
						{/each}
					</div>
				</div>
			{/if}
		{/if}
	{/if}
</SettingsSection>

<style>
	.dcp-pad {
		padding: var(--space-3) calc(var(--edge-md) + var(--space-2));
	}
	.dcp-skeleton {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) calc(var(--edge-md) + var(--space-2));
	}
	.dcp-skel {
		height: var(--control-h);
		background: var(--surface-2);
	}
	.dcp-off {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-2);
		min-height: var(--control-h);
		padding: var(--space-2) calc(var(--edge-md) + var(--space-2));
		border-bottom: var(--hairline) solid var(--border-subtle);
		background: var(--surface-2);
	}
	.dcp-off__label {
		font-size: var(--text-xs);
		color: var(--text-1);
	}
	.dcp-off__reason {
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.dcp-off__note {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.dcp-scroll {
		--dcp-cols: minmax(15rem, 1.8fr) 7rem 4.5rem 4.5rem 5.5rem 4.5rem 10rem 6rem 5.5rem;
		--dcp-fg: var(--text-1);
		overflow-x: auto;
		min-width: 0;
	}
	.dcp-scroll[data-dim] {
		--dcp-fg: var(--text-3);
	}
	.dcp-table {
		min-width: 62rem;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		color: var(--dcp-fg);
	}
	.dcp-row {
		box-sizing: border-box;
		display: grid;
		grid-template-columns: var(--dcp-cols);
		align-items: center;
		height: var(--control-h);
		border: 0;
		border-bottom: var(--hairline) solid var(--border-subtle);
		padding: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		text-align: left;
		width: 100%;
	}
	.dcp-cell {
		min-width: 0;
		padding: 0 var(--space-2);
		white-space: nowrap;
	}
	.dcp-num {
		text-align: right;
	}
	.dcp-head {
		position: sticky;
		top: 0;
		z-index: 1;
		background: var(--surface-2);
		color: var(--text-2);
	}
	.dcp-head__cell {
		font-size: var(--text-xs);
	}
	.dcp-tip {
		cursor: help;
	}
	.dcp-group {
		background: var(--surface-2);
		cursor: pointer;
		color: var(--text-1);
	}
	.dcp-group:not([data-open]) .dcp-group__label {
		color: var(--text-3);
	}
	.dcp-group:hover {
		color: var(--text-strong);
	}
	.dcp-group:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.dcp-group__label {
		font-size: var(--text-xs);
	}
	.dcp-group__count {
		color: var(--text-3);
	}
	.dcp-data:hover {
		background: var(--surface-2);
	}
	.dcp-row--open,
	.dcp-row--open:hover {
		background: var(--funk-cyan-dim);
	}
	.dcp-row--off .dcp-point__name,
	.dcp-row--off .dcp-num {
		color: var(--text-3);
	}
	.dcp-point {
		font-family: var(--font-body, inherit);
		font-size: var(--text-sm);
		padding: 0;
	}
	.dcp-point__btn {
		display: block;
		width: 100%;
		height: var(--control-h);
		padding: 0 var(--space-2);
		border: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		text-align: left;
		cursor: pointer;
		overflow-wrap: anywhere;
		white-space: normal;
		line-height: 1.2;
	}
	.dcp-point__btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.dcp-row--open .dcp-point__name {
		color: var(--funk-cyan);
	}
	.dcp-mode {
		padding: 0 var(--space-1);
	}
	.dcp-mode :global(.dcp-select) {
		width: 100%;
	}
	.dcp-mode :global(.dcp-select .sel__frame:has(select[data-invalid])) {
		--edge-line: var(--status-danger-border);
	}
	.dcp-fleet-mode {
		color: var(--text-3);
		cursor: help;
	}
	.dcp-notrun {
		grid-column: 3 / -1;
		color: var(--text-3);
	}
	.dcp-findings {
		color: var(--status-warning-fg);
	}
	.dcp-detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3) calc(var(--edge-md) + var(--space-2));
		border-bottom: var(--hairline) solid var(--border-subtle);
		background: var(--surface-card);
		font-size: var(--text-sm);
		overflow-wrap: anywhere;
	}
	.dcp-detail__line {
		margin: 0;
		color: var(--text-3);
	}
	.dcp-detail__error {
		margin: 0;
		color: var(--status-danger-fg);
	}
	.dcp-findings-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.dcp-finding {
		margin: 0;
		color: var(--status-warning-fg);
	}
	.dcp-facts {
		display: grid;
		grid-template-columns: minmax(12rem, max-content) minmax(0, 1fr);
		gap: var(--space-1) var(--space-4);
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
	}
	.dcp-facts dt {
		color: var(--text-3);
	}
	.dcp-facts dd {
		margin: 0;
		color: var(--text-1);
	}
</style>
