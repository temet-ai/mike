<script lang="ts">
	import { SvelteMap } from 'svelte/reactivity';
	import type { CodeRuleInfo } from '$lib/api/codeFindings';
	import { Root as DataTable, Row, Cell, GroupRow } from '$lib/components/ui/data-table';
	import type { DataTableColumn } from '$lib/components/ui/data-table';
	import { StatusMark } from '$lib/components/ui/status-mark';
	import { COLUMN_TOOLTIPS, severityMark } from './codeFindingsFormat';

	interface Props {
		catalogue: CodeRuleInfo[];
	}

	let { catalogue }: Props = $props();

	const COLUMNS: DataTableColumn[] = [
		{ key: 'rule', label: 'Rule' },
		{ key: 'severity', label: 'Severity' },
		{ key: 'source', label: 'Source' },
		{ key: 'replaces', label: 'Replaces', tooltip: COLUMN_TOOLTIPS.replaces }
	];

	const groups = $derived.by(() => {
		const byLanguage = new SvelteMap<string, CodeRuleInfo[]>();
		for (const rule of catalogue) {
			const list = byLanguage.get(rule.language);
			if (list) list.push(rule);
			else byLanguage.set(rule.language, [rule]);
		}
		return [...byLanguage.entries()].map(([language, rules]) => ({
			language,
			rules: [...rules].sort((a, b) => a.id.localeCompare(b.id))
		}));
	});
</script>

<DataTable label="Rule catalogue" columns={COLUMNS}>
	<tbody>
		{#each groups as group (group.language)}
			<GroupRow colspan={COLUMNS.length} label="{group.language} · {group.rules.length}" />
			{#each group.rules as rule (rule.id)}
				{@const mark = severityMark(rule.severity)}
				<Row>
					<Cell mono>{rule.id}</Cell>
					<Cell><StatusMark icon={mark.icon} label={mark.label} tone={mark.tone} /></Cell>
					<Cell tone="quiet">{rule.source}</Cell>
					{#if rule.replaces}
						<Cell mono>{rule.replaces}</Cell>
					{:else}
						<Cell tone="quiet">—</Cell>
					{/if}
				</Row>
			{/each}
		{/each}
	</tbody>
</DataTable>
