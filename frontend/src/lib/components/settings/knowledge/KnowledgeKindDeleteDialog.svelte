<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { deleteKnowledgeKind, kindSource, type KnowledgeKind } from '$lib/api/knowledgeKinds';
	import { formatCount } from '$lib/time';

	interface Props {
		kind: KnowledgeKind;
		onClose: () => void;
		onDeleted: (kind: string) => void;
	}

	const { kind, onClose, onDeleted }: Props = $props();

	let busy = $state(false);
	let error = $state<string | null>(null);
	let cancelRef = $state<HTMLButtonElement | null>(null);

	async function remove(): Promise<void> {
		busy = true;
		error = null;
		try {
			await deleteKnowledgeKind(kind.kind);
			onDeleted(kind.kind);
		} catch (err) {
			error = err instanceof Error ? err.message : String(err);
		} finally {
			busy = false;
		}
	}
</script>

<Dialog.Root
	open
	onOpenChange={(open) => {
		if (!open && !busy) onClose();
	}}
>
	<Dialog.Content
		class="sm:max-w-md"
		showCloseButton={false}
		onOpenAutoFocus={(event) => {
			event.preventDefault();
			cancelRef?.focus();
		}}
	>
		<Dialog.Header>
			<Dialog.Title>Delete {kind.label}?</Dialog.Title>
			<Dialog.Description>
				Removes its {formatCount(kind.stats.items)} items and {formatCount(kind.stats.embedded)}
				embeddings. Rows in <code class="kk-code">{kindSource(kind)}</code> are not touched.
			</Dialog.Description>
		</Dialog.Header>
		{#if error !== null}
			<p class="kk-alert" role="alert">{error}</p>
		{/if}
		<Dialog.Footer>
			<Button variant="ghost" bind:ref={cancelRef} disabled={busy} onclick={onClose}>Cancel</Button>
			<Button variant="destructive" disabled={busy} onclick={() => void remove()}>
				{busy ? 'Deleting' : 'Delete'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.kk-code {
		font-family: var(--font-mono);
		overflow-wrap: anywhere;
	}
	.kk-alert {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
