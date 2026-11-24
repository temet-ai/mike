<!--
@component
Merge into: the operator picks which of the selected causes keeps the others, and reads what Separate
can and cannot undo before confirming. The causes arrive in rank order and the first one is
chosen to begin with. It closes when the merge worked.
-->
<script lang="ts">
	import type { WrightCause } from '$lib/api/wright';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import { problemsText } from '$lib/wright/causes';

	interface Props {
		open: boolean;
		/** The selected causes, in rank order. */
		causes: WrightCause[];
		busy?: boolean;
		/** The server's reason when the last try failed. */
		error?: string | null;
		/** Absorbs `ids` into `rootId`; true when it worked and the dialog may close. */
		onConfirm: (rootId: number, ids: number[]) => Promise<boolean>;
	}

	let { open = $bindable(false), causes, busy = false, error = null, onConfirm }: Props = $props();

	let picked = $state('');
	// A fresh pick each time the dialog opens: the highest-ranked selected cause keeps the rest.
	$effect(() => {
		if (open) picked = String(causes[0]?.cause_id ?? '');
	});

	const root = $derived(causes.find((c) => String(c.cause_id) === picked));
	const others = $derived(causes.filter((c) => String(c.cause_id) !== picked));
	const moving = $derived(others.reduce((sum, c) => sum + c.problems, 0));

	async function confirm(): Promise<void> {
		if (root === undefined) return;
		const ids = others.map((c) => c.cause_id);
		if (await onConfirm(root.cause_id, ids)) open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>Merge {causes.length} causes</Dialog.Title>
			<Dialog.Description>
				Merging says these are one fault. The cause you pick keeps the others: their problems move
				into it and they leave the queue.
			</Dialog.Description>
		</Dialog.Header>
		<span class="machine merge__label" id="merge-into-label">Merge into</span>
		<RadioGroup.Root name="merge-into" bind:value={picked} aria-labelledby="merge-into-label">
			{#each causes as cause (cause.cause_id)}
				<RadioGroup.Item
					value={String(cause.cause_id)}
					label={cause.check_name}
					description="{cause.resource} · {problemsText(cause.problems)}"
				/>
			{/each}
		</RadioGroup.Root>
		<p class="merge__note">
			{#if root !== undefined}
				{problemsText(moving)} move into {root.check_name}.
			{/if}
			Separate on the kept cause moves those problems back. Problems opened after the merge stay with
			it, and a merge Wright's engineer makes cannot be separated.
		</p>
		<p class="merge__error" role="alert">{error ?? ''}</p>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
			<Button disabled={busy || root === undefined || others.length === 0} onclick={() => void confirm()}>
				Merge
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.merge__label {
		color: var(--text-2);
		font-size: var(--text-xs);
	}

	.merge__note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}

	/* The line is reserved, so a refusal appearing does not move the buttons. */
	.merge__error {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
