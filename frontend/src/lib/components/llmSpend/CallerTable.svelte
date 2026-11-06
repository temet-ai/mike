<script lang="ts">
	import { onMount } from 'svelte';
	import { ArrowDown, ArrowUp } from '@lucide/svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		callerHref,
		callsLabel,
		costLabel,
		groupCallers,
		memberDisplayLabel,
		perCallLabel
	} from '$lib/api/llmSpend';
	import type { LlmSpendCallerDto, LlmSpendCallerGroup, LlmSpendCallerSortKey } from '$lib/api/llmSpend';

	interface Props {
		callers: LlmSpendCallerDto[];
		totalCostUsd: number;
	}

	const { callers, totalCostUsd }: Props = $props();

	let sortKey = $state<LlmSpendCallerSortKey>('cost');
	let sortDesc = $state(true);

	function setSort(key: LlmSpendCallerSortKey): void {
		if (sortKey === key) {
			sortDesc = !sortDesc;
		} else {
			sortKey = key;
			sortDesc = true;
		}
	}

	const groups = $derived.by((): LlmSpendCallerGroup[] => groupCallers(callers, sortKey, sortDesc));

	function share(cost: number): number {
		return totalCostUsd > 0 ? cost / totalCostUsd : 0;
	}
	function formatShare(cost: number): string {
		return `${(share(cost) * 100).toFixed(1)}%`;
	}

	const COLLAPSE_STORAGE_KEY = 'mike:llm-spend:caller-table:collapsed-groups';

	function loadCollapsedGroups(): Record<string, boolean> {
		try {
			const raw = localStorage.getItem(COLLAPSE_STORAGE_KEY);
			if (!raw) return {};
			const parsed: unknown = JSON.parse(raw);
			return parsed !== null && typeof parsed === 'object'
				? (parsed as Record<string, boolean>)
				: {};
		} catch {
			return {};
		}
	}

	function saveCollapsedGroups(state: Record<string, boolean>): void {
		try {
			localStorage.setItem(COLLAPSE_STORAGE_KEY, JSON.stringify(state));
		} catch {
			return;
		}
	}

	let collapsedGroups = $state<Record<string, boolean>>({});
	onMount(() => {
		collapsedGroups = loadCollapsedGroups();
	});

	function isExpanded(group: LlmSpendCallerGroup): boolean {
		return group.members.length <= 1 || collapsedGroups[group.groupKey] !== true;
	}

	function toggleGroup(groupKey: string): void {
		const next = { ...collapsedGroups, [groupKey]: !(collapsedGroups[groupKey] === true) };
		collapsedGroups = next;
		saveCollapsedGroups(next);
	}

	const COLUMNS: Array<{ key: LlmSpendCallerSortKey; label: string; tooltip: string }> = [
		{
			key: 'cost',
			label: 'Cost',
			tooltip: 'Total US dollars this caller (or group) spent through OpenRouter in the selected window.'
		},
		{
			key: 'share',
			label: 'Share',
			tooltip: "This caller's (or group's) cost as a percentage of the whole window's total spend."
		},
		{
			key: 'calls',
			label: 'Calls',
			tooltip: 'Number of OpenRouter calls this caller (or group) made in the window.'
		},
		{
			key: 'cost_per_call',
			label: 'Cost / call',
			tooltip:
				'Cost ÷ calls — the number that hides behind a role-only breakdown. For a group this is the group cost divided by the group calls, not an average of the members’ own cost/call.'
		},
		{
			key: 'avg_input',
			label: 'Avg input tokens',
			tooltip:
				'Average prompt (input) token count per call — for a group, weighted by each member’s call count.'
		},
		{
			key: 'failed',
			label: 'Failed',
			tooltip: "Calls whose outcome was neither 'ok' nor 'truncated'."
		}
	];
</script>

