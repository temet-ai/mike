<!--
@component
Open alarms that need the operator's attention, each as one plain sentence with when it was first
seen and an Acknowledge action that asks for a note. The caller passes the alarms that belong on its
page (see `alarmsFor` in `$lib/components/judges/judges`). With no alarms and no error the section
renders nothing, so an empty box never shows.
-->
<script lang="ts">
	import { resolve } from '$app/paths';
	import type { EvalAlarmDto } from '$lib/api/evalMetrics';
	import { Button } from '$lib/components/ui/button';
	import DisclosureMarker from '$lib/components/shared/DisclosureMarker.svelte';
	import { formatWhen } from '$lib/components/eval/evalReview';
	import { alarmWords } from '$lib/components/judges/judges';
	import type { AlarmHome } from '$lib/components/judges/judges';

	let {
		alarms,
		error = null,
		home = null,
		onAcknowledge
	}: {
		alarms: EvalAlarmDto[];
		error?: string | null;
		home?: AlarmHome | null;
		onAcknowledge: (id: number, note: string) => Promise<void>;
	} = $props();

	let drafting: number | null = $state(null);
	let note = $state('');
	let saving = $state(false);
	let saveError: string | null = $state(null);
	let detailOpen = $state<Record<number, boolean>>({});

	function start(id: number): void {
		drafting = id;
		note = '';
		saveError = null;
	}

	async function submit(id: number): Promise<void> {
		if (!note.trim()) {
			saveError = 'Write a note first: why this is known or handled.';
			return;
		}
		saving = true;
		try {
			await onAcknowledge(id, note.trim());
			drafting = null;
		} catch (e) {
			saveError = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}
</script>

{#if error || alarms.length > 0}
	<section class="eal edge mech-group" aria-label="Needs attention" data-testid="alarms">
		<div class="band-hd band-hd--static">
			<h2 class="machine eal-title">Needs attention · {alarms.length}</h2>
		</div>
		{#if error}
			<p class="band-body eal-error" role="alert">The alarms could not be loaded: {error}</p>
		{/if}
		{#each alarms as alarm (alarm.id)}
			{@const words = alarmWords(alarm, home)}
			<div class="band-body eal-alarm" data-alarm-id={alarm.id}>
				<div class="eal-row">
					<p class="eal-sentence">
						{words.sentence}
						{#if words.sessionUuid}<a
								class="eal-link"
								href={resolve(
									`sessions/${encodeURIComponent(words.sessionUuid)}` as `sessions/${string}`
								)}>Open the session</a
							>{/if}
					</p>
					<span class="eal-since">since {formatWhen(alarm.first_seen_at)}</span>
					{#if !alarm.acknowledged_at && drafting !== alarm.id}
						<Button variant="outline" size="sm" onclick={() => start(alarm.id)}>Acknowledge</Button>
					{/if}
				</div>
				{#if words.advice || words.actions.length > 0}
					<div class="eal-help" data-testid="alarm-help">
						{#if words.advice}<p class="eal-advice">{words.advice}</p>{/if}
						{#if words.actions.length > 0}
							<ul class="eal-actions-list">
								{#each words.actions as action (action.label)}
									<li>
										<a class="eal-link" href={`${resolve(action.path)}${action.suffix}`}
											>{action.label}</a
										>
									</li>
								{/each}
							</ul>
						{/if}
					</div>
				{/if}
				{#if alarm.acknowledged_at}
					<p class="eal-ack">
						Acknowledged on {formatWhen(alarm.acknowledged_at)}: {alarm.ack_note}
					</p>
				{:else if drafting === alarm.id}
					<form
						class="eal-form"
						onsubmit={(e) => {
							e.preventDefault();
							void submit(alarm.id);
						}}
					>
						<label class="eal-field">
							<span class="machine eal-k">Note, saved with the acknowledgement</span>
							<textarea bind:value={note} rows="3" maxlength="2000"></textarea>
						</label>
						<div class="eal-actions">
							<Button type="submit" variant="default" size="sm" disabled={saving}
								>Save acknowledgement</Button
							>
							<Button variant="ghost" size="sm" onclick={() => (drafting = null)}>Cancel</Button>
						</div>
						{#if saveError}<p class="eal-error" role="alert">{saveError}</p>{/if}
					</form>
				{/if}
				{#if words.showDetail}
					<button
						type="button"
						class="eal-toggle"
						aria-expanded={detailOpen[alarm.id] ?? false}
						onclick={() =>
							(detailOpen = { ...detailOpen, [alarm.id]: !(detailOpen[alarm.id] ?? false) })}
					>
						<DisclosureMarker open={detailOpen[alarm.id] ?? false} size={10} /><span>Details</span>
					</button>
					{#if detailOpen[alarm.id]}<pre class="eal-pre">{JSON.stringify(
								alarm.detail,
								null,
								2
							)}</pre>{/if}
				{/if}
			</div>
		{/each}
	</section>
{/if}

<style>
	.eal-help {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-1) var(--space-3);
	}
	.eal-advice {
		margin: 0;
		max-width: 80ch;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.eal-actions-list {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--text-sm);
	}
	.eal {
		--edge: var(--edge-sm);
		min-width: 0;
	}
	.eal-title {
		margin: 0;
		font-size: var(--text-xs);
		font-weight: var(--weight-body);
		color: var(--status-warning-fg);
	}
	.eal-alarm {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding-top: var(--space-3);
	}
	.eal-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-4);
	}
	.eal-sentence {
		flex: 1 1 24rem;
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
		overflow-wrap: anywhere;
	}
	.eal-since {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eal-link {
		margin-left: var(--space-2);
		color: var(--interactive-active-fg);
		text-decoration: underline;
	}
	.eal-ack {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.eal-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.eal-field {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}
	.eal-k {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.eal-field textarea {
		padding: var(--space-2);
		background: var(--surface-bg);
		border: var(--hairline) solid var(--border-default);
		color: var(--text-1);
		font: inherit;
		font-size: var(--text-sm);
		resize: vertical;
	}
	.eal-field textarea:focus-visible,
	.eal-toggle:focus-visible,
	.eal-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.eal-actions {
		display: flex;
		gap: var(--space-2);
	}
	.eal-error {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--status-danger-fg);
	}
	.eal-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: var(--text-xs);
		color: var(--text-2);
		cursor: pointer;
	}
	.eal-pre {
		margin: 0;
		padding: var(--space-2);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		background: var(--surface-bg);
		font-size: var(--text-xs);
		color: var(--text-1);
	}
</style>
