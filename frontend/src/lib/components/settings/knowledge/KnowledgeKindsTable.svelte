<script lang="ts">
	import { CircleCheck, CircleX, Hourglass, Lock, Trash2 } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Tooltip from '$lib/components/ui/tooltip';
	import { Switch } from '$lib/components/ui/switch';
	import StatusMark from '$lib/components/ui/status-mark/status-mark.svelte';
	import {
		DataTable,
		DataTableCell,
		DataTableRow,
		type DataTableColumn
	} from '$lib/components/ui/data-table';
	import { kindSource, sortKinds, type KnowledgeKind } from '$lib/api/knowledgeKinds';
	import { formatCount, formatRelative } from '$lib/time';

	interface Props {
		kinds: KnowledgeKind[];
		/** Kinds whose switch has a save in flight; their switch is disabled. */
		saving?: readonly string[];
		onToggle: (kind: string, enabled: boolean) => void;
		onTune: (kind: string) => void;
		onDelete: (kind: string) => void;
	}

	const { kinds, saving = [], onToggle, onTune, onDelete }: Props = $props();

	const COLUMNS: DataTableColumn[] = [
		{ key: 'kind', label: 'Kind' },
		{ key: 'source', label: 'Source' },
		{ key: 'items', label: 'Items', align: 'end' },
		{
			key: 'embedded',
			label: 'Embedded',
			align: 'end',
			tooltip: 'Items with a current embedding.'
		},
		{
			key: 'waiting',
			label: 'Waiting',
			align: 'end',
			tooltip: 'Changed items the next sweep embeds, once their settle time has passed.'
		},
		{
			key: 'failed',
			label: 'Failed',
			align: 'end',
			tooltip: 'Items that failed to embed. Open to read the newest error.'
		},
		{ key: 'settle', label: 'Settle', align: 'end' },
		{ key: 'on', label: 'Embedding' },
		{ key: 'actions', label: 'Actions' }
	];

	const rows = $derived(sortKinds(kinds));
	let errorKind = $state<string | null>(null);
	const errorRow = $derived(rows.find((k) => k.kind === errorKind) ?? null);

	function waitingNote(k: KnowledgeKind): string {
		const ready = k.stats.oldest_waiting_ready_at;
		if (ready === null) return '—';
		return Date.parse(ready) > Date.now() ? 'settling' : `since ${formatRelative(ready)}`;
	}
</script>

