<script lang="ts">
	import { AlertTriangle } from '@lucide/svelte';
	import { resolve } from '$app/paths';
	import { formatRelative } from '$lib/time';
	import { describeThreadDelivery, type DeliveryPresentation } from './deliveryPresentation';
	import { buildThreadIdentityIndex, resolveParticipantLabel } from './participantIdentity';
	import Badge from '$lib/components/shared/Badge.svelte';
	import CommsAvatar from './CommsAvatar.svelte';
	import { describeMessageCount, splitFleetSubject } from './groupThreads';
	import type { ParticipantIdentityDto, ThreadSummaryDto } from './types';

	interface Props {
		threads: ThreadSummaryDto[];
		selectedThreadUuid: string | null;
		now: number;
		id?: string;
		onSelect: (uuid: string) => void;
	}

	const { threads, selectedThreadUuid, now, id, onSelect }: Props = $props();

	function isFindable(presentation: DeliveryPresentation): boolean {
		return presentation.tone === 'warn' || presentation.tone === 'alert';
	}

	function threadAriaLabel(t: ThreadSummaryDto, presentation: DeliveryPresentation): string {
		const bits = [t.subject];
		if (isFindable(presentation)) bits.push(presentation.label);
		if (t.tags.length > 0) bits.push(`tags ${t.tags.join(', ')}`);
		if (t.fleet) bits.push(`fleet ${t.fleet.slug}`);
		bits.push(describeMessageCount(t.message_count));
		bits.push(`updated ${formatRelative(t.last_message_at, now)}`);
		if (t.unread_count > 0) bits.push(`${t.unread_count} unread`);
		if (t.resolved_at !== null) bits.push('resolved');
		return bits.join(', ');
	}

	function identityFor(
		uuid: string,
		index: Map<string, ParticipantIdentityDto>
	): ParticipantIdentityDto | undefined {
		return index.get(uuid);
	}
</script>