<section class="ct-section" aria-labelledby="ct-title">
	<h2 class="ct-title" id="ct-title">
		Callers
		<Tooltip
			text="Every distinct spender, grouped by role: Steerers (attributed and unattributed), Turn judge, Session judge, Eval runs, Lesson relations."
		>
			<span class="ct-hint" aria-hidden="true">?</span>
		</Tooltip>
	</h2>

	{#if callers.length === 0}
		<p class="ct-empty">No calls recorded in this window.</p>
	{:else}
		<div class="ct-table-wrap">
			<table class="ct-table">
				<thead>
					<tr>
						<th scope="col" class="ct-th ct-th--label">Caller</th>
						{#each COLUMNS as col (col.key)}
							<th
								scope="col"
								class="ct-th ct-th--num"
								aria-sort={sortKey === col.key ? (sortDesc ? 'descending' : 'ascending') : 'none'}
							>
								<Tooltip text={col.tooltip}>
									{#snippet child({ props })}
										<button
											type="button"
											class="ct-sort-btn"
											{...props}
											onclick={() => setSort(col.key)}
										>
											{col.label}
											<span class="ct-sort-icon" class:ct-sort-icon--active={sortKey === col.key}>
												{#if sortKey === col.key && !sortDesc}
													<ArrowUp size={11} aria-hidden="true" />
												{:else}
													<ArrowDown size={11} aria-hidden="true" />
												{/if}
											</span>
										</button>
									{/snippet}
								</Tooltip>
							</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each groups as group (group.groupKey)}
						{#if group.collapsedRow}
							{@const member = group.members[0]}
							{@const href = member ? callerHref(member) : null}
							<tr class="ct-row" class:ct-row--failing={(member?.failed_count ?? 0) > 0}>
								<td class="ct-td ct-td--label">
									{#if href}
										<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- href is one of several dynamically-built internal route shapes (steerer id, workers, judge-disagreements runs) from callerHref(); no single resolve() overload covers all of them -->
										<a class="ct-caller-link" {href}>{member?.label}</a>
									{:else}
										<span class="ct-caller-plain">{member?.label}</span>
									{/if}
								</td>
								<td class="ct-td ct-td--num">{costLabel(group.cost_usd, group.call_count, group.subscription_call_count)}</td>
								<td class="ct-td ct-td--num">{formatShare(group.cost_usd)}</td>
								<td class="ct-td ct-td--num">{callsLabel(group.call_count, group.subscription_call_count)}</td>
								<td class="ct-td ct-td--num">{perCallLabel(group.avg_cost_per_call, group.call_count, group.subscription_call_count)}</td>
								<td class="ct-td ct-td--num"
									>{Math.round(group.avg_prompt_tokens).toLocaleString()}</td
								>
								<td class="ct-td ct-td--num" class:ct-td--danger={group.failed_count > 0}
									>{group.failed_count.toLocaleString()}</td
								>
							</tr>
						{:else}
							{@const expanded = isExpanded(group)}
							<tr class="ct-row ct-row--group" class:ct-row--failing={group.failed_count > 0}>
								<td class="ct-td ct-td--label">
									{#if group.members.length > 1}
										<button
											type="button"
											class="ct-group-toggle"
											aria-expanded={expanded}
											onclick={() => toggleGroup(group.groupKey)}
										>
											{group.groupLabel}
										</button>
									{:else}
										<span class="ct-group-label">{group.groupLabel}</span>
									{/if}
								</td>
								<td class="ct-td ct-td--num">{costLabel(group.cost_usd, group.call_count, group.subscription_call_count)}</td>
								<td class="ct-td ct-td--num">{formatShare(group.cost_usd)}</td>
								<td class="ct-td ct-td--num">{callsLabel(group.call_count, group.subscription_call_count)}</td>
								<td class="ct-td ct-td--num">{perCallLabel(group.avg_cost_per_call, group.call_count, group.subscription_call_count)}</td>
								<td class="ct-td ct-td--num"
									>{Math.round(group.avg_prompt_tokens).toLocaleString()}</td
								>
								<td class="ct-td ct-td--num" class:ct-td--danger={group.failed_count > 0}
									>{group.failed_count.toLocaleString()}</td
								>
							</tr>
							{#if expanded}
								{#each group.members as member (member.key)}
									{@const href = callerHref(member)}
									<tr class="ct-row ct-row--member" class:ct-row--failing={member.failed_count > 0}>
										<td class="ct-td ct-td--label ct-td--indent">
											{#if href}
												<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- href is one of several dynamically-built internal route shapes from callerHref(); no single resolve() overload covers all of them -->
												<a class="ct-caller-link" {href}>{memberDisplayLabel(member)}</a>
											{:else}
												<span class="ct-caller-plain">{memberDisplayLabel(member)}</span>
											{/if}
										</td>
										<td class="ct-td ct-td--num">{costLabel(member.cost_usd, member.call_count, member.subscription_call_count)}</td>
										<td class="ct-td ct-td--num">{formatShare(member.cost_usd)}</td>
										<td class="ct-td ct-td--num">{callsLabel(member.call_count, member.subscription_call_count)}</td>
										<td class="ct-td ct-td--num">{perCallLabel(member.avg_cost_per_call, member.call_count, member.subscription_call_count)}</td>
										<td class="ct-td ct-td--num"
											>{Math.round(member.avg_prompt_tokens).toLocaleString()}</td
										>
										<td class="ct-td ct-td--num" class:ct-td--danger={member.failed_count > 0}
											>{member.failed_count.toLocaleString()}</td
										>
									</tr>
								{/each}
							{/if}
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>

<style>
	.ct-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.ct-title {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}
	.ct-hint {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 13px;
		height: 13px;
		border-radius: 50%;
		border: 1px solid var(--text-3);
		font-size: 9px;
		color: var(--text-3);
		cursor: help;
	}
	.ct-empty {
		margin: 0;
		font-size: 12px;
		color: var(--text-3);
	}
	.ct-table-wrap {
		overflow-x: auto;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
	}
	.ct-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
	}
	.ct-th {
		padding: 8px 10px;
		text-align: left;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
		background: var(--surface-2);
		border-bottom: 1px solid var(--border-subtle);
		white-space: nowrap;
	}
	.ct-th--num {
		text-align: right;
	}
	.ct-sort-btn {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		font-weight: inherit;
		text-transform: inherit;
		letter-spacing: inherit;
		color: inherit;
		cursor: pointer;
	}
	.ct-sort-btn:hover {
		color: var(--text-1);
	}
	.ct-sort-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.ct-sort-icon {
		display: inline-flex;
		width: 11px;
		height: 11px;
		opacity: 0;
	}
	.ct-sort-icon--active {
		opacity: 1;
	}
	.ct-row {
		border-bottom: 1px solid var(--border-subtle);
	}
	.ct-row:last-child {
		border-bottom: none;
	}
	.ct-row--group {
		background: var(--surface-2);
	}
	.ct-row--failing {
		background: color-mix(in oklch, var(--status-danger-bg) 40%, transparent);
	}
	.ct-td {
		padding: 7px 10px;
		color: var(--text-1);
		font-variant-numeric: tabular-nums;
		vertical-align: middle;
	}
	.ct-row--group .ct-td {
		font-weight: 600;
		color: var(--text-strong);
	}
	.ct-td--label {
		font-variant-numeric: normal;
		overflow-wrap: anywhere;
	}
	.ct-td--indent {
		padding-left: 24px;
	}
	.ct-td--num {
		text-align: right;
	}
	.ct-td--danger {
		color: var(--status-danger-fg);
		font-weight: 600;
	}
	.ct-group-toggle {
		background: transparent;
		border: 0;
		padding: 0;
		font: inherit;
		font-weight: inherit;
		color: inherit;
		cursor: pointer;
		text-align: left;
	}
	.ct-group-toggle:hover {
		color: var(--funk-cyan);
	}
	.ct-group-toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.ct-group-label {
		font-weight: 600;
	}
	.ct-caller-link {
		color: var(--funk-cyan);
		text-decoration: none;
	}
	.ct-caller-link:hover {
		text-decoration: underline;
	}
	.ct-caller-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-radius: 2px;
	}
	.ct-caller-plain {
		color: var(--text-2);
	}
</style>
