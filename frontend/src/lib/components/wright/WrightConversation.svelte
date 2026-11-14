<script lang="ts">
	import { MessageSquare, Send } from '@lucide/svelte';
	import { tick, untrack } from 'svelte';
	import type { WrightMessage } from '$lib/api/wright';
	import { Button } from '$lib/components/ui/button';
	import { Field } from '$lib/components/ui/field';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import type { FeedStatus } from '$lib/wright/feed';
	import type { UndoStatus } from '$lib/wright/store.svelte';
	import { PAIR_COMMAND } from '$lib/wright/pairing';
	import { dayLabel } from '$lib/wright/present';
	import WrightMessageRow from './WrightMessageRow.svelte';

	interface Props {
		messages: WrightMessage[];
		status: FeedStatus;
		error: string | null;
		pendingReply: boolean;
		/** True when this browser holds the operator token. Unpaired it can read, not post. */
		paired: boolean;
		sending: boolean;
		sendError: string | null;
		undo: Record<string, UndoStatus>;
		/** Resolves with the stored message's id (or null) and whether it was stored. */
		onSend: (text: string) => Promise<{ ok: boolean; id: number | null }>;
		onUndoSteer: (message: WrightMessage) => void;
		onRetry: () => void;
	}

	const {
		messages,
		status,
		error,
		pendingReply,
		paired,
		sending,
		sendError,
		undo,
		onSend,
		onUndoSteer,
		onRetry
	}: Props = $props();

	let draft = $state('');
	let listEl = $state<HTMLOListElement | null>(null);
	let seenThrough = $state<number | null>(null);

	const lastId = $derived(messages.at(-1)?.id ?? 0);
	const newBelow = $derived.by(() => {
		const seen = seenThrough;
		return seen === null ? 0 : messages.filter((m) => m.author === 'wright' && m.id > seen).length;
	});
	const groups = $derived.by(() => {
		const out: { day: string; rows: WrightMessage[] }[] = [];
		for (const m of messages) {
			const day = dayLabel(m.created_at);
			const last = out.at(-1);
			if (last !== undefined && last.day === day) last.rows.push(m);
			else out.push({ day, rows: [m] });
		}
		return out;
	});

	// The list opens at its newest message once; after that it never scrolls by itself.
	$effect(() => {
		const el = listEl;
		if (el !== null && messages.length > 0 && untrack(() => seenThrough) === null) {
			el.scrollTop = el.scrollHeight;
			seenThrough = untrack(() => lastId);
		}
	});

	function nearBottom(el: HTMLElement): boolean {
		return el.scrollHeight - el.scrollTop - el.clientHeight < 24;
	}

	function onScroll(): void {
		if (listEl !== null && nearBottom(listEl)) seenThrough = lastId;
	}

	function showNew(): void {
		if (listEl === null) return;
		listEl.scrollTop = listEl.scrollHeight;
		seenThrough = lastId;
	}

	async function submit(): Promise<void> {
		if (!paired || draft.trim().length === 0 || sending) return;
		const result = await onSend(draft);
		if (!result.ok) return;
		draft = '';
		await tick();
		const row = result.id === null ? null : listEl?.querySelector<HTMLElement>(`#msg-${result.id}`);
		if (listEl !== null && row) {
			listEl.scrollTop = row.offsetTop - listEl.offsetTop;
			seenThrough = result.id;
		}
	}

	function onKeydown(event: KeyboardEvent): void {
		if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
			event.preventDefault();
			void submit();
		}
	}
</script>

