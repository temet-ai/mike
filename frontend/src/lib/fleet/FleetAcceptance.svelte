<script lang="ts">
	import { resolve } from '$app/paths';
	import { RefreshCw } from '@lucide/svelte';
	import Badge from '$lib/components/shared/Badge.svelte';
	import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import { Button } from '$lib/components/ui/button';
	import { formatClockAbsolute } from '$lib/time';
	import type { FleetAcceptanceStatus, FleetAcceptanceObligation } from './types';

	interface Props {
		status: FleetAcceptanceStatus | null;
		loading: boolean;
		error: string | null;
		onRefresh: () => void;
	}

	const { status, loading, error, onRefresh }: Props = $props();
	const obligations = $derived(status?.obligations ?? []);
	const requiredOpen = $derived(
		obligations.filter((obligation) => obligation.required && !isAccepted(obligation)).length
	);
	const readyCount = $derived(status?.tasks.filter((task) => task.ready === true).length ?? 0);
	const reportsAcceptance = $derived(
		typeof status?.accepted === 'boolean' && Array.isArray(status?.obligations)
	);

	function isAccepted(obligation: FleetAcceptanceObligation): boolean {
		return obligation.outcome === 'accepted' && !obligation.issue;
	}

	function outcomeVariant(obligation: FleetAcceptanceObligation): BadgeVariant {
		if (obligation.issue || obligation.outcome === 'failed') return 'chip-error';
		if (isAccepted(obligation)) return 'chip-active';
		return obligation.required ? 'chip-warning' : 'chip-muted';
	}

	function field(data: unknown, keys: string[], fallback = 'Not recorded'): string {
		if (data === null || typeof data !== 'object' || Array.isArray(data)) return fallback;
		const fields = data as Record<string, unknown>;
		for (const key of keys) {
			const value = fields[key];
			if (value === undefined || value === null || value === '') continue;
			return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
		}
		return fallback;
	}
</script>

<section
	class="flex min-w-0 flex-col gap-3"
	data-slot="fleet-acceptance"
	aria-label="Fleet acceptance"
