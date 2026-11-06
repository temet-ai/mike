<script lang="ts">
	import { Bot, Inbox, Send } from '@lucide/svelte';
	import { formatRelative } from '$lib/time';
	import { roleTokens } from '$lib/fleet/roleTokens';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import type { TeamCommsResponse } from './teamComms';

	interface Props {
		now: number;
		data: TeamCommsResponse | null;
		loading: boolean;
		error: string | null;
		unavailable: boolean;
		onRetry: () => void;
	}

	const { now, data, loading, error, unavailable, onRetry }: Props = $props();
</script>

{#if !unavailable}
	<section class="team-feed" aria-label="Team activity" data-augmented-ui="tl-clip br-clip border">
		<h3
			class="team-feed__title"
			title="Agents this session dispatched via the Agent or Workflow tool, and the mailbox messages exchanged with them"
		>
			Team activity
		</h3>

		{#if loading}
			<p class="team-feed__status">Loading team activity…</p>
		{:else if error}
			<LoadErrorBanner title="Couldn't load team activity" detail={error} {onRetry} />
		{:else if data}
			{#if data.dispatches.length === 0 && data.messages.length === 0}
				<p class="team-feed__status">No dispatched agents or team messages yet.</p>
			{:else}
				{#if data.dispatches.length > 0}
					<ul class="team-roster" role="list">
						{#each data.dispatches as d (d.agent_label + d.first_seen)}
							{@const tokens = roleTokens(d.agent_label)}
							{@const Icon = tokens.icon}
							<li class="team-roster__row">
								<span
									class="team-roster__icon"
									style:--role-fg={tokens.fg}
									style:--role-bg={tokens.bg}
									style:--role-border={tokens.border}
									aria-hidden="true"
								>
									<Icon size={12} />
								</span>
								<span class="team-roster__label">{d.agent_label}</span>
								<span class="team-roster__desc">{d.description}</span>
								<span class="team-roster__count">{d.request_count} req</span>
								<span class="team-roster__time" title="{d.first_seen} – {d.last_seen}">
									{formatRelative(d.last_seen, now)}
								</span>
							</li>
						{/each}
					</ul>
				{/if}

				{#if data.messages.length > 0}
					<ul
						class="team-feed__list"
						role="list"
						class:team-feed__list--divided={data.dispatches.length > 0}
					>
						{#each data.messages as m, i (m.ts + m.direction + m.counterpart + i)}
							<li class="team-msg" data-augmented-ui="tl-clip br-clip border">
								<div class="team-msg__row">
									<span class="team-msg__icon" aria-hidden="true">
										{#if m.direction === 'sent'}
											<Send size={12} />
										{:else}
											<Inbox size={12} />
										{/if}
									</span>
									<div class="team-msg__body-col">
										<div class="team-msg__head">
											<span class="team-msg__dir">{m.direction === 'sent' ? 'To' : 'From'}</span>
											<span class="team-msg__counterpart">{m.counterpart}</span>
											<span class="team-msg__time" title={m.ts}>{formatRelative(m.ts, now)}</span>
										</div>
										{#if m.summary}
											<p class="team-msg__summary">{m.summary}</p>
										{/if}
										<pre class="team-msg__body">{m.body}</pre>
									</div>
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{/if}
		{/if}
	</section>
{/if}

<style>
	.team-feed {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		background: var(--surface-card);
	}

	.team-feed__title {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		cursor: default;
	}

	.team-feed__status {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
	}

	.team-roster {
		display: flex;
		flex-direction: column;
		gap: 4px;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.team-roster__row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 3px 0;
	}

	.team-roster__icon {
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border-radius: 4px;
		background: var(--role-bg);
		color: var(--role-fg);
		border: 1px solid var(--role-border);
	}

	.team-roster__label {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-1);
	}

	.team-roster__desc {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.team-roster__count {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}

	.team-roster__time {
		flex: none;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.team-feed__list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.team-feed__list--divided {
		padding-top: 8px;
		border-top: 1px solid var(--border-subtle);
	}

	.team-msg {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: 10px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-subtle);
		padding: 8px 10px;
		background: var(--surface-2);
	}

	.team-msg__row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
	}

	.team-msg__icon {
		flex: none;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		margin-top: 1px;
		border-radius: 4px;
		background: var(--surface-card);
		color: var(--text-2);
		border: 1px solid var(--border-subtle);
	}

	.team-msg__body-col {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.team-msg__head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
	}

	.team-msg__dir {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		color: var(--text-3);
	}

	.team-msg__counterpart {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-1);
		overflow-wrap: anywhere;
	}

	.team-msg__time {
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		color: var(--text-3);
	}

	.team-msg__summary {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
	}

	.team-msg__body {
		margin: 0;
		max-height: 320px;
		overflow: auto;
		padding: 8px 10px;
		border-radius: 2px;
		background: var(--surface-card);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-1);
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
