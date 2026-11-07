<script lang="ts">
	import {
		reviewCompression,
		type CompressionReview,
		type ReviewStatus
	} from '$lib/api/compressionReview';
	import { Button } from '$lib/components/ui/button';
	import { Field } from '$lib/components/ui/field';
	import { Select } from '$lib/components/ui/select';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	let { item, onSaved }: { item: CompressionReview; onSaved: () => void } = $props();
	let status = $state('');
	let note = $state('');
	let busy = $state(false);
	let error = $state('');
	const options = ['open', 'investigating', 'evaluated', 'dismissed'].map((value) => ({
		value,
		label: value
	}));
	$effect(() => {
		status = item.status;
	});
	const bodyHref = $derived(
		`/api/requests/${item.target.request_id}/body/${item.target.kind === 'response' ? 'response' : 'request'}`
	);
	async function save(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = '';
		try {
			await reviewCompression(item.id, status as ReviewStatus, note.trim());
			note = '';
			onSaved();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Could not save review.';
		} finally {
			busy = false;
		}
	}
</script>

<article
	class="flex flex-col gap-3 border border-[var(--border-subtle)] bg-[var(--surface-card)] p-4"
>
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h2 class="font-semibold">
			{item.target.kind.replace('_', ' ')} · request #{item.target
				.request_id}{#if item.target.tool_use_id}
				· tool #{item.target.tool_use_id}{/if}
		</h2>
		<span>{item.status}</span>
	</div>
	<Button variant="outline" size="sm" href={bodyHref} target="_blank" rel="noreferrer"
		>Original capture</Button
	>
	<details>
		<summary class="cursor-pointer">Evidence and history ({item.history.length})</summary>
		<ol class="flex flex-col gap-3 py-3">
			{#each item.history as event, index (index)}
				<li class="whitespace-pre-wrap break-words">
					<p>{event.actor} · {event.at}</p>
					<p>{event.reason ?? event.note}</p>
					{#if event.evidence}<p>{event.evidence}</p>{/if}{#if event.status}<p>
							Status: {event.status}
						</p>{/if}
				</li>
			{/each}
		</ol>
	</details>
	<form onsubmit={save} class="flex flex-col gap-3">
		<Select label="Review status" {options} bind:value={status} />
		<Field label="Review note" multiline bind:value={note} required />
		{#if error}<LoadErrorBanner title="Review was not saved" detail={error} />{/if}
		<Button type="submit" disabled={busy || !note.trim()}>{busy ? 'Saving' : 'Save review'}</Button>
	</form>
</article>