<div class="edge convo" data-slot="wright-conversation">
	<div class="convo__inner">
		{#if status === 'error'}
			<div class="convo__fill">
				<LoadErrorBanner title="Could not load the conversation" detail={error ?? ''} {onRetry} />
			</div>
		{:else if status === 'missing'}
			<div class="convo__fill">
				<FilteredEmptyState
					icon={MessageSquare}
					filtered={false}
					emptyText="The conversation is not running yet"
					emptySubtext="This mike does not serve Wright's conversation yet. It appears here once that part is deployed."
					filteredText=""
				/>
			</div>
		{:else if status === 'ready' && messages.length === 0}
			<div class="convo__fill">
				<FilteredEmptyState
					icon={MessageSquare}
					filtered={false}
					emptyText="No conversation yet"
					emptySubtext="Ask why Wright did something, or tell it what to leave alone."
					filteredText=""
				/>
			</div>
		{:else}
			<ol
				class="convo__list"
				bind:this={listEl}
				onscroll={onScroll}
				aria-label="Conversation with Wright"
			>
				{#each groups as group (group.day)}
					<li class="convo__day" aria-hidden="true">
						<span class="convo__day-label">{group.day}</span>
					</li>
					{#each group.rows as m (m.id)}
						<WrightMessageRow
							message={m}
							{undo}
							undoDisabledReason={paired ? null : 'Pair this browser to undo'}
							{onUndoSteer}
						/>
					{/each}
				{/each}
				{#if pendingReply}
					<li class="convo__pending"><span role="status">Wright is reading the evidence</span></li>
				{/if}
			</ol>
		{/if}
		<div class="convo__notice" aria-live="polite">
			{#if !paired}
				<p class="convo__note">
					This browser is not paired, so you can read here but not post. Run
					<code>{PAIR_COMMAND}</code> in a terminal and use the link it opens.
				</p>
			{:else if sendError}
				<p class="convo__note convo__note--error" role="alert">Not sent: {sendError}</p>
			{:else if newBelow > 0}
				<button type="button" class="convo__new" onclick={showNew}>
					New message from Wright below
				</button>
			{/if}
		</div>
		<form
			class="convo__compose"
			onsubmit={(e) => {
				e.preventDefault();
				void submit();
			}}
		>
			<Field
				multiline
				rows={2}
				bind:value={draft}
				aria-label="Message Wright"
				placeholder="Ask Wright, or tell it what to leave alone"
				disabled={!paired}
				onkeydown={onKeydown}
				class="convo__field"
			/>
			<Button
				type="submit"
				size="icon"
				aria-label="Send"
				disabled={!paired || sending || draft.trim().length === 0}
			>
				<Send aria-hidden="true" />
			</Button>
		</form>
	</div>
</div>

<style>
	.convo {
		--edge: var(--edge-sm);
		--edge-pad: 0px;
		--edge-line: var(--border-subtle);
		--edge-surface: var(--surface-card);
	}

	.convo__inner {
		display: flex;
		flex-direction: column;
		block-size: min(46rem, calc(100svh - 12rem));
		min-block-size: 24rem;
	}

	.convo__list {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		margin: 0;
		padding: var(--space-2) var(--space-4);
		list-style: none;
	}

	.convo__fill {
		flex: 1;
		display: grid;
		place-items: center;
		padding: var(--space-5);
	}

	.convo__day {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin: var(--space-2) 0;
	}

	.convo__day::before,
	.convo__day::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--border-subtle);
	}

	.convo__day-label {
		font-family: var(--font-mono);
		font-size: var(--text-micro);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}

	.convo__day + :global(.msg) {
		border-top: none;
	}

	.convo__pending {
		padding: var(--space-3) 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--text-3);
	}

	.convo__notice {
		flex: none;
		min-block-size: 2.25rem;
		padding: var(--space-2) var(--space-4) 0;
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.convo__note {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-2);
		overflow-wrap: anywhere;
	}

	.convo__note--error {
		color: var(--status-danger-fg);
	}

	.convo__new {
		padding: 0;
		border: 0;
		background: transparent;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--interactive-active-fg);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}

	.convo__new:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.convo__compose {
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-4) var(--space-3);
	}

	.convo__compose :global(.convo__field) {
		flex: 1;
	}

	@container (max-width: 860px) {
		.convo__inner {
			block-size: 70svh;
		}
	}
</style>
