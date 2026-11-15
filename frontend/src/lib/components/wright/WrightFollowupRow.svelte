<!--
@component
One follow-up Wright filed: its state, the issue, the session fixing it and the problems it came
from. A stuck follow-up carries an "Open issue" button in a fixed slot, so a poll that moves a row
into or out of stuck repaints in place and the rows below stay where they are.
Design: artifacts/2026-11-15/wright-followups-ui.md.
-->
<script lang="ts">
	import { CircleDot, ExternalLink, FileSearch, Link, Wrench } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import type { FollowupLimits, WrightFollowup } from '$lib/api/wright';
	import Badge from '$lib/components/shared/Badge.svelte';
	import LinkChipRow, { type LinkChip } from '$lib/components/shared/LinkChipRow.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		dayAndTime,
		followupBadge,
		followupLine,
		followupMeta,
		followupTooltip,
		humanise
	} from '$lib/wright/present';

	interface Props {
		followup: WrightFollowup;
		/** The flow's limits, once loaded: they put "of 3" after the attempt and the daily caps in tooltips. */
		limits?: FollowupLimits | null;
	}

	const { followup, limits = null }: Props = $props();

	let open = $state(false);

	const stuck = $derived(followup.state === 'stuck');
	const badge = $derived(followupBadge(followup.state));
	const issueUrl = $derived(followup.issue_url);

	const links = $derived.by<LinkChip[]>(() => {
		const chips: LinkChip[] = [];
		if (followup.issue_url !== null && followup.issue_number !== null) {
			chips.push({
				icon: CircleDot,
				kind: 'Issue',
				text: `#${followup.issue_number}`,
				href: followup.issue_url,
				external: true
			});
		}
		if (followup.fixer_session !== null) {
			chips.push({
				icon: Wrench,
				kind: 'Fixer',
				text:
					followup.fixer_machine === null
						? followup.fixer_session
						: `${followup.fixer_machine} · ${followup.fixer_session}`,
				href: resolve('/sessions/[sessionId]', { sessionId: followup.fixer_session })
			});
		}
		for (const id of followup.problems) {
			chips.push({ icon: Link, kind: 'Problem', text: `${id}`, href: `#problem-${id}` });
		}
		return chips;
	});

	const findings = $derived<LinkChip[]>(
		followup.findings.map((id) => ({ icon: FileSearch, kind: 'Finding', text: `${id}` }))
	);

	const stands = $derived(
		[
			['Noted', followup.created_at, null],
			['Filed', followup.filed_at, null],
			[`Handed to ${followup.fixer_machine ?? 'a session'}`, followup.handed_off_at, null],
			[
				'Closed',
				followup.closed_at,
				followup.closed_reason ? humanise(followup.closed_reason) : null
			]
		].filter((line): line is [string, string, string | null] => line[1] !== null)
	);
</script>

<li class="followup" id="followup-{followup.id}" data-slot="wright-followup-row">
	<button
		type="button"
		class="followup__toggle"
		aria-expanded={open}
		aria-controls="followup-{followup.id}-detail"
		onclick={() => (open = !open)}
	>
		<span class="followup__head">
			<span class="followup__title">{followup.title}</span>
			<span class="followup__badge">
				<Tooltip text={followupTooltip(followup.state, limits)}>
					{#snippet child({ props })}
						<span {...props}><Badge variant={badge.variant}>{badge.label}</Badge></span>
					{/snippet}
				</Tooltip>
			</span>
		</span>
		<span class="followup__meta">{followupMeta(followup)}</span>
		<span class="followup__line" class:followup__line--stuck={stuck}>
			{followupLine(followup, limits)}
		</span>
	</button>
	<div class="followup__links">
		<div class="followup__chips">
			<LinkChipRow items={links} label="Links for follow-up {followup.id}" />
		</div>
		<span class="followup__action" data-slot="followup-action">
			{#if stuck && issueUrl !== null && followup.issue_number !== null}
				<Button
					variant="outline"
					size="sm"
					href={issueUrl}
					target="_blank"
					rel="noopener noreferrer"
				>
					<ExternalLink aria-hidden="true" />
					Open issue #{followup.issue_number}
				</Button>
			{/if}
		</span>
	</div>
	{#if open}
		<div class="followup__detail" id="followup-{followup.id}-detail">
			<div class="followup__section">
				<h3 class="machine followup__h">What Wright wrote</h3>
				<p class="followup__body">{followup.body}</p>
			</div>
			<div class="followup__section">
				<h3 class="machine followup__h">Where it stands</h3>
				{#each stands as [label, at, reason] (label)}
					<p class="followup__stand">
						{label}
						{dayAndTime(at)}{reason === null ? '' : `: ${reason}`}
					</p>
				{/each}
				{#if followup.attempts > 0}
					<p class="followup__stand">
						Attempt {followup.attempts}{limits === null ? '' : ` of ${limits.max_attempts}`}
					</p>
				{/if}
			</div>
			{#if findings.length > 0}
				<div class="followup__section">
					<h3 class="machine followup__h">Findings</h3>
					<LinkChipRow items={findings} label="Findings for follow-up {followup.id}" />
				</div>
			{/if}
		</div>
	{/if}
</li>

<style>
	.followup {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-3) var(--space-2);
		scroll-margin-top: var(--space-6);
	}

	.followup:not(:first-child) {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.followup:target {
		background: var(--surface-2);
	}

	.followup__toggle {
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

	.followup__toggle:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.followup__head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.followup__title {
		flex: 1;
		min-width: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 600;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}

	.followup__badge {
		flex: none;
		order: 9;
		margin-left: auto;
	}

	.followup__meta {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}

	.followup__line {
		/* Two lines reserved: a state change repaints without moving the rows below. */
		min-block-size: 3em;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.followup__line--stuck {
		color: var(--status-warning-fg);
	}

	.followup__links {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
	}

	.followup__chips {
		flex: 1;
		min-width: 0;
	}

	.followup__action {
		flex: none;
		order: 9;
		margin-left: auto;
		display: inline-flex;
		justify-content: flex-end;
		inline-size: 10rem;
		block-size: 1.75rem;
	}

	.followup__detail {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-2);
	}

	.followup__section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.followup__h {
		margin: 0;
		font-size: var(--text-micro);
		color: var(--text-3);
	}

	.followup__body {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.followup__stand {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-3);
		overflow-wrap: anywhere;
	}
</style>
