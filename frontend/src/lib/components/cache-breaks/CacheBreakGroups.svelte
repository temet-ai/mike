<script lang="ts">
	import type { CacheBreakGroup } from '$lib/api/cacheBreaks';
	import { Root as DataTable, Row, Cell } from '$lib/components/ui/data-table';
	import type { DataTableColumn } from '$lib/components/ui/data-table';
	import { formatTokens, groupKey, groupLabel, orderGroups } from './cacheBreakFormat';

	interface Props {
		groups: CacheBreakGroup[];
		showSessions?: boolean;
	}

	let { groups, showSessions = false }: Props = $props();

	const columns = $derived<DataTableColumn[]>([
		{ key: 'cause', label: 'Cause' },
		{ key: 'breaks', label: 'Breaks', align: 'end' },
		...(showSessions ? [{ key: 'sessions', label: 'Sessions', align: 'end' as const }] : []),
		{ key: 'tokens', label: 'Tokens rewritten', align: 'end' }
	]);

	const rows = $derived(orderGroups(groups));
</script>

<DataTable label="Cache breaks by cause" {columns}>
	<tbody>
		{#each rows as group (groupKey(group))}
			<Row data-cause={group.cause}>
				<Cell mono>{groupLabel(group)}</Cell>
				<Cell align="end" mono>{group.breaks.toLocaleString('en-US')}</Cell>
				{#if showSessions}
					<Cell align="end" mono>{group.sessions.toLocaleString('en-US')}</Cell>
				{/if}
				<Cell align="end" mono>{formatTokens(group.tokens_rewritten)}</Cell>
			</Row>
		{/each}
	</tbody>
</DataTable>