>
	<div class="flex items-center justify-between gap-2">
		<h4 class="machine text-label text-[var(--text-3)]">Fleet acceptance</h4>
		<Button
			variant="ghost"
			size="sm"
			onclick={onRefresh}
			disabled={loading}
			aria-label="Refresh fleet acceptance"
		>
			<RefreshCw class="size-3" aria-hidden="true" />
			Refresh
		</Button>
	</div>
	{#if error}
		<LoadErrorBanner title="Could not load fleet acceptance" detail={error} onRetry={onRefresh} />
	{:else if loading && status === null}
		<p class="text-caption text-[var(--text-3)]" role="status">Loading fleet acceptance</p>
	{:else if status === null}
		<p class="text-caption text-[var(--text-3)]">This session has no registered fleet.</p>
	{:else}
		<dl class="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-3 gap-y-2 text-caption">
			<dt class="text-[var(--text-3)]">Tasks ready for dependents</dt>
			<dd data-slot="fleet-ready-count" class="text-right">
				<Badge variant="chip-analyzed"
					>{reportsAcceptance ? readyCount : '?'} / {status.tasks.length}</Badge
				>
			</dd>
			<dt class="text-[var(--text-3)]">Acceptance</dt>
			<dd data-slot="fleet-acceptance-state" class="text-right">
				<Badge variant={reportsAcceptance && status.accepted ? 'chip-active' : 'chip-warning'}>
					{!reportsAcceptance ? 'Unavailable' : status.accepted ? 'Accepted' : 'Outstanding'}
				</Badge>
			</dd>
			<dt class="text-[var(--text-3)]">Required observations outstanding</dt>
			<dd class="text-right text-[var(--text-1)]">
				{reportsAcceptance ? requiredOpen : 'Unknown'}
			</dd>
			<dt class="text-[var(--text-3)]">Fleet activity</dt>
			<dd class="text-right text-[var(--text-2)]">{status.state}</dd>
		</dl>
		<p class="text-caption text-[var(--text-3)]">
			Readiness releases dependent work. Acceptance records whether the agreed outcome is
			established.
		</p>
		<p class="text-caption text-[var(--text-3)]">
			Snapshot {formatClockAbsolute(status.now)}. Refresh to load newer observations.
		</p>
		<details class="min-w-0" data-slot="fleet-agreement-details">
			<summary
				class="cursor-pointer text-caption text-[var(--text-3)] focus-visible:outline-2 focus-visible:outline-[var(--interactive-focus-ring)]"
			>
				Fleet agreement
			</summary>
			<div class="mt-2 flex min-w-0 flex-col gap-2">
				<p class="text-sm break-words text-[var(--text-1)]">{status.slug}</p>
				<p class="text-caption whitespace-pre-wrap break-words text-[var(--text-2)]">
					{status.goal}
				</p>
			</div>
		</details>
		{#if !reportsAcceptance}
			<p class="text-caption text-[var(--status-warning-fg)]">
				This running service does not report independent readiness and acceptance obligations yet.
			</p>
		{/if}
		{#if status.blocked.length > 0 || status.needs_operator.length > 0}
			<ul
				class="flex list-none flex-col gap-2 text-caption text-[var(--status-warning-fg)]"
				aria-label="Fleet blockers"
			>
				{#each status.blocked as blocker, index (index)}
					<li class="whitespace-pre-wrap break-words">{blocker.seat}: {blocker.title}</li>
				{/each}
				{#each status.needs_operator as request, index (index)}
					<li class="whitespace-pre-wrap break-words">
						Operator needed · {request.seat}: {request.title}
					</li>
				{/each}
			</ul>
		{/if}
		<ul
			class="list-none divide-y divide-[var(--border-subtle)]"
			aria-label="Task readiness and acceptance"
		>
			{#each status.tasks as task (task.id)}
				<li class="flex flex-col gap-2 py-3" data-slot="fleet-task-status" data-task-id={task.id}>
					<p class="text-caption whitespace-pre-wrap break-words text-[var(--text-1)]">
						{task.id} · {task.name}
					</p>
					<div class="flex flex-wrap items-center gap-2 text-caption text-[var(--text-3)]">
						<Badge variant="chip-analyzed"
							>{task.ready === undefined
								? 'Readiness unknown'
								: task.ready
									? 'Ready for dependents'
									: 'Not ready for dependents'}</Badge
						>
						<Badge
							variant={reportsAcceptance &&
							task.criteria.length > 0 &&
							task.criteria.every((id) => task.passed_criteria.includes(id))
								? 'chip-active'
								: 'chip-muted'}
						>
							{reportsAcceptance ? task.passed_criteria.length : '?'} / {task.criteria.length} criteria
							accepted
						</Badge>
						<span>Work state: {task.state}</span>
					</div>
				</li>
			{/each}
		</ul>
		<ul class="list-none divide-y divide-[var(--border-subtle)]" aria-label="Agreed criteria">
			{#each status.criteria as criterion (criterion.id)}
				<li class="flex flex-col gap-2 py-3" data-slot="fleet-criterion-status">
					<p class="text-caption whitespace-pre-wrap break-words text-[var(--text-1)]">
						{criterion.id} · {criterion.text}
					</p>
					<p class="text-caption text-[var(--text-3)]">
						{criterion.ready ?? '?'} / {criterion.tasks} tasks ready · {reportsAcceptance
							? criterion.passed
							: '?'} / {criterion.tasks} tasks accepted
					</p>
				</li>
			{/each}
		</ul>
		<div class="flex flex-col gap-2">
			<h5 class="machine text-label text-[var(--text-3)]">Acceptance obligations</h5>
			{#if obligations.length === 0}
				<p class="text-caption text-[var(--text-3)]">No acceptance obligations recorded.</p>
			{/if}
			<ul
				class="list-none divide-y divide-[var(--border-subtle)]"
				aria-label="Acceptance obligations"
			>
				{#each obligations as obligation (obligation.note_id)}
					{@const nextAction = field(obligation.data, ['next_action'], '')}
					{@const owner = field(obligation.data, ['owner'], '')}
					{@const trigger = field(obligation.data, ['trigger'], '')}
					<li
						class="flex min-w-0 flex-col gap-3 py-3"
						data-slot="fleet-obligation"
						data-obligation-id={obligation.obligation_id}
					>
						<div class="flex flex-wrap items-center gap-2">
							<Badge variant={outcomeVariant(obligation)}
								>{obligation.issue
									? 'Needs attention'
									: obligation.outcome.replaceAll('_', ' ')}</Badge
							>
							<span class="text-caption text-[var(--text-3)]"
								>{obligation.required ? 'Required' : 'Advisory'} · {obligation.obligation_id}</span
							>
						</div>
						<p class="text-sm whitespace-pre-wrap break-words text-[var(--text-1)]">
							{obligation.title}
						</p>
						<p class="text-caption text-[var(--text-3)]">
							Tasks: {obligation.tasks.join(', ') || 'Fleet-wide'} · Criteria: {obligation.criteria.join(
								', '
							) || 'Fleet-wide'}
						</p>
						{#if obligation.issue}
							<p
								class="text-caption whitespace-pre-wrap break-words text-[var(--status-danger-fg)]"
							>
								{obligation.issue}
							</p>
						{/if}
						<dl class="flex min-w-0 flex-col gap-2 text-caption">
							<dt class="text-[var(--text-3)]">Expected outcome</dt>
							<dd
								class="whitespace-pre-wrap break-words text-[var(--text-1)]"
								data-slot="obligation-expected"
							>
								{field(obligation.data, ['expected_outcome', 'expected'])}
							</dd>
							<dt class="text-[var(--text-3)]">Observed outcome</dt>
							<dd
								class="whitespace-pre-wrap break-words text-[var(--text-1)]"
								data-slot="obligation-observed"
							>
								{field(obligation.data, ['observation', 'observed_outcome', 'observed'])}
							</dd>
							<dt class="text-[var(--text-3)]">Evidence</dt>
							<dd class="whitespace-pre-wrap break-words text-[var(--text-1)]">
								{field(obligation.data, ['evidence', 'observations', 'observation_notes'])}
							</dd>
							{#if nextAction}
								<dt class="text-[var(--text-3)]">Next action</dt>
								<dd class="whitespace-pre-wrap break-words text-[var(--text-1)]">{nextAction}</dd>
							{/if}
							{#if owner}
								<dt class="text-[var(--text-3)]">Owner</dt>
								<dd class="whitespace-pre-wrap break-words text-[var(--text-1)]">{owner}</dd>
							{/if}
							{#if trigger}
								<dt class="text-[var(--text-3)]">Resume when</dt>
								<dd class="whitespace-pre-wrap break-words text-[var(--text-1)]">{trigger}</dd>
							{/if}
						</dl>
						<details class="min-w-0" data-slot="obligation-record-details">
							<summary
								class="cursor-pointer text-caption text-[var(--text-3)] focus-visible:outline-2 focus-visible:outline-[var(--interactive-focus-ring)]"
							>
								Record details
							</summary>
							{#if obligation.body}
								<p class="mt-2 text-caption whitespace-pre-wrap break-words text-[var(--text-2)]">
									{obligation.body}
								</p>
							{/if}
							<pre
								class="mt-2 min-w-0 whitespace-pre-wrap break-words text-caption text-[var(--text-2)]"
								data-slot="obligation-record">{JSON.stringify(obligation.data, null, 2)}</pre>
						</details>
						<a
							class="text-caption break-all text-[var(--interactive-active-fg)] underline"
							href={resolve(
								`/sessions/${encodeURIComponent(obligation.session_uuid)}/comms` as `/sessions/${string}/comms`
							)}
						>
							Source note {obligation.note_id}
						</a>
						<p class="text-caption text-[var(--text-3)]">
							Updated {formatClockAbsolute(obligation.updated_at ?? obligation.at)}
						</p>
					</li>
				{/each}
			</ul>
		</div>
	{/if}
</section>
