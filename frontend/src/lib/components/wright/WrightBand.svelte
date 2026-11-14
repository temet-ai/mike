<script lang="ts">
	import { getContext, type Snippet } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import { createCollapseStore, type CollapseStore } from '$lib/stores/collapseState.svelte';

	interface Props {
		/** Stable key: the open or closed choice is remembered in localStorage under it. */
		id: string;
		label: string;
		meta: string;
		/** Rows that arrived while the page was open and wait for the operator. */
		held?: number;
		onShowHeld?: () => void;
		defaultOpen?: boolean;
		children: Snippet;
	}

	const { id, label, meta, held = 0, onShowHeld, defaultOpen = true, children }: Props = $props();

	const store: CollapseStore =
		getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY) ?? createCollapseStore();
	const key = $derived(`wright:band:${id}`);
	const open = $derived(store.isExpanded(key, defaultOpen));
</script>

<section class="wright-band" data-slot="wright-band" aria-label={label}>
	<div class="band-hd-wrap">
		<button
			type="button"
			class="band-hd"
			aria-expanded={open}
			aria-controls="wright-band-{id}"
			onclick={() => store.toggle(key)}
		>
			<DisclosureMarker {open} />
			<span class="machine wright-band__label">{label}</span>
			<span class="band-hd__meta">{meta}</span>
		</button>
		<span class="wright-band__new">
			{#if held > 0 && onShowHeld}
				<Button variant="ghost" size="sm" onclick={onShowHeld}>{held} new</Button>
			{/if}
		</span>
	</div>
	{#if open}
		<div class="band-body" id="wright-band-{id}">
			{@render children()}
		</div>
	{/if}
</section>

<style>
	.wright-band__label {
		margin: 0;
		font-size: var(--text-xs);
		color: inherit;
	}

	.wright-band__new {
		align-self: center;
		flex: none;
		inline-size: 5rem;
		margin-right: var(--space-3);
		display: inline-flex;
		justify-content: flex-end;
	}
</style>
