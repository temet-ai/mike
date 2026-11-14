<script lang="ts">
	import type { WrightAction, WrightRemedy } from '$lib/api/wright';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import { dayAndTime, fixVerdict, humanise, problemTitle, remedyTitle } from '$lib/wright/present';

	interface Props {
		remedy: WrightRemedy;
		/** The actions on screen, which the verdict is read from. */
		actions: WrightAction[];
	}

	const { remedy, actions }: Props = $props();

	const TONE: Record<string, BadgeVariant> = {
		active: 'chip-active',
		muted: 'chip-muted',
		warning: 'chip-warning',
		analyzed: 'chip-analyzed'
	};

	const verdict = $derived(fixVerdict(remedy, actions));
</script>

<li class="fix" id="fix-{remedy.name}" data-slot="wright-fix-row">
	<div class="fix__head">
		<p class="fix__title">{remedyTitle(remedy.name)}</p>
		<span class="fix__badge"
			><Badge variant={TONE[verdict.tone] ?? 'chip-muted'}>{verdict.label}</Badge></span
		>
	</div>
	<p class="fix__text">{verdict.line}</p>
	<p class="fix__meta">
		{humanise(remedy.kind)} · answers "{problemTitle(remedy.check_name)}" · {remedy.actions_today} of
		{remedy.daily_limit} today, up to {remedy.max_resources} at a time
		{#if remedy.paused_at}· paused {dayAndTime(remedy.paused_at)}{/if}
	</p>
</li>

<style>
	.fix {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.fix:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.fix:target {
		background: var(--surface-2);
	}

	.fix__head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.fix__title {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.fix__badge {
		flex: none;
		order: 9;
		margin-left: auto;
	}

	.fix__text {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.fix__meta {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}
</style>
