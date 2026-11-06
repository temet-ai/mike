<script lang="ts">
	import { flagCompression, type CompressionTarget } from '$lib/api/compressionReview';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Field } from '$lib/components/ui/field';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	let { target }: { target: CompressionTarget } = $props();
	let open = $state(false);
	let reason = $state('');
	let evidence = $state('');
	let busy = $state(false);
	let saved = $state(false);
	let error = $state('');
	const key = $derived(`${target.kind}:${target.request_id}:${target.tool_use_id ?? ''}`);
	$effect(() => {
		key;
		open = false;
		saved = false;
		busy = false;
		reason = '';
		evidence = '';
		error = '';
	});
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const savingKey = key;
		busy = true;
		error = '';
		try {
			await flagCompression(target, reason.trim(), evidence.trim());
			if (key === savingKey) {
				saved = true;
				open = false;
			}
		} catch (e) {
			if (key === savingKey)
				error = e instanceof Error ? e.message : 'Could not flag this capture.';
		} finally {
			if (key === savingKey) busy = false;
		}
	}
</script>

<div class="flex flex-wrap items-center gap-2">
	<Button variant="outline" size="sm" onclick={() => (open = true)}
		>{saved ? 'Flagged for review' : 'Flag for compression'}</Button
	>
	{#if saved}<Button variant="ghost" size="sm" href="/compression-review">Review queue</Button>{/if}
</div>
<Dialog.Root bind:open>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>Investigate compression</Dialog.Title>
			<Dialog.Description
				>Flag this {target.kind.replace('_', ' ')} for investigation. Original content stays intact.</Dialog.Description
			>
		</Dialog.Header>
		<form onsubmit={submit} class="flex flex-col gap-4">
			<Field
				label="Reason"
				multiline
				bind:value={reason}
				required
				placeholder="What could be represented more precisely?"
			/>
			<Field
				label="Evidence"
				multiline
				bind:value={evidence}
				required
				placeholder="Relevant facts, follow-up calls or constraints to preserve"
			/>
			{#if error}<LoadErrorBanner title="Flag was not saved" detail={error} />{/if}
			<Dialog.Footer
				><Button type="submit" disabled={busy || !reason.trim() || !evidence.trim()}
					>{busy ? 'Saving' : 'Save flag'}</Button
				></Dialog.Footer
			>
		</form>
	</Dialog.Content>
</Dialog.Root>
