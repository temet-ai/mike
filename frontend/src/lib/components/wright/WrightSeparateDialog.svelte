<!--
@component
Asks to confirm Separate on a merge the operator made. It says what comes back and what stays: the
problems that moved with the merge go back to the absorbed cause; problems opened after the merge
stay where Wright's assignment already sent them. It closes when the action worked.
-->
<script lang="ts">
	import type { AbsorbedCause } from '$lib/api/wright';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { problemsText } from '$lib/wright/causes';

	interface Props {
		open: boolean;
		/** The merge to undo; null while nothing is asked. */
		merge: AbsorbedCause | null;
		/** The cause that kept it, named in the text. */
		keptBy: string;
		busy?: boolean;
		/** The server's reason when the last try failed. */
		error?: string | null;
		/** True when the merge was separated: the dialog closes. */
		onConfirm: (mergeId: number) => Promise<boolean>;
	}

	let { open = $bindable(false), merge, keptBy, busy = false, error = null, onConfirm }: Props =
		$props();

	async function confirm(): Promise<void> {
		if (merge === null) return;
		if (await onConfirm(merge.merge_id)) open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>Separate this merge</Dialog.Title>
			<Dialog.Description>
				{#if merge !== null}
					{merge.check_name} comes back as its own cause, with the {problemsText(merge.problems)} that
					moved when you merged it. Problems opened after the merge stay with {keptBy}, because Wright
					had already sent them there.
				{/if}
			</Dialog.Description>
		</Dialog.Header>
		<p class="sep__error" role="alert">{error ?? ''}</p>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
			<Button disabled={busy || merge === null} onclick={() => void confirm()}>Separate</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	/* The line is reserved, so a refusal appearing does not move the buttons. */
	.sep__error {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
