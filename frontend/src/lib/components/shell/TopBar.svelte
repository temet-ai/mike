<script lang="ts">
	import { Pause, Play, RefreshCw, HelpCircle, Palette, Search, ArrowRight } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import type { StatsResponse } from '$lib/types';
	import { shellState } from '$lib/stores/shellState.svelte';
	import { themeStore } from '$lib/stores/theme.svelte';
	import { THEMES, type ThemeId } from '$lib/themes';
	import * as Popover from '$lib/components/ui/popover';
	import StatusIndicators from './StatusIndicators.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import MobileNavigation from './MobileNavigation.svelte';

	interface Props {
		stats: StatsResponse | null;
		onRefresh: () => void;
	}

	let { stats, onRefresh }: Props = $props();

	function togglePause(): void {
		shellState.paused = !shellState.paused;
	}

	function toggleHelpDrawer(): void {
		shellState.helpDrawerOpen = !shellState.helpDrawerOpen;
	}

	let themePickerOpen = $state(false);
	let shortcutLabel = $state('Ctrl K');

	onMount(() => {
		if (/Mac|iPhone|iPad|iPod/.test(navigator.platform)) shortcutLabel = '⌘ K';
	});

	function selectTheme(id: ThemeId): void {
		themeStore.set(id);
	}
</script>