<div class="kk-table">
	<DataTable label="Knowledge sources" columns={COLUMNS}>
		<tbody>
			{#each rows as k (k.kind)}
				<DataTableRow>
					<DataTableCell wrap={false}>
						<div class="kk-two kk-kind" data-off={k.enabled ? undefined : ''}>
							<Tooltip.Root>
								<Tooltip.Trigger class="kk-tip">
									{#if k.built_in}<Lock size={12} aria-hidden="true" />{/if}
									<span class="kk-label">{k.label}</span>
								</Tooltip.Trigger>
								<Tooltip.Content side="top">
									{k.built_in
										? 'Built in. It can be switched off and tuned, not deleted.'
										: k.description || k.label}
									{#if k.built_in && k.description}<br />{k.description}{/if}
								</Tooltip.Content>
							</Tooltip.Root>
							<span class="kk-sub">{k.kind}</span>
						</div>
					</DataTableCell>
					<DataTableCell mono tone="quiet">{kindSource(k)}</DataTableCell>
					<DataTableCell align="end" mono class="kk-count"
						>{formatCount(k.stats.items)}</DataTableCell
					>
					<DataTableCell align="end" class="kk-count">
						<div class="kk-two kk-two--end">
							<StatusMark
								icon={CircleCheck}
								label={formatCount(k.stats.embedded)}
								tone={k.stats.embedded === k.stats.items && k.stats.items > 0 ? 'success' : 'quiet'}
							/>
							<span class="kk-sub">&nbsp;</span>
						</div>
					</DataTableCell>
					<DataTableCell align="end" class="kk-count">
						<div class="kk-two kk-two--end">
							<StatusMark
								icon={Hourglass}
								label={formatCount(k.stats.waiting)}
								tone={k.stats.waiting === 0 ? 'quiet' : k.behind ? 'warning' : 'info'}
							/>
							<span class="kk-sub">{waitingNote(k)}</span>
						</div>
					</DataTableCell>
					<DataTableCell align="end" class="kk-count">
						<div class="kk-two kk-two--end">
							<Button
								variant="ghost"
								size="sm"
								disabled={k.stats.failed === 0}
								aria-label="Read the newest error for {k.label}"
								onclick={() => (errorKind = k.kind)}
							>
								<StatusMark
									icon={CircleX}
									label={formatCount(k.stats.failed)}
									tone={k.stats.failed > 0 ? 'danger' : 'quiet'}
								/>
							</Button>
							<span class="kk-sub">{formatRelative(k.stats.newest_error_at)}</span>
						</div>
					</DataTableCell>
					<DataTableCell align="end" mono>{k.settle_secs} s</DataTableCell>
					<DataTableCell wrap={false}>
						<div class="kk-switch">
							<Switch
								checked={k.enabled}
								disabled={saving.includes(k.kind)}
								aria-label="Embed {k.label}"
								onCheckedChange={(next) => onToggle(k.kind, next)}
							/>
							<span class="kk-state">{k.enabled ? 'On' : 'Off'}</span>
						</div>
					</DataTableCell>
					<DataTableCell wrap={false}>
						<div class="kk-actions">
							<Button variant="ghost" size="sm" onclick={() => onTune(k.kind)}
								>Tune<span class="sr-only"> {k.label}</span></Button
							>
							<span class="kk-slot">
								{#if !k.built_in}
									<Button
										variant="ghost"
										size="icon"
										aria-label="Delete {k.label}"
										onclick={() => onDelete(k.kind)}
									>
										<Trash2 aria-hidden="true" />
									</Button>
								{/if}
							</span>
						</div>
					</DataTableCell>
				</DataTableRow>
			{/each}
		</tbody>
	</DataTable>
</div>

<Dialog.Root
	open={errorRow !== null}
	onOpenChange={(open) => {
		if (!open) errorKind = null;
	}}
>
	<Dialog.Content class="sm:max-w-2xl" showCloseButton={false}>
		{#if errorRow}
			<Dialog.Header>
				<Dialog.Title>Newest error · {errorRow.label}</Dialog.Title>
				<Dialog.Description>
					{formatCount(errorRow.stats.failed)} failed · {formatRelative(
						errorRow.stats.newest_error_at
					)}
				</Dialog.Description>
			</Dialog.Header>
			<pre class="kk-error">{errorRow.stats.newest_error ?? '—'}</pre>
		{/if}
		<Dialog.Footer>
			<Dialog.Close>
				{#snippet child({ props })}
					<Button variant="outline" {...props}>Close</Button>
				{/snippet}
			</Dialog.Close>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.kk-table :global(.kk-count) {
		min-inline-size: 9ch;
	}
	.kk-two {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-block-size: 2.6em;
	}
	.kk-two--end {
		align-items: flex-end;
	}
	.kk-sub {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
	.kk-kind :global(.kk-tip) {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		padding: 0;
		border: 0;
		background: transparent;
		font: inherit;
		text-align: start;
		color: var(--text-strong);
		cursor: default;
	}
	.kk-kind :global(.kk-tip:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.kk-kind[data-off] :global(.kk-tip) {
		color: var(--text-3);
	}
	.kk-kind :global(.kk-tip svg) {
		color: var(--text-3);
	}
	.kk-switch {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}
	.kk-state {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.kk-actions {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
	.kk-slot {
		display: inline-block;
		inline-size: var(--space-6);
		block-size: var(--space-6);
	}
	.kk-error {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
