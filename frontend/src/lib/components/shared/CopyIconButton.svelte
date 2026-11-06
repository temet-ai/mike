<script lang="ts">
	import { onMount } from 'svelte';
	import { Check, Copy, Loader } from '@lucide/svelte';
	import Tooltip from './Tooltip.svelte';

	interface Props {
		getText: () => string | null | Promise<string | null>;
		label?: string;
		copiedLabel?: string;
		size?: number;
	}

	let { getText, label = 'Copy', copiedLabel = 'Copied', size = 12 }: Props = $props();

	let clipboardSupported = $state(false);
	let copied = $state(false);
	let loading = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	onMount(() => {
		clipboardSupported = typeof navigator !== 'undefined' && !!navigator.clipboard?.writeText;
		return () => {
			if (copyTimer !== null) clearTimeout(copyTimer);
		};
	});

	async function handleCopy(event: MouseEvent): Promise<void> {
		event.stopPropagation();
		event.preventDefault();
		if (loading) return;
		loading = true;
		try {
			const text = await getText();
			if (text === null || text.length === 0) return;
			await navigator.clipboard.writeText(text);
			copied = true;
			if (copyTimer !== null) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => {
				copied = false;
				copyTimer = null;
			}, 1500);
		} catch {
		} finally {
			loading = false;
		}
	}
</script>

{#if clipboardSupported}
	<Tooltip text={copied ? copiedLabel : label}>
		{#snippet child({ props })}
			<button
				{...props}
				type="button"
				data-motion="hover press"
				class="inline-flex size-6 items-center justify-center rounded-[2px] text-[var(--text-2)] hover:bg-[var(--surface-2)] hover:text-[var(--text-1)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface-bg)]"
				aria-label={copied ? copiedLabel : label}
				aria-busy={loading}
				aria-live="polite"
				onclick={handleCopy}
			>
				{#if loading}
					<Loader {size} data-motion="wait wait-spin" aria-hidden="true" />
				{:else if copied}
					<Check {size} aria-hidden="true" />
				{:else}
					<Copy {size} aria-hidden="true" />
				{/if}
			</button>
		{/snippet}
	</Tooltip>
{/if}

<style>
</style>
