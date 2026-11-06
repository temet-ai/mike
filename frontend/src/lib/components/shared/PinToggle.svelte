<script lang="ts">
	import { Star } from '@lucide/svelte';
	import Tooltip from './Tooltip.svelte';

	interface Props {
		pinned: boolean;
		onToggle: () => void;
		ariaLabel?: string;
		size?: number;
	}

	let { pinned, onToggle, ariaLabel, size = 12 }: Props = $props();

	const label = $derived(ariaLabel ?? (pinned ? 'Unpin session' : 'Pin session'));

	function handleClick(event: MouseEvent): void {
		event.stopPropagation();
		event.preventDefault();
		onToggle();
	}
</script>

<Tooltip text={label}>
	{#snippet child({ props })}
		<button
			{...props}
			type="button"
			data-motion="hover press"
			class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-3)] hover:bg-[var(--surface-card)] hover:text-[var(--text-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
			class:is-pinned={pinned}
			aria-pressed={pinned}
			aria-label={label}
			onclick={handleClick}
		>
			<Star {size} aria-hidden="true" />
		</button>
	{/snippet}
</Tooltip>

<style>
	button.is-pinned {
		color: var(--interactive-active-fg);
	}
	button.is-pinned:hover {
		color: var(--interactive-active-fg);
		background: var(--surface-card);
	}
	button.is-pinned :global(svg) {
		fill: currentColor;
	}
</style>
