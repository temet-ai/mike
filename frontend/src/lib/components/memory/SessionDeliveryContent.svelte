<script lang="ts">
	import Badge from '$lib/components/shared/Badge.svelte';
	import DeliveryRequestLink from '$lib/components/memory/DeliveryRequestLink.svelte';
	import { formatRelative, formatClockAbsolute } from '$lib/time';
	import type { DeliveryTopic, SessionDelivery, TopicState } from '$lib/api/sessionDelivery';

	interface Props {
		sessionId: string;
		data: SessionDelivery;
	}

	let { sessionId, data }: Props = $props();

	const TOPIC_LABEL: Record<TopicState, string> = {
		recalled: 'recalled on request',
		delivered_in_full: 'delivered as full lessons',
		available: 'available, no delivery recorded'
	};

	function topicVariant(topic: DeliveryTopic): 'chip-active' | 'tag-blue' | 'chip-muted' {
		if (topic.state === 'recalled') return 'chip-active';
		if (topic.state === 'delivered_in_full') return 'tag-blue';
		return 'chip-muted';
	}
</script>

<div class="sdc" data-slot="session-delivery-content">
	<section class="sdc-section" aria-label="Lessons delivered">
		<h4 class="sdc-heading">Lessons delivered ({data.lessons.length})</h4>
		{#if data.lessons.length === 0}
			<p class="sdc-empty">No lesson was injected into this session as full text.</p>
		{:else}
			<ul class="sdc-list" role="list">
				{#each data.lessons as lesson (lesson.lesson_id)}
					<li class="sdc-row" data-slot="session-delivery-lesson">
						<div class="sdc-row__head">
							<Badge variant="tag-default">lesson #{lesson.lesson_id}</Badge>
							<Badge variant="tag-default">{lesson.kind}</Badge>
							{#if lesson.topic}<Badge variant="tag-blue">topic {lesson.topic}</Badge>{/if}
							<Badge variant="chip-active">full text injected {lesson.injection_count}×</Badge>
							{#if lesson.archived_at}
								<Badge variant="chip-warning"
									>now archived{lesson.archived_reason ? `: ${lesson.archived_reason}` : ''}</Badge
								>
							{/if}
							<span class="sdc-when" title={formatClockAbsolute(lesson.first_injected_at)}>
								first {formatRelative(lesson.first_injected_at)} · last {formatRelative(
									lesson.last_injected_at
								)}
							</span>
						</div>
						<p class="sdc-text">{lesson.text}</p>
						<div class="sdc-row__foot">
							<DeliveryRequestLink {sessionId} requestId={lesson.first_request_id} />
							{#if lesson.last_request_id !== lesson.first_request_id}
								<DeliveryRequestLink {sessionId} requestId={lesson.last_request_id} />
							{/if}
							<span class="sdc-dim"
								>{lesson.lanes.join(', ')}{lesson.personas.length > 0
									? ` · ${lesson.personas.join(', ')}`
									: ''}</span
							>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="sdc-section" aria-label="Topics">
		<h4 class="sdc-heading">Topics ({data.topics.length})</h4>
		{#if data.topics.length === 0}
			<p class="sdc-empty">
				No topic is available to this session's project, and none was recalled.
			</p>
		{:else}
			<ul class="sdc-list" role="list">
				{#each data.topics as topic (topic.topic)}
					<li class="sdc-row" data-slot="session-delivery-topic" data-state={topic.state}>
						<div class="sdc-row__head">
							<span class="sdc-name">{topic.topic}</span>
							<Badge variant={topicVariant(topic)}>{TOPIC_LABEL[topic.state]}</Badge>
							<span class="sdc-dim"
								>{topic.available_project_lessons} project and {topic.available_shared_lessons} shared
								lessons available now</span
							>
						</div>
						{#if topic.recall_request_ids.length > 0}
							<div class="sdc-row__foot">
								{#each topic.recall_request_ids as requestId (requestId)}
									<DeliveryRequestLink {sessionId} {requestId} />
								{/each}
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="sdc-section" aria-label="Skills loaded">
		<h4 class="sdc-heading">Skills loaded ({data.skills.length})</h4>
		{#if data.skills.length === 0}
			<p class="sdc-empty">This session never loaded a skill.</p>
		{:else}
			<ul class="sdc-list" role="list">
				{#each data.skills as skill (skill.name ?? 'unnamed')}
					<li class="sdc-row" data-slot="session-delivery-skill" data-state={skill.state}>
						<div class="sdc-row__head">
							<span class="sdc-name">{skill.name ?? 'unnamed skill'}</span>
							<Badge variant={skill.state === 'loaded' ? 'chip-active' : 'chip-error'}>
								{skill.state === 'loaded' ? 'loaded' : 'load failed'}
							</Badge>
							<span class="sdc-dim">
								{skill.load_count} load{skill.load_count === 1 ? '' : 's'}
								{#if skill.error_count > 0}, {skill.error_count} failed{/if}
								· first {formatRelative(skill.first_loaded_at)}
							</span>
						</div>
						<div class="sdc-row__foot">
							<DeliveryRequestLink {sessionId} requestId={skill.first_request_id} />
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="sdc-section" aria-label="Recall calls">
		<h4 class="sdc-heading">Recall calls the session made ({data.recall_tool_calls.length})</h4>
		{#if data.recall_tool_calls.length === 0}
			<p class="sdc-empty">The session called no recall or project-knowledge tool.</p>
		{:else}
			<ul class="sdc-list" role="list">
				{#each data.recall_tool_calls as call (call.tool_name)}
					<li class="sdc-row" data-slot="session-delivery-recall-call">
						<div class="sdc-row__head">
							<span class="sdc-name">{call.tool_name}</span>
							<Badge variant="tag-default">{call.call_count} calls</Badge>
							{#if call.error_count > 0}
								<Badge variant="chip-error">{call.error_count} failed</Badge>
							{/if}
						</div>
						<div class="sdc-row__foot">
							<DeliveryRequestLink {sessionId} requestId={call.first_request_id} />
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

<style>
	.sdc {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
	.sdc-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sdc-heading {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		font-weight: var(--weight-display);
		text-transform: uppercase;
		letter-spacing: var(--track-display);
		color: var(--text-2);
	}
	.sdc-empty {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--text-2);
		font-style: italic;
	}
	.sdc-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.sdc-row {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		border: var(--hairline) solid var(--line);
		border-radius: var(--r-tol);
		padding: var(--space-2) var(--space-3);
		background: var(--surface-2);
	}
	.sdc-row[data-state='recalled'],
	.sdc-row[data-state='loaded'] {
		border-color: var(--cyan-border);
		box-shadow: inset 2px 0 0 0 var(--signal-active);
	}
	.sdc-row__head,
	.sdc-row__foot {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
	.sdc-when {
		margin-left: auto;
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sdc-name {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-sm);
		color: var(--text-1);
	}
	.sdc-dim {
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.sdc-text {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-size: var(--text-sm);
		line-height: var(--leading-body);
		color: var(--text-1);
	}
</style>
