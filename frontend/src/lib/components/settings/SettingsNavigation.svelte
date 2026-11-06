<script lang="ts">
	import { resolve } from '$app/paths';
	import { SETTINGS_DESTINATIONS as destinations } from '$lib/settings/destinations';

	let { current }: { current: string } = $props();
</script>

<nav class="settings-navigation" aria-label="Settings sections" data-reveal>
	{#each destinations as destination (destination.href)}
		<a
			href={resolve(destination.href)}
			aria-current={current === destination.href ? 'page' : undefined}
		>
			<destination.icon size={14} aria-hidden="true" />
			<span>{destination.label}</span>
		</a>
	{/each}
</nav>

<style>
	.settings-navigation {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(6.5rem, 1fr));
		gap: var(--hairline);
		margin-bottom: var(--space-5);
		padding: var(--hairline);
	}
	a {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--control-h-lg);
		padding: var(--space-2) var(--space-3);
		background: var(--surface-card);
		box-shadow: 0 0 0 var(--hairline) var(--border-default);
		color: var(--text-2);
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		text-decoration: none;
		transition:
			background var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap);
	}
	a:hover {
		background: var(--surface-2);
		color: var(--text-strong);
	}
	a[aria-current='page'] {
		color: var(--interactive-active-fg);
		background: var(--interactive-active-bg);
		box-shadow:
			0 0 0 var(--hairline) var(--border-default),
			inset 0 calc(-1 * var(--space-1)) 0 var(--interactive-active-border);
	}
	a:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
</style>
