<script lang="ts">
	import type { Snippet } from 'svelte';
	import { revealSections } from '$lib/actions/reveal';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import SettingsNavigation from './SettingsNavigation.svelte';

	interface Props {
		title: string;
		current: string;
		lede?: Snippet;
		loadErrors?: (string | null)[];
		onRetry?: () => void;
		wide?: boolean;
		children: Snippet;
	}

	const {
		title,
		current,
		lede,
		loadErrors = [],
		onRetry,
		wide = false,
		children
	}: Props = $props();

	const failures = $derived(
		loadErrors.filter((failure): failure is string => failure !== null && failure !== '')
	);
</script>

<div class="settings-page-scroll" data-slot="settings-page">
	<div class="settings-page" data-wide={wide ? '' : undefined} use:revealSections>
		{#if lede}
			<PageIntro {title} level="h1">{@render lede()}</PageIntro>
		{:else}
			<PageIntro {title} level="h1" />
		{/if}
		<SettingsNavigation {current} />
		{#if failures.length > 0}
			<LoadErrorBanner title="Could not load this page." detail={failures.join(' · ')} {onRetry} />
		{/if}
		{@render children()}
	</div>
</div>

<style>
	.settings-page-scroll {
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		background: var(--surface-bg);
	}
	.settings-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-width: 920px;
		margin: 0 auto;
		padding: var(--space-5) var(--space-5) var(--space-8);
	}
	.settings-page[data-wide] {
		max-width: 1100px;
	}
</style>