<ul class="thread-list__rows" role="list" {id}>
	{#each threads as t (t.thread_uuid)}
		{@const identities = buildThreadIdentityIndex(t)}
		{@const presentation = describeThreadDelivery(t)}
		{@const findable = isFindable(presentation)}
		{@const split = splitFleetSubject(t)}
		<li>
			<div
				class="thread-row-wrap"
				class:thread-row-wrap--selected={selectedThreadUuid === t.thread_uuid}
				class:thread-row-wrap--warn={findable}
				class:thread-row-wrap--resolved={t.resolved_at !== null}
				data-augmented-ui={selectedThreadUuid === t.thread_uuid ? 'br-clip border' : undefined}
			>
				<div class="thread-row__line1">
					<button
						type="button"
						class="thread-row"
						onclick={() => onSelect(t.thread_uuid)}
						aria-current={selectedThreadUuid === t.thread_uuid ? 'true' : undefined}
						aria-label={threadAriaLabel(t, presentation)}
					>
						<span
							class="thread-row__subject"
							class:thread-row__subject--unread={t.unread_count > 0}
							aria-hidden="true"
							>{#if split}<Badge variant="tag-blue">{split.lead}</Badge><span class="sr-only"
									>:
								</span>
								{split.rest}{:else}{t.subject}{/if}</span
						>
					</button>
					<span class="thread-row__time" aria-hidden="true"
						>{formatRelative(t.last_message_at, now)}</span
					>
					<span class="thread-row__unread-slot" aria-hidden="true">
						<span
							class="thread-row__unread-count"
							style:visibility={t.unread_count > 0 ? 'visible' : 'hidden'}>{t.unread_count}</span
						>
					</span>
				</div>

				<div class="thread-row__line2">
					<span class="thread-row__avatars" role="group" aria-label="Participants in this thread">
						{#each t.participants as p (p)}
							{@const identity = identityFor(p, identities)}
							{@const label = resolveParticipantLabel(p, identities)}
							<a
								class="thread-row__avatar-link"
								href={resolve(
									`sessions/${encodeURIComponent(p)}/comms` as `sessions/${string}/comms`
								)}
								aria-label={`Open comms for ${label}`}
								title={label}
							>
								<CommsAvatar
									sessionUuid={p}
									label={identity?.label ?? null}
									role={identity?.role ?? null}
									agentType={identity?.agent_type ?? null}
									size="compact"
								/>
								<span class="sr-only">{label}</span>
							</a>
						{/each}
					</span>
					{#if findable}
						<span class="thread-row__delivery" title={presentation.detail} aria-hidden="true">
							<AlertTriangle size={11} aria-hidden="true" />
							{presentation.label}
						</span>
					{/if}
					{#if t.fleet && !split}
						<span aria-hidden="true"><Badge variant="tag-blue">fleet {t.fleet.slug}</Badge></span>
					{/if}
					<span class="thread-row__count" aria-hidden="true"
						>{describeMessageCount(t.message_count)}</span
					>
					{#if t.tags.length > 0}
						<span class="thread-row__tags-text" aria-hidden="true">{t.tags.join(' · ')}</span>
					{/if}
					{#if t.resolved_at !== null}
						<span class="thread-row__resolved-text" aria-hidden="true">resolved</span>
					{/if}
				</div>
			</div>
		</li>
	{/each}
</ul>

<style>
	.thread-list__rows {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.thread-list__rows > li + li {
		border-top: var(--hairline) solid var(--border-subtle);
	}

	.thread-row-wrap {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		width: 100%;
		padding: var(--space-2) var(--space-3);
	}

	.thread-row-wrap:hover {
		background: var(--surface-2);
	}

	.thread-row-wrap--selected,
	.thread-row-wrap--selected:hover {
		background:
			linear-gradient(0deg, var(--funk-cyan-dim), var(--funk-cyan-dim)), var(--surface-card);
		--aug-br: var(--edge-sm);
		--aug-border-all: 1px;
		--aug-border-left: 2px;
		--aug-border-bg: var(--funk-cyan-border);
	}

	.thread-row-wrap--selected .thread-row__subject {
		color: var(--funk-cyan);
		text-shadow: var(--glow-cyan);
	}

	.thread-row-wrap--resolved {
		opacity: 0.7;
	}

	.thread-row__line1 {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		min-width: 0;
	}

	.thread-row {
		all: unset;
		flex: 1;
		min-width: 0;
		cursor: pointer;
	}

	.thread-row::after {
		content: '';
		position: absolute;
		inset: 0;
	}

	.thread-row:focus-visible::after {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}

	.thread-row__subject {
		overflow-wrap: anywhere;
		font-family: var(--font-display);
		font-weight: var(--weight-display);
		font-size: var(--text-sm);
		line-height: var(--leading-tight);
		color: var(--text-2);
	}

	.thread-row__subject--unread {
		color: var(--text-strong);
	}

	.thread-row__time {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--text-faint);
		font-variant-numeric: tabular-nums;
	}

	.thread-row__unread-slot {
		flex: none;
		display: inline-flex;
		justify-content: flex-end;
		min-width: var(--space-5);
	}

	.thread-row__unread-count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 15px;
		height: 15px;
		padding: 0 var(--space-1);
		border-radius: 100px;
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		border: var(--hairline) solid var(--status-info-border);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		font-variant-numeric: tabular-nums;
	}

	.thread-row__line2 {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
		font-family: var(--font-mono, monospace);
		font-size: var(--text-caption);
		color: var(--text-faint);
	}

	.thread-row__avatars {
		position: relative;
		z-index: 1;
		display: inline-flex;
		flex-wrap: wrap;
		gap: 2px;
	}

	.thread-row__avatar-link {
		display: inline-flex;
		text-decoration: none;
		border-radius: 3px;
	}

	.thread-row__avatar-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 1px;
	}

	.thread-row__delivery {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		color: var(--status-warning-fg);
	}

	.thread-row__tags-text {
		overflow-wrap: anywhere;
	}

	.thread-row__resolved-text {
		color: var(--status-success-fg);
	}
</style>
