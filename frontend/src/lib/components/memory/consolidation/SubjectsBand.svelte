<script lang="ts">
	import {
		DataTable,
		DataTableCell,
		DataTableRow,
		type DataTableColumn
	} from '$lib/components/ui/data-table';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import type { SubjectSummary } from '$lib/api/knowledgeConsolidation';
	import SubjectLink from './SubjectLink.svelte';
	import { VERDICTS, formatFull, formatWhen } from './labels';

	interface Props {
		subjects: SubjectSummary[];
		project: string;
	}

	const { subjects, project }: Props = $props();

	const COLUMNS: DataTableColumn[] = [
		{ key: 'subject', label: 'Subject' },
		{
			key: 'statements',
			label: 'Statements',
			align: 'end',
			tooltip: 'Statements in force in this subject.'
		},
		{
			key: 'items',
			label: 'Items',
			align: 'end',
			tooltip: 'Lessons and files those statements sit in.'
		},
		{ key: 'decision', label: 'Last decision' },
		{ key: 'judged', label: 'Judged' }
	];
</script>

<DataTable label="Subjects" columns={COLUMNS}>
	<tbody>
		{#each subjects as s (s.id)}
			<DataTableRow>
				<DataTableCell><SubjectLink id={s.id} title={s.title} {project} /></DataTableCell>
				<DataTableCell align="end" mono tone="quiet">
					{s.members.toLocaleString('en-US')}
				</DataTableCell>
				<DataTableCell align="end" mono tone="quiet">
					{s.carriers.toLocaleString('en-US')}
				</DataTableCell>
				<DataTableCell tone="quiet">
					{#if s.newest_verdict && s.newest_decision_at}
						<Tooltip text={formatFull(s.newest_decision_at)}>
							<span>{VERDICTS[s.newest_verdict].word} · {formatWhen(s.newest_decision_at)}</span>
						</Tooltip>
					{:else}
						—
					{/if}
				</DataTableCell>
				<DataTableCell tone="quiet" mono>
					{#if s.changed_at}
						<Tooltip text="Changed since it was last judged. Judged after the settle delay.">
							<span>waiting</span>
						</Tooltip>
					{:else}
						{formatWhen(s.judged_at)}
					{/if}
				</DataTableCell>
			</DataTableRow>
		{/each}
	</tbody>
</DataTable>
