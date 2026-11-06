<script lang="ts">
	import { onMount } from 'svelte';
	import {
		dismissCandidate,
		listCandidates,
		medoidText,
		nameCandidate,
		validSlug
	} from '$lib/api/semantic';
	import type { CandidateEntry } from '$lib/api/semantic';

	let entries = $state<CandidateEntry[]>([]);
	let loadError = $state<string | null>(null);
	let expanded = $state<Record<number, boolean>>({});
	let slug = $state<Record<number, string>>({});
	let description = $state<Record<number, string>>({});
	let note = $state<Record<number, string>>({});
	let rowError = $state<Record<number, string | null>>({});

	async function load(): Promise<void> {
		try {
			entries = (await listCandidates('open')).candidates;
			loadError = null;
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
		}
	}

	async function name(id: number): Promise<void> {
		const s = slug[id] ?? '';
		if (!validSlug(s) || !(description[id] ?? '').trim()) {
			rowError[id] = 'A name is 3 to 64 lowercase letters, digits or dashes, with a description.';
			return;
		}
		try {
			await nameCandidate(id, s, (description[id] ?? '').trim());
			rowError[id] = null;
			await load();
		} catch (e) {
			rowError[id] = e instanceof Error ? e.message : String(e);
		}
	}

	async function dismiss(id: number): Promise<void> {
		try {
			await dismissCandidate(id, note[id] ?? '');
			rowError[id] = null;
			await load();
		} catch (e) {
			rowError[id] = e instanceof Error ? e.message : String(e);
		}
	}

	onMount(() => {
		void load();
	});
</script>

<section class="flex flex-col gap-3" data-testid="pattern-candidates">
	<h2 class="text-sm font-semibold text-[var(--text-strong)]">Possible new failure patterns</h2>
	{#if loadError}
		<p class="text-micro text-[var(--text-3)]" role="alert">Could not load candidates: {loadError}</p>
	{:else if entries.length === 0}
		<p class="text-micro text-[var(--text-3)]">No open candidates.</p>
	{:else}
		{#each entries as entry (entry.candidate.id)}
			{@const c = entry.candidate}
			<article class="flex flex-col gap-2 border border-[var(--border-subtle)] p-3" data-candidate-id={c.id}>
				<p class="text-micro text-[var(--text-3)]">
					{c.member_count} corrections in {c.session_count} sessions, {c.window_start.split('T')[0]} to {c.window_end.split('T')[0]}
				</p>
				<p class="whitespace-pre-wrap text-sm text-[var(--text-strong)]">{medoidText(entry)}</p>
				<button
					type="button"
					class="self-start text-micro text-[var(--text-faint)] hover:text-[var(--text-strong)]"
					aria-expanded={expanded[c.id] ?? false}
					onclick={() => (expanded[c.id] = !(expanded[c.id] ?? false))}>Every member</button
				>
				{#if expanded[c.id]}
					<ol class="flex flex-col gap-2">
						{#each entry.members as m (m.item_id)}
							<li class="whitespace-pre-wrap text-micro text-[var(--text-2)]" data-member-id={m.item_id}>
								{m.status === 'gone' ? `item ${m.item_id} is gone` : m.text}
							</li>
						{/each}
					</ol>
				{/if}
				<div class="flex flex-wrap items-center gap-2">
					<input
						class="border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
						placeholder="pattern-name"
						aria-label={`Pattern name for candidate ${c.id}`}
						bind:value={slug[c.id]}
					/>
					<input
						class="min-w-64 border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
						placeholder="What the pattern is"
						aria-label={`Description for candidate ${c.id}`}
						bind:value={description[c.id]}
					/>
					<button type="button" class="text-micro font-semibold" onclick={() => void name(c.id)}>Name</button>
					<input
						class="border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-2 py-1 text-micro"
						placeholder="Why not"
						aria-label={`Dismissal note for candidate ${c.id}`}
						bind:value={note[c.id]}
					/>
					<button type="button" class="text-micro" onclick={() => void dismiss(c.id)}>Dismiss</button>
				</div>
				{#if rowError[c.id]}
					<p class="text-micro text-[var(--text-3)]" role="alert">{rowError[c.id]}</p>
				{/if}
			</article>
		{/each}
	{/if}
</section>
