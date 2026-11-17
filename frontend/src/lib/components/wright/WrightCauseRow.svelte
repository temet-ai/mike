<!--
@component
One cause in the Triage queue: the state Wright has reached on it, the check, how many open problems
share it, the resource, how long it has been open and how many times the engineer has looked.
Clicking the row opens the representative problem's detail. A waiting cause (nothing has answered
it for the waited hours) carries the warning border; the border is always reserved, so a state
change repaints in place and the rows below stay where they are.
-->
<script lang="ts">
	import { CircleDot, FileSearch, Link, ListOrdered, TriangleAlert, Wrench } from '@lucide/svelte';
	import type { Component, Snippet } from 'svelte';
	import type { WrightCause } from '$lib/api/wright';
	import Badge from '$lib/components/shared/Badge.svelte';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import {
		attemptsText,
		causeBadge,
		causeLinks,
		causeState,
		causeTooltip,
		openFor,
		problemsText,
		type CauseState
	} from '$lib/wright/causes';
	import { dayAndTime } from '$lib/wright/present';

	interface Props {
		cause: WrightCause;
		/** A cause nothing has answered for this long is "waiting": the one the operator must see. */
		waitedHours: number;
		/** Called the first time the detail opens, so the page can load the representative problem. */
		onOpen?: (problemId: number) => void;
		/** The representative problem's detail. */
		children?: Snippet;
	}

	const { cause, waitedHours, onOpen, children }: Props = $props();

	const GLYPH: Record<CauseState, Component> = {
		found: FileSearch,
		filed: CircleDot,
		known_fix: Wrench,
		waiting: TriangleAlert,
		queued: ListOrdered
	};

	let open = $state(false);
	let opened = false;
	$effect(() => {
		if (open && !opened) {
			opened = true;
			onOpen?.(cause.problem_id);
		}
	});

	const reached = $derived(causeState(cause, waitedHours));
	const badge = $derived(causeBadge(reached));
	const Glyph = $derived(GLYPH[reached]);
	const waiting = $derived(reached === 'waiting');
	/** "16 problems · Open 30h · no attempts": the count is left out for a cause of one problem. */
	const metaLine = $derived(
		[
			...(cause.problems > 1 ? [problemsText(cause.problems)] : []),
			openFor(cause.opened_at),
			attemptsText(cause.attempts)
		].join(' · ')
	);

	const links = $derived<LinkChip[]>(
		causeLinks(cause).map((link) => ({
			icon: link.kind === 'finding' ? FileSearch : link.kind === 'issue' ? CircleDot : Link,
			kind: link.kind === 'finding' ? 'Finding' : link.kind === 'issue' ? 'Issue' : 'Problem',
			text: link.text,
			href: link.href,
			external: link.external
		}))
	);

	const stands = $derived(
		[
			['Opened', cause.opened_at],
			['Last attempt', cause.last_attempt_at],
			['Operator told', cause.notified_at],
			['Wright commented', cause.commented_at]
		].filter((line): line is [string, string] => line[1] !== null)
	);
</script>

<li
	class="cause"
	class:cause--waiting={waiting}
	id="cause-{cause.cause_id}"
	data-slot="wright-cause-row"
	data-state={reached}
>
	<button
		type="button"
		class="cause__toggle"
		aria-expanded={open}
		aria-controls="cause-{cause.cause_id}-detail"
		onclick={() => (open = !open)}
	>
		<span class="cause__head">
			<span class="cause__title">{cause.check_name}</span>
			<span class="cause__badge">
				<Tooltip text={causeTooltip(reached, waitedHours)}>
					{#snippet child({ props })}
						<span {...props}>
							<Badge variant={badge.variant}>
								<span class="cause__state">
									<Glyph size={11} aria-hidden="true" />
									{badge.label}
								</span>
							</Badge>
						</span>
					{/snippet}
				</Tooltip>
			</span>
		</span>
		<span class="cause__resource" title={cause.resource}>{cause.resource}</span>
		<span class="cause__meta" class:cause__meta--waiting={waiting}>
			{metaLine}
		</span>
	</button>
	<div class="cause__links">
		<LinkChipRow items={links} label="Links for cause {cause.cause_id}" />
	</div>
	{#if open}
		<div class="cause__detail" id="cause-{cause.cause_id}-detail">
			<div class="cause__section">
				<h3 class="machine cause__h">Resource</h3>
				<p class="cause__full">{cause.resource}</p>
			</div>
			<div class="cause__section">
				<h3 class="machine cause__h">Where it stands</h3>
				<p class="cause__stand">
					{problemsText(cause.problems)} open · {cause.observations} observations · {attemptsText(
						cause.attempts
					)}
				</p>
				{#each stands as [label, at] (label)}
					<p class="cause__stand">{label} {dayAndTime(at)}</p>
				{/each}
			</div>
			{@render children?.()}
		</div>
	{/if}
</li>

<style>
	.cause {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.cause:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	/* An inset ring takes no space: a row turning waiting repaints without moving its neighbours. */
	.cause--waiting {
		box-shadow: inset 0 0 0 var(--hairline) var(--status-warning-border);
	}

	.cause__toggle {
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

	.cause__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.cause__head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.cause__title {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.cause__badge {
		flex: none;
		order: 9;
		margin-left: auto;
	}

	.cause__state {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}

	/* One line: the whole text is in the title and in the opened detail. */
	.cause__resource {
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.cause__meta {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
	}

	.cause__meta--waiting {
		color: var(--status-warning-fg);
	}

	.cause__links {
		/* Reserved for one row of chips: a state that adds a link does not move the rows below. */
		min-block-size: 1.5rem;
	}

	.cause__detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-2);
	}

	.cause__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.cause__h {
		margin: 0;
		font-size: var(--text-micro);
		color: var(--text-3);
	}

	.cause__full {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.cause__stand {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
</style>
