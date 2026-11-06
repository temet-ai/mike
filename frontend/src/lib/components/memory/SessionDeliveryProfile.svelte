<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import { formatClockAbsolute } from '$lib/time';
	import type { DeliveryToggle, SessionDelivery, SettingBasis } from '$lib/api/sessionDelivery';

	interface Props {
		data: SessionDelivery;
	}

	let { data }: Props = $props();

	const profile = $derived(data.profile);

	const BASIS_LABEL: Record<SettingBasis, string> = {
		session_frozen: 'frozen when the session started',
		change_log_at_session_start: 'from the change log at session start',
		current_settings: 'current settings'
	};

	function stateLabel(toggle: DeliveryToggle): string {
		if (toggle.enabled === null) return 'unknown';
		return toggle.enabled ? 'on' : 'off';
	}

	function stateVariant(toggle: DeliveryToggle): 'chip-active' | 'chip-muted' | 'chip-warning' {
		if (toggle.enabled === null) return 'chip-warning';
		return toggle.enabled ? 'chip-active' : 'chip-muted';
	}
</script>

<div class="sdp" data-slot="session-delivery-profile">
	<h4 class="sdp-heading">Effective profile</h4>
	<dl class="sdp-facts">
		<dt>Project</dt>
		<dd>
			{profile.project.name}
			<span class="sdp-dim">{profile.project.path} · resolved by {profile.project.resolution}</span>
		</dd>
		<dt>Seen</dt>
		<dd>{formatClockAbsolute(profile.first_seen)} to {formatClockAbsolute(profile.last_seen)}</dd>
		<dt>Models</dt>
		<dd>
			{#each profile.models as model (model.model ?? 'none')}
				<Badge variant="tag-default"
					>{model.model ?? 'unknown model'} · {model.request_count} requests</Badge
				>
			{:else}
				<span class="sdp-dim">No requests captured.</span>
			{/each}
		</dd>
		<dt>Agents</dt>
		<dd>
			{#each profile.agents as agent, index (index)}
				<Badge variant="tag-blue">{agent.persona ?? agent.agent_name ?? 'agent'}</Badge>
			{:else}
				<span class="sdp-dim">Main session only.</span>
			{/each}
		</dd>
		<dt>Lesson limits</dt>
		<dd>
			{profile.limits.inject_lessons ? 'lessons on' : 'lessons off'}, up to
			{profile.limits.max_lessons_to_inject} per reminder and
			{profile.limits.max_lessons_per_topic} per recalled topic
			<span class="sdp-dim">({BASIS_LABEL[profile.limits.basis]})</span>
		</dd>
	</dl>

	<ul class="sdp-toggles" role="list" aria-label="Delivery toggles for this session">
		{#each profile.toggles as toggle (toggle.key)}
			<li class="sdp-toggle" data-slot="session-delivery-toggle" data-key={toggle.key}>
				<span class="sdp-toggle__key">{toggle.key}</span>
				<Badge variant={stateVariant(toggle)}>{stateLabel(toggle)}</Badge>
				{#if toggle.changed_since_session_start}
					<Badge variant="chip-warning"
						>changed since, now {toggle.enabled_now ? 'on' : 'off'}</Badge
					>
				{/if}
				<span class="sdp-dim">{BASIS_LABEL[toggle.basis]}</span>
			</li>
		{/each}
	</ul>

	{#if profile.toggle_changes.length > 0}
		<h4 class="sdp-heading">Toggle changes since the session started</h4>
		<ul class="sdp-changes" role="list">
			{#each profile.toggle_changes as change (change.key + change.changed_at)}
				<li>
					<span class="sdp-toggle__key">{change.key}</span>
					{change.enabled_before === null ? 'created' : change.enabled_before ? 'on' : 'off'}
					to {change.enabled_after === null ? 'removed' : change.enabled_after ? 'on' : 'off'}
					<span class="sdp-dim"
						>{formatClockAbsolute(change.changed_at)} via {change.via} · {change.during_session
							? 'while the session was running'
							: 'after the session'}</span
					>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.sdp {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sdp-heading {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
	}
	.sdp-facts {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: var(--space-1) var(--space-3);
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.sdp-facts dt {
		color: var(--text-2);
	}
	.sdp-facts dd {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		align-items: center;
	}
	.sdp-dim {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sdp-toggles,
	.sdp-changes {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--text-sm);
	}
	.sdp-toggle {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.sdp-toggle__key {
		font-family: var(--font-mono, monospace);
		color: var(--text-1);
	}
</style>
