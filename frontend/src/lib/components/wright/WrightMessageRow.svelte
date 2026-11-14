<script lang="ts">
	import type { WrightMessage } from '$lib/api/wright';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import CommsAvatar from '$lib/comms/CommsAvatar.svelte';
	import type { UndoStatus } from '$lib/wright/store.svelte';
	import { clockTime, messageLinks } from '$lib/wright/present';
	import { CircleAlert, FileSearch, Wrench } from '@lucide/svelte';
	import type { Component } from 'svelte';
	import WrightChangeCard from './WrightChangeCard.svelte';
	import type { UndoState } from './UndoSlot.svelte';

	interface Props {
		message: WrightMessage;
		undo: Record<string, UndoStatus>;
		/** Why steer Undo is off, such as an unpaired browser. */
		undoDisabledReason?: string | null;
		onUndoSteer: (message: WrightMessage) => void;
	}

	const { message, undo, undoDisabledReason = null, onUndoSteer }: Props = $props();

	const wright = $derived(message.author === 'wright');
	const paragraphs = $derived(message.body.split(/\n{2,}/).filter((p) => p.length > 0));
	const ICON: Record<string, Component> = { problem: CircleAlert, action: Wrench, fix: Wrench };
	const chips = $derived<LinkChip[]>(
		messageLinks(message).map((link) => ({
			icon: ICON[link.kind] ?? FileSearch,
			kind: link.text.split(' ')[0],
			text: link.text.split(' ').slice(1).join(' '),
			href: link.href
		}))
	);
	const steerUndo = $derived<UndoState>(
		message.steer === null
			? 'none'
			: message.steer.undone_at !== null
				? 'undone'
				: (undo[`steer-${message.steer.id}`]?.state ?? (message.steer.can_undo ? 'ready' : 'none'))
	);
</script>

<li class="msg" id="msg-{message.id}" data-from={message.author} data-slot="wright-message-row">
	<div class="msg__gutter">
		{#if wright}
			<CommsAvatar sessionUuid="wright" label="Wright" sender="mike" />
		{:else}
			<CommsAvatar sessionUuid="operator" label="You" sender="operator" />
		{/if}
	</div>
	<div class="msg__col">
		<div class="msg__head">
			<span class="msg__sender">{wright ? 'Wright' : 'You'}</span>
			<span class="msg__time">{clockTime(message.created_at)}</span>
		</div>
		{#each paragraphs as para, i (i)}
			<p class="msg__p">{para}</p>
		{/each}
		{#if message.steer}
			<WrightChangeCard
				change={message.steer.change}
				undo={steerUndo}
				{undoDisabledReason}
				undoReason={undo[`steer-${message.steer.id}`]?.reason ?? null}
				onUndo={() => onUndoSteer(message)}
			/>
		{/if}
		<LinkChipRow items={chips} label="Evidence for this reply" />
	</div>
</li>

<style>
	.msg {
		display: flex;
		gap: var(--space-2);
		padding: var(--space-3) 0;
		border-top: var(--hairline) solid var(--border-subtle);
		scroll-margin-top: var(--space-6);
	}

	.msg:target {
		background: var(--surface-2);
	}

	.msg__gutter {
		flex: none;
		width: var(--control-h);
		padding-top: 2px;
	}

	.msg__col {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.msg__head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-2);
	}

	.msg__sender {
		font-family: var(--font-display);
		font-size: var(--text-md);
		font-weight: 700;
		color: var(--text-strong);
	}

	.msg__time {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.msg__p {
		margin: 0;
		max-width: 92ch;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
</style>
