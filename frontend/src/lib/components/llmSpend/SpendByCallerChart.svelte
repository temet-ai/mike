<script lang="ts">
	import { Tooltip, TooltipTrigger, TooltipContent } from '$lib/components/ui/tooltip';
	import LegendFilter from '$lib/components/shared/LegendFilter.svelte';
	import type { LegendEntry } from '$lib/components/shared/LegendFilter.svelte';
	import { formatUsd } from '$lib/format';
	import { groupCallers, groupLabelForRole, memberDisplayLabel } from '$lib/api/llmSpend';
	import type { LlmSpendCallerDto, LlmSpendDayCallerDto } from '$lib/api/llmSpend';

	interface Props {
		callers: LlmSpendCallerDto[];
		byDayCaller: LlmSpendDayCallerDto[];
	}

	const { callers, byDayCaller }: Props = $props();

	const PALETTE = [
		'--funk-cyan',
		'--funk-magenta',
		'--funk-green',
		'--funk-yellow',
		'--funk-blue',
		'--funk-red',
		'--data-teal-fg'
	] as const;

	const callerOrder = $derived(callers.map((c) => c.key));
	const callerByKey = $derived(new Map(callers.map((c) => [c.key, c])));
	const roleByKey = $derived(new Map(callers.map((c) => [c.key, c.role])));
	const colorByKey = $derived(
		new Map(callerOrder.map((key, i) => [key, PALETTE[i % PALETTE.length]]))
	);

	interface LegendGroup {
		groupKey: string;
		groupLabel: string;
		showCaption: boolean;
		entries: LegendEntry[];
	}

	const legendGroups = $derived.by((): LegendGroup[] =>
		groupCallers(callers).map((g) => ({
			groupKey: g.groupKey,
			groupLabel: g.groupLabel,
			showCaption: g.members.length > 1,
			entries: g.members.map((m) => ({
				key: m.key,
				label: memberDisplayLabel(m),
				kind: 'fill' as const,
				colorToken: colorByKey.get(m.key) ?? PALETTE[0]
			}))
		}))
	);

	let active = $state<Set<string>>(new Set());
	const seenKeys = new Set<string>();
	$effect(() => {
		const keys = callerOrder;
		const next = new Set(active);
		let changed = false;
		for (const k of keys) {
			if (seenKeys.has(k)) continue;
			seenKeys.add(k);
			if (!next.has(k)) {
				next.add(k);
				changed = true;
			}
		}
		if (changed) active = next;
	});

	interface DaySegment {
		key: string;
		cost: number;
		calls: number;
	}

	interface DayBar {
		day: string;
		total: number;
		segments: DaySegment[];
	}

	const days = $derived.by((): DayBar[] => {
		const byDay = new Map<string, Map<string, LlmSpendDayCallerDto>>();
		for (const row of byDayCaller) {
			if (!active.has(row.key)) continue;
			const dayMap = byDay.get(row.day) ?? new Map<string, LlmSpendDayCallerDto>();
			dayMap.set(row.key, row);
			byDay.set(row.day, dayMap);
		}
		return [...byDay.keys()]
			.sort((a, b) => a.localeCompare(b))
			.map((day) => {
				const dayMap = byDay.get(day) ?? new Map<string, LlmSpendDayCallerDto>();
				const segments: DaySegment[] = [];
				let total = 0;
				for (const key of callerOrder) {
					const row = dayMap.get(key);
					if (!row || row.cost_usd <= 0) continue;
					segments.push({ key, cost: row.cost_usd, calls: row.call_count });
					total += row.cost_usd;
				}
				return { day, total, segments };
			});
	});

	const maxTotal = $derived(days.reduce((m, d) => Math.max(m, d.total), 0));
	const labelStep = $derived(Math.max(1, Math.ceil(days.length / 12)));

	function dayDate(day: string): Date {
		return new Date(`${day}T00:00:00Z`);
	}
	function shortDayLabel(day: string): string {
		return dayDate(day).toLocaleDateString(undefined, {
			month: 'short',
			day: 'numeric',
			timeZone: 'UTC'
		});
	}
	function fullDayLabel(day: string): string {
		return dayDate(day).toLocaleDateString(undefined, {
			weekday: 'short',
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			timeZone: 'UTC'
		});
	}

	function segmentRects(bar: DayBar): Array<{ key: string; y: number; height: number }> {
		if (bar.total <= 0) return [];
		let cumulative = 0;
		const rects: Array<{ key: string; y: number; height: number }> = [];
		for (const seg of bar.segments) {
			const height = (seg.cost / bar.total) * 100;
			rects.push({ key: seg.key, y: 100 - cumulative - height, height });
			cumulative += height;
		}
		return rects;
	}

	function barHeightPct(bar: DayBar): number {
		if (maxTotal <= 0) return 0;
		return Math.max((bar.total / maxTotal) * 100, bar.total > 0 ? 2 : 0);
	}

	interface TooltipGroupSection {
		groupKey: string;
		groupLabel: string;
		cost: number;
		calls: number;
		segments: DaySegment[];
	}

	function groupedTooltipSections(bar: DayBar): TooltipGroupSection[] {
		const byRole = new Map<string, DaySegment[]>();
		for (const seg of bar.segments) {
			const role = roleByKey.get(seg.key) ?? seg.key;
			const list = byRole.get(role) ?? [];
			list.push(seg);
			byRole.set(role, list);
		}
		return [...byRole.entries()]
			.map(([role, segments]): TooltipGroupSection => ({
				groupKey: role,
				groupLabel: groupLabelForRole(role),
				cost: segments.reduce((sum, s) => sum + s.cost, 0),
				calls: segments.reduce((sum, s) => sum + s.calls, 0),
				segments
			}))
			.sort((a, b) => b.cost - a.cost || a.groupKey.localeCompare(b.groupKey));
	}
