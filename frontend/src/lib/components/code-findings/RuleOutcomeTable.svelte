<script lang="ts">
	import type { CodeRuleCount, CodeRuleInfo } from '$lib/api/codeFindings';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Root as DataTable, Row, Cell } from '$lib/components/ui/data-table';
	import type { DataTableColumn } from '$lib/components/ui/data-table';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import {
		COLUMN_TOOLTIPS,
		fixRate,
		formatLag,
		severityMark,
		sortRules,
		totalRules
	} from './codeFindingsFormat';

	interface Props {
		rules: CodeRuleCount[];
		catalogue: CodeRuleInfo[] | null;
	}

	let { rules, catalogue }: Props = $props();

	const COLUMNS: DataTableColumn[] = [
		{ key: 'rule', label: 'Rule' },
		{ key: 'found', label: 'Found', align: 'end', tooltip: COLUMN_TOOLTIPS.found },
		{ key: 'told', label: 'Told', align: 'end', tooltip: COLUMN_TOOLTIPS.told },
		{ key: 'pushed', label: 'Pushed', align: 'end', tooltip: COLUMN_TOOLTIPS.pushed },
		{
			key: 'fixed_after_told',
			label: 'Fixed after told',
			align: 'end',
			tooltip: COLUMN_TOOLTIPS.fixed_after_told
		},
		{
			key: 'fixed_untold',
			label: 'Fixed untold',
			align: 'end',
			tooltip: COLUMN_TOOLTIPS.fixed_untold
		},
		{
			key: 'fixed_by_other',
			label: 'By other',
			align: 'end',
			tooltip: COLUMN_TOOLTIPS.fixed_by_other
		},
		{ key: 'ignored', label: 'Ignored', align: 'end', tooltip: COLUMN_TOOLTIPS.ignored },
		{ key: 'open', label: 'Open', align: 'end', tooltip: COLUMN_TOOLTIPS.open },
		{ key: 'fix_rate', label: 'Fix rate', align: 'end', tooltip: COLUMN_TOOLTIPS.fix_rate },
		{ key: 'told_after', label: 'Told after', align: 'end', tooltip: COLUMN_TOOLTIPS.told_after }
	];

	const rows = $derived(sortRules(rules));
	const totals = $derived(totalRules(rules));
	const severityById = $derived(new Map((catalogue ?? []).map((rule) => [rule.id, rule.severity])));

	function severityFor(ruleId: string) {
		const severity = severityById.get(ruleId);
		return severity ? severityMark(severity) : null;
	}
</script>

{#snippet number(value: number)}
	<Cell align="end" mono tone={value === 0 ? 'quiet' : undefined}
		>{value.toLocaleString('en-US')}</Cell
	>
{/snippet}

<DataTable label="Rules that fired" columns={COLUMNS}>
	<tbody>
		{#each rows as rule (rule.rule_id)}
			{@const mark = severityFor(rule.rule_id)}
			<Row data-rule={rule.rule_id}>
				<Cell mono>
					<span class="rot-rule">
						{#if mark}
							<StatusMark icon={mark.icon} label={mark.label} tone={mark.tone} />
						{/if}
						<span>{rule.rule_id}</span>
						{#if rule.source === 'project'}
							<Tooltip text={COLUMN_TOOLTIPS.project}>
								{#snippet child({ props })}
									<button {...props} type="button" class="rot-project">project</button>
								{/snippet}
							</Tooltip>
						{/if}
					</span>
				</Cell>
				{@render number(rule.found)}
				{@render number(rule.delivered)}
				{@render number(rule.pushed)}
				{@render number(rule.fixed_after_told)}
				{@render number(rule.fixed_untold)}
				{@render number(rule.fixed_by_other)}
				{@render number(rule.ignored)}
				{@render number(rule.open)}
				<Cell align="end" mono>{fixRate(rule.fixed_after_told, rule.delivered)}</Cell>
				<Cell align="end" mono>{formatLag(rule.median_lag_s)}</Cell>
			</Row>
		{/each}
	</tbody>
	<tfoot>
		<Row data-rule="all">
			<Cell>All rules</Cell>
			{@render number(totals.found)}
			{@render number(totals.delivered)}
			{@render number(totals.pushed)}
			{@render number(totals.fixed_after_told)}
			{@render number(totals.fixed_untold)}
			{@render number(totals.fixed_by_other)}
			{@render number(totals.ignored)}
			{@render number(totals.open)}
			<Cell align="end" mono>{fixRate(totals.fixed_after_told, totals.delivered)}</Cell>
			<Cell align="end" mono tone="quiet">—</Cell>
		</Row>
	</tfoot>
</DataTable>

<style>
	.rot-rule {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		white-space: nowrap;
	}
	.rot-project {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--text-3);
		cursor: default;
	}
	.rot-project:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
</style>
