<script lang="ts" module>
	/** `none`: nothing to undo, but the slot keeps its width so rows line up. */
	export type UndoState = 'ready' | 'undoing' | 'undone' | 'failed' | 'none';
</script>

<script lang="ts">
	import { Undo2 } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		label?: 'Undo' | 'Revert';
		/** Names what is undone, for the button's accessible name. */
		subject: string;
		state: UndoState;
		onUndo: () => void;
		/** Why the button is off, for example that this browser is not paired. */
		disabledReason?: string | null;
		/** Why the undo failed, shown as the tooltip of "Couldn't undo". */
		reason?: string | null;
	}

	const {
		label = 'Undo',
		subject,
		state,
		onUndo,
		disabledReason = null,
		reason = null
	}: Props = $props();
</script>

<span class="undo-slot" data-slot="undo-slot" data-state={state}>
	{#if state === 'none'}
		<!-- empty on purpose -->
	{:else if state === 'undone'}
		<span class="undo-slot__text" role="status">Undone</span>
	{:else if state === 'failed'}
		<span class="undo-slot__text undo-slot__text--failed" role="status" title={reason ?? undefined}>
			Couldn't undo
		</span>
	{:else if state === 'undoing'}
		<Button variant="outline" size="sm" disabled aria-label="{label}: {subject}">
			<Undo2 aria-hidden="true" />
			Undoing
		</Button>
	{:else}
		<Button
			variant="outline"
			size="sm"
			aria-label="{label}: {subject}"
			disabled={disabledReason !== null}
			title={disabledReason ?? undefined}
			onclick={onUndo}
		>
			<Undo2 aria-hidden="true" />
			{label}
		</Button>
	{/if}
</span>

<style>
	.undo-slot {
		flex: none;
		order: 9;
		margin-left: auto;
		display: inline-flex;
		justify-content: flex-end;
		inline-size: 6.5rem;
	}

	.undo-slot__text {
		display: inline-flex;
		align-items: center;
		block-size: 1.75rem;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.undo-slot__text--failed {
		color: var(--status-warning-fg);
	}
</style>