</script>

<section class="sc-chart" aria-labelledby="sc-title">
	<div class="sc-head">
		<h2 class="sc-title" id="sc-title">Spend per day, by caller</h2>
		<div class="sc-legend-groups">
			{#each legendGroups as group (group.groupKey)}
				<div class="sc-legend-group">
					{#if group.showCaption}
						<span class="sc-legend-caption">{group.groupLabel}</span>
					{/if}
					<LegendFilter entries={group.entries} bind:active />
				</div>
			{/each}
		</div>
	</div>

	{#if days.length === 0}
		<p class="sc-empty">No spend recorded in this window.</p>
	{:else}
		<div class="sc-row" role="img" aria-label="Stacked bar chart of daily LLM spend by caller">
			{#each days as bar, i (bar.day)}
				<Tooltip>
					<TooltipTrigger>
						{#snippet child({ props })}
							<div
								class="sc-col"
								{...props}
								aria-label="{fullDayLabel(bar.day)}: {formatUsd(bar.total)}"
							>
								<div class="sc-col-bar-wrap">
									<svg
										class="sc-col-bar"
										style="height: {barHeightPct(bar)}%"
										viewBox="0 0 10 100"
										preserveAspectRatio="none"
										aria-hidden="true"
									>
										{#each segmentRects(bar) as rect (rect.key)}
											<rect
												x="0"
												y={rect.y}
												width="10"
												height={rect.height}
												fill="var({colorByKey.get(rect.key) ?? PALETTE[0]})"
											/>
										{/each}
									</svg>
								</div>
								<span class="sc-col-label">{i % labelStep === 0 ? shortDayLabel(bar.day) : ''}</span>
							</div>
						{/snippet}
					</TooltipTrigger>
					<TooltipContent>
						<div class="sc-tip">
							<strong class="sc-tip-day">{fullDayLabel(bar.day)}</strong>
							<span class="sc-tip-total">{formatUsd(bar.total)} total</span>
							{#each groupedTooltipSections(bar) as section (section.groupKey)}
								<div class="sc-tip-group">
									<div class="sc-tip-group-head">
										<span class="sc-tip-group-label">{section.groupLabel}</span>
										<span class="sc-tip-group-subtotal"
											>{formatUsd(section.cost)} ({section.calls} call{section.calls === 1
												? ''
												: 's'})</span
										>
									</div>
									<ul class="sc-tip-list">
										{#each section.segments as seg (seg.key)}
											{@const member = callerByKey.get(seg.key)}
											<li class="sc-tip-row">
												<span
													class="sc-tip-swatch"
													style="background: var({colorByKey.get(seg.key) ?? PALETTE[0]})"
													aria-hidden="true"
												></span>
												<span class="sc-tip-label"
													>{member ? memberDisplayLabel(member) : seg.key}</span
												>
												<span class="sc-tip-cost"
													>{formatUsd(seg.cost)} ({seg.calls} call{seg.calls === 1
														? ''
														: 's'})</span
												>
											</li>
										{/each}
									</ul>
								</div>
							{/each}
						</div>
					</TooltipContent>
				</Tooltip>
			{/each}
		</div>
	{/if}
</section>

<style>
	.sc-chart {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.sc-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		flex-wrap: wrap;
	}
	.sc-title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}
	.sc-empty {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}
	.sc-legend-groups {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}
	.sc-legend-group {
		display: flex;
		align-items: center;
		gap: 6px;
		padding-left: 10px;
		border-left: 1px solid var(--border-subtle);
	}
	.sc-legend-group:first-child {
		padding-left: 0;
		border-left: none;
	}
	.sc-legend-caption {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.sc-row {
		display: flex;
		align-items: stretch;
		gap: 2px;
		height: 180px;
		padding: 8px 4px 0;
		border-bottom: 1px solid var(--border-subtle);
	}
	.sc-col {
		flex: 1 1 0;
		min-width: 2px;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		align-items: center;
		gap: 4px;
		outline: none;
		cursor: default;
	}
	.sc-col:focus-visible {
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}
	.sc-col-bar-wrap {
		width: 100%;
		flex: 1 1 auto;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		min-height: 0;
	}
	.sc-col-bar {
		width: 100%;
		display: block;
		transition: height 150ms ease-out;
	}
	.sc-col-label {
		font-size: 9px;
		color: var(--text-3);
		white-space: nowrap;
		height: 12px;
	}
	.sc-tip {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 180px;
	}
	.sc-tip-day {
		color: var(--text-strong);
		font-size: 12px;
	}
	.sc-tip-total {
		font-size: 11px;
		color: var(--text-3);
	}
	.sc-tip-group {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-top: 4px;
		border-top: 1px solid var(--border-subtle);
	}
	.sc-tip-group:first-of-type {
		padding-top: 4px;
		border-top: 1px solid var(--border-subtle);
	}
	.sc-tip-group-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-3);
	}
	.sc-tip-group-subtotal {
		font-variant-numeric: tabular-nums;
		text-transform: none;
		letter-spacing: normal;
	}
	.sc-tip-list {
		list-style: none;
		margin: 2px 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.sc-tip-row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		color: var(--text-1);
	}
	.sc-tip-swatch {
		width: 8px;
		height: 8px;
		flex-shrink: 0;
		border-radius: 2px;
	}
	.sc-tip-label {
		flex: 1 1 auto;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.sc-tip-cost {
		flex-shrink: 0;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
</style>
