<script lang="ts">
	import { CircleCheck, CircleX } from '@lucide/svelte';
	import type { CodeProjectRules } from '$lib/api/codeFindings';
	import { Root as DataTable, Row, Cell } from '$lib/components/ui/data-table';
	import type { DataTableColumn } from '$lib/components/ui/data-table';
	import { StatusMark } from '$lib/components/ui/status-mark';

	interface Props {
		watched: string[];
		projectRules: CodeProjectRules[];
	}

	let { watched, projectRules }: Props = $props();

	const COLUMNS: DataTableColumn[] = [
		{ key: 'checkout', label: 'Checkout' },
		{ key: 'files', label: 'Rule files' },
		{ key: 'rules', label: 'Rules' },
		{ key: 'version', label: 'Version' },
		{ key: 'status', label: 'Status' }
	];

	interface CheckoutRow {
		checkout: string;
		rules: CodeProjectRules | null;
	}

	const rows = $derived.by((): CheckoutRow[] => {
		const byCheckout = new Map(projectRules.map((entry) => [entry.checkout, entry]));
		const joined = watched.map((checkout) => ({
			checkout,
			rules: byCheckout.get(checkout) ?? null
		}));
		const seen = new Set(watched);
		for (const entry of projectRules) {
			if (!seen.has(entry.checkout)) joined.push({ checkout: entry.checkout, rules: entry });
		}
		return joined;
	});
</script>

<DataTable label="Checkouts and project rules" columns={COLUMNS}>
	<tbody>
		{#each rows as row (row.checkout)}
			{@const rules = row.rules}
			<Row data-checkout={row.checkout}>
				<Cell mono>{row.checkout}</Cell>
				<Cell mono>
					{#if rules && rules.files.length > 0}
						{#each rules.files as file (file)}
							<div>{file}</div>
						{/each}
					{:else}
						—
					{/if}
				</Cell>
				<Cell mono>{rules && rules.rules.length > 0 ? rules.rules.join(', ') : '—'}</Cell>
				<Cell mono tone={rules?.version ? undefined : 'quiet'}>{rules?.version ?? '—'}</Cell>
				<Cell>
					{#if rules?.error}
						<StatusMark icon={CircleX} label="error" tone="danger" />
					{:else if rules}
						<StatusMark icon={CircleCheck} label="loaded" tone="success" />
					{:else}
						<span class="crt-none">no project rules</span>
					{/if}
				</Cell>
			</Row>
			{#if rules?.error}
				<tr data-slot="checkout-rules-error">
					<Cell colspan={COLUMNS.length}>
						<pre class="crt-error">{rules.error}</pre>
					</Cell>
				</tr>
			{/if}
		{/each}
	</tbody>
</DataTable>

<style>
	.crt-none {
		color: var(--text-3);
	}
	.crt-error {
		margin: 0;
		color: var(--status-danger-fg);
		font-family: var(--font-mono, monospace);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
