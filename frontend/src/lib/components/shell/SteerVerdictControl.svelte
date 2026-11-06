<script lang="ts">
	import { labelEvaluation } from '$lib/api/steerQuestions';

	let { evaluationUuid }: { evaluationUuid: string } = $props();

	let note = $state('');
	let saved = $state<'right' | 'wrong' | null>(null);
	let saveError = $state<string | null>(null);

	async function mark(label: 'right' | 'wrong'): Promise<void> {
		try {
			await labelEvaluation(evaluationUuid, label, note.trim() || undefined);
			saved = label;
			saveError = null;
		} catch (e) {
			saveError = e instanceof Error ? e.message : String(e);
		}
	}
</script>

<span class="inline-flex flex-wrap items-center gap-2" data-steer-verdict={saved ?? 'unset'}>
	<button type="button" class="text-micro font-semibold" aria-pressed={saved === 'right'} onclick={() => void mark('right')}
		>Right</button
	>
	<button type="button" class="text-micro font-semibold" aria-pressed={saved === 'wrong'} onclick={() => void mark('wrong')}
		>Wrong</button
	>
	<input
		class="border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
		placeholder="Note (optional)"
		aria-label="Note on this steer"
		bind:value={note}
	/>
	{#if saved === 'wrong'}
		<span class="text-micro text-[var(--text-3)]">A pending wake for this fire is cancelled.</span>
	{/if}
	{#if saveError}
		<span class="text-micro text-[var(--text-3)]" role="alert">{saveError}</span>
	{/if}
</span>
