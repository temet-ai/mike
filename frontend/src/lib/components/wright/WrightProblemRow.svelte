<script lang="ts">
	import { FileSearch, Link } from '@lucide/svelte';
	import type { Snippet } from 'svelte';
	import type { WrightProblem } from '$lib/api/wright';
	import Badge from '$lib/components/shared/Badge.svelte';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import { dayAndTime, evidenceFigures, problemLinks, problemTitle } from '$lib/wright/present';

	interface Props {
		problem: WrightProblem;
		/** A link to this row (`#problem-id`) asks for its detail to be open. */
		requestOpen?: boolean;
		/** Called the first time the detail opens, so the page can load it. */
		onOpen?: (id: number) => void;
		/** The detail: findings, repairs, actions, evidence and episodes. */
		children?: Snippet;
	}

	const { problem, requestOpen = false, onOpen, children }: Props = $props();

	let open = $state(false);

	$effect(() => {
		if (requestOpen) open = true;
	});

	let opened = false;
	$effect(() => {
		if (open && !opened) {
			opened = true;
			onOpen?.(problem.id);
		}
	});

	const healthy = $derived(problem.state === 'healthy');
	const figures = $derived(evidenceFigures(problem.latest_evidence));
	const links = $derived<LinkChip[]>(
		problemLinks(problem).map((link) => ({
			icon: link.kind === 'problem' ? Link : FileSearch,
			kind: link.kind === 'problem' ? 'Problem' : undefined,
			text: link.kind === 'problem' ? link.text.replace(/^Problem /, '') : link.text,
			href: link.href,
			external: link.external
		}))
	);
</script>

<li class="problem" id="problem-{problem.id}" data-slot="wright-problem-row">
	<button
		type="button"
		class="problem__toggle"
		aria-expanded={open}
		aria-controls="problem-{problem.id}-detail"
		onclick={() => (open = !open)}
	>
		<span class="problem__head">
			<span class="problem__title">{problemTitle(problem.check_name)}</span>
			<span class="problem__badge">
				<Badge variant={healthy ? 'chip-active' : 'chip-warning'}
					>{healthy ? 'Healthy' : 'Open'}</Badge
				>
			</span>
		</span>
		<span class="problem__resource">{problem.resource}</span>
		<span class="problem__meta">
			Opened {dayAndTime(problem.opened_at)}
			{#if healthy && problem.healthy_at}· healthy {dayAndTime(problem.healthy_at)}
			{:else}· last seen {dayAndTime(problem.last_seen_at)}{/if}
			· {problem.episodes}
			{problem.episodes === 1 ? 'episode' : 'episodes'}
		</span>
		{#if figures.length > 0}<span class="problem__meta">{figures.join(' · ')}</span>{/if}
	</button>
	<div class="problem__links">
		<LinkChipRow items={links} label="Evidence for problem {problem.id}" />
	</div>
	{#if open}
		<div class="problem__detail" id="problem-{problem.id}-detail">
			{@render children?.()}
		</div>
	{/if}
</li>

<style>
	.problem {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.problem:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.problem:target {
		background: var(--surface-2);
	}

	.problem__toggle {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: 0;
		border: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.problem__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.problem__head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.problem__title {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.problem__badge {
		flex: none;
		order: 9;
		margin-left: auto;
	}

	.problem__resource {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.problem__meta {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.problem__detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-2);
	}
</style>