<div class="topbar">
	<div class="left">
		<div class="mobile-nav-trigger"><MobileNavigation /></div>
		<button
			type="button"
			class="mini-btn command-trigger"
			aria-label="Search pages and commands"
			aria-haspopup="dialog"
			onclick={() => (shellState.commandPaletteOpen = true)}
		>
			<Search class="size-3.5" aria-hidden="true" />
			<span>Search</span>
			<kbd aria-hidden="true">{shortcutLabel}</kbd>
		</button>
		<div class="connection-addresses">
			<Tooltip
				text="Address Claude Code points ANTHROPIC_BASE_URL at — the proxy listens here."
				slug="anthropic-base-url"
			>
				<span class="endpoint">
					proxy <ArrowRight class="size-3" aria-hidden="true" />
					<code>http://localhost:9988</code>
				</span>
			</Tooltip>
			<span class="text-[var(--text-faint)]" aria-hidden="true">·</span>
			<Tooltip
				text="Configured upstream service receiving forwarded requests over HTTPS."
				slug="byte-identity-forwarding"
			>
				<span class="endpoint">
					fwd <ArrowRight class="size-3" aria-hidden="true" />
					<code>api.anthropic.com</code>
				</span>
			</Tooltip>
		</div>
	</div>

	<div class="right">
		<StatusIndicators {stats} showCommandShortcut={false} />

		<span class="text-[var(--text-faint)]" aria-hidden="true">·</span>

		<Tooltip text={shellState.paused ? 'Resume live updates' : 'Pause live updates'}>
			{#snippet child({ props })}
				<button
					{...props}
					type="button"
					class="mini-btn"
					class:filter-active={shellState.paused}
					onclick={togglePause}
					aria-pressed={shellState.paused}
				>
					{#if shellState.paused}
						<Play class="size-3" aria-hidden="true" />
						<span>resume</span>
					{:else}
						<Pause class="size-3" aria-hidden="true" />
						<span>pause</span>
					{/if}
				</button>
			{/snippet}
		</Tooltip>

		<Tooltip text="Refresh stats">
			{#snippet child({ props })}
				<button {...props} type="button" class="mini-btn" onclick={onRefresh}>
					<RefreshCw class="size-3" aria-hidden="true" />
					<span>refresh</span>
				</button>
			{/snippet}
		</Tooltip>

		<Popover.Root bind:open={themePickerOpen}>
			<Tooltip text="Choose a colour theme — applies instantly and persists across reloads">
				{#snippet child({ props: tooltipProps })}
					<Popover.Trigger>
						{#snippet child({ props: triggerProps })}
							<button
								{...tooltipProps}
								{...triggerProps}
								type="button"
								class="mini-btn"
								class:filter-active={themePickerOpen}
								aria-label="Choose colour theme"
							>
								<Palette class="size-3" aria-hidden="true" />
								<span>theme</span>
							</button>
						{/snippet}
					</Popover.Trigger>
				{/snippet}
			</Tooltip>
			<Popover.Content align="end" class="w-52 p-2">
				<fieldset class="theme-options">
					<legend>Colour theme</legend>
					{#each THEMES as theme (theme.id)}
						{@const selected = themeStore.current === theme.id}
						<label class="theme-item" class:theme-item-selected={selected}>
							<input
								type="radio"
								name="colour-theme"
								value={theme.id}
								checked={selected}
								onchange={() => selectTheme(theme.id)}
							/>
							<span data-theme={theme.id} class="theme-swatch" aria-hidden="true"></span>
							<span class="theme-label">{theme.label}</span>
						</label>
					{/each}
				</fieldset>
			</Popover.Content>
		</Popover.Root>

		<Tooltip text="Documentation, links, and capture diagnostics">
			{#snippet child({ props })}
				<button
					{...props}
					type="button"
					class="mini-btn"
					class:filter-active={shellState.helpDrawerOpen}
					onclick={toggleHelpDrawer}
					aria-label="Open help drawer"
					aria-pressed={shellState.helpDrawerOpen}
				>
					<HelpCircle class="size-3" aria-hidden="true" />
					<span>help</span>
				</button>
			{/snippet}
		</Tooltip>
	</div>
</div>

<style>
	.topbar {
		gap: var(--space-3);
	}
	.topbar .left,
	.topbar .right {
		min-width: 0;
		gap: var(--space-2);
	}
	.mobile-nav-trigger {
		display: none;
	}
	.connection-addresses,
	.endpoint {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.command-trigger {
		min-height: var(--control-h);
		flex-shrink: 0;
	}
	.command-trigger kbd {
		padding: 0 var(--space-1);
		border: var(--hairline) solid var(--border-default);
		border-radius: var(--r-tol);
		color: var(--text-3);
		font-size: var(--text-xs);
	}
	.theme-options {
		border: 0;
		margin: 0;
		padding: 0;
	}
	.theme-options legend {
		padding: var(--space-2);
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		letter-spacing: var(--track-machine);
		text-transform: uppercase;
		color: var(--text-2);
	}
	.theme-item input {
		accent-color: var(--funk-cyan);
		flex-shrink: 0;
	}
	.theme-item:has(input:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	@media (max-width: 79.999rem) {
		.connection-addresses {
			display: none;
		}
	}
	@media (max-width: 47.999rem) {
		.mobile-nav-trigger {
			display: flex;
		}
		.topbar {
			height: auto;
			min-height: calc(var(--control-h-lg) + var(--space-2));
			padding: var(--space-2);
			flex-wrap: wrap;
		}
		.topbar .right {
			flex-wrap: wrap;
			margin-left: auto;
		}
		.command-trigger kbd {
			display: none;
		}
	}
	@media (pointer: coarse) {
		.mini-btn,
		.theme-options .theme-item {
			min-height: calc(var(--control-h-lg) + var(--space-1));
		}
		.mobile-nav-trigger :global([data-button-root]) {
			min-height: calc(var(--control-h-lg) + var(--space-1));
			min-width: calc(var(--control-h-lg) + var(--space-1));
		}
	}
	.mini-btn.filter-active {
		color: var(--interactive-active-fg);
		border-color: var(--interactive-active-border);
	}

	.theme-item {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		padding: var(--space-2);
		min-height: var(--control-h-lg);
		border-radius: var(--r-tol);
		font: var(--text-sm) var(--font-mono);
		color: var(--text-2);
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
	}

	.theme-item:hover {
		background: var(--surface-2);
		color: var(--text-1);
	}

	.theme-item-selected {
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg);
	}

	.theme-label {
		flex: 1;
	}

	.theme-swatch {
		width: var(--space-3);
		height: var(--space-3);
		flex-shrink: 0;
		border-radius: var(--r-tol);
		background: var(--interactive-active-fg);
		box-shadow: 0 0 6px var(--interactive-active-fg);
		border: 1px solid var(--border-subtle);
	}
</style>
