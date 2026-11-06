<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';
	import ProjectComms from './ProjectComms.svelte';
	import type {
		ParticipantIdentityDto,
		TagsResponse,
		ThreadDetailResponse,
		ThreadMessageDto,
		ThreadSummaryDto,
		ThreadsListResponse
	} from '$lib/comms/types';

	const PROJECT = '/mnt/data/projects/temet/temet-proxy';
	const FLEET_UUID = 'fleet-cr3-0000-0000-0000-000000000000';
	const SEAT_UUID = 'th-seat-0000-0000-0000-000000000000';
	const GOAL =
		'Everything milestone 25 still lists as not built, in order: the change record store, the what_changed answers, the Edit-error proof, the clone launcher refusal, and the handover note a successor reads first. Each seat takes one task and records its evidence before the coordinator closes the fleet.';
	const COORDINATOR: ParticipantIdentityDto = {
		session_uuid: 'e74ba661-4d5e-6f70-8192-a3b4c5d6e7f8',
		label: 'coordinator-e74ba661',
		role: null,
		agent_type: null
	};
	const TOOLING: ParticipantIdentityDto = {
		session_uuid: 'f85cb772-4d5e-6f70-8192-a3b4c5d6e7f8',
		label: 'tooling-T11-f85cb772',
		role: null,
		agent_type: null
	};
	const IDENTITIES = [COORDINATOR, TOOLING];

	function thread(overrides: Partial<ThreadSummaryDto>): ThreadSummaryDto {
		return {
			thread_uuid: 'th',
			subject: 'subject',
			created_at: '2026-08-10T09:00:00Z',
			last_message_at: '2026-08-10T12:10:00Z',
			resolved_at: null,
			created_by_session_uuid: COORDINATOR.session_uuid,
			participants: IDENTITIES.map((i) => i.session_uuid),
			participant_identities: IDENTITIES,
			tags: [],
			message_count: 2,
			unread_count: 0,
			worst_delivery_state: 'delivered',
			has_stale_pending: false,
			project: { project_uuid: 'proj-a', name: 'mike', path: PROJECT, pinned: true },
			fleet: null,
			...overrides
		};
	}

	const THREADS: ThreadSummaryDto[] = [
		thread({
			thread_uuid: SEAT_UUID,
			subject: 'Seat T11 reports the Edit-error proof',
			created_at: '2026-08-10T11:00:00Z',
			tags: ['review'],
			fleet: { fleet_uuid: FLEET_UUID, slug: 'change-record-3', state: 'running' }
		}),
		thread({
			thread_uuid: FLEET_UUID,
			subject: `Fleet change-record-3: ${GOAL}`,
			fleet: { fleet_uuid: FLEET_UUID, slug: 'change-record-3', state: 'running' }
		}),
		thread({ thread_uuid: 'th-plain', subject: 'Plain thread without a fleet', message_count: 5 })
	];

	const MIKE_BODY = Array.from(
		{ length: 24 },
		(_, i) => `Line ${i + 1}: seat ${i % 14} reports its task state and the evidence it recorded.`
	).join('\n');

	const MESSAGES: ThreadMessageDto[] = [
		{
			message_uuid: 'm-1',
			thread_uuid: FLEET_UUID,
			from_session_uuid: null,
			to_session_uuid: COORDINATOR.session_uuid,
			queued_at: '2026-08-10T10:00:00Z',
			claimed_at: null,
			sent_request_id: 92001,
			sent_at: null,
			consumed_at: null,
			dead_lettered_at: null,
			delivery_attempts: 0,
			status: 'delivered',
			in_reply_to_uuid: null,
			context_items: [],
			body: `mike digest: Fleet change-record-3 at 53s\n${MIKE_BODY}`
		}
	];

	const { Story } = defineMeta({
		title: 'Comms/ProjectComms',
		component: ProjectComms,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ projectPath: PROJECT }}
	loaders={[
		async () => {
			stubFetchJsonRouter((method, url) => {
				if (method === 'GET' && url.includes('/api/tags')) {
					return typed<TagsResponse>({ tags: [{ code: 'review', hue: 8, usage_count: 3 }] });
				}
				const detail = url.match(/\/api\/threads\/([^/?]+)$/);
				if (method === 'GET' && detail) {
					const found = THREADS.find((t) => t.thread_uuid === decodeURIComponent(detail[1] ?? ''));
					if (!found) return typed<{ error: string }>({ error: 'no such thread' });
					return typed<ThreadDetailResponse>({
						thread: found,
						messages: found.thread_uuid === FLEET_UUID ? MESSAGES : []
					});
				}
				if (method === 'GET' && url.includes('/api/threads')) {
					return typed<ThreadsListResponse>({ threads: THREADS, total_count: THREADS.length });
				}
				return typed<Record<string, never>>({});
			});
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await waitFor(() =>
			expect(canvas.getByText('Plain thread without a fleet')).toBeInTheDocument()
		);
		await expect(canvas.getByText('Comms · 3 threads')).toBeInTheDocument();
		await expect(canvas.getByRole('link', { name: 'Open in Comms' })).toHaveAttribute(
			'href',
			`/comms?project=${encodeURIComponent(PROJECT)}`
		);

		const subjects = Array.from(canvasElement.querySelectorAll('.thread-row__subject')).map(
			(e) => e.textContent ?? ''
		);
		await expect(subjects.length).toBe(3);
		await expect(subjects[0]).toContain('Seat T11 reports the Edit-error proof');
		await expect(subjects.findIndex((t) => t.includes(GOAL))).toBeGreaterThanOrEqual(0);
		await expect(subjects.findIndex((t) => t.startsWith('Fleet change-record-3'))).toBeLessThan(
			subjects.indexOf('Plain thread without a fleet')
		);
		await expect(canvas.getByText('fleet change-record-3')).toBeInTheDocument();
		await expect(canvas.getByText('5 messages')).toBeInTheDocument();

		const fleetRow = Array.from(canvasElement.querySelectorAll('.thread-row')).find((b) =>
			b.textContent?.includes('Fleet change-record-3')
		) as HTMLElement;
		await userEvent.click(fleetRow);
		await waitFor(() => expect(canvasElement.querySelector('.transcript__goal')).not.toBeNull());
		await expect(canvasElement.querySelector('.transcript__goal')?.textContent).toBe(GOAL);
		await expect(canvasElement.querySelector('.transcript-compose')).toBeNull();
		await expect(
			canvasElement.querySelector('[data-message-uuid="m-1"] .transcript-msg__sender')?.textContent
		).toContain('mike');
		await expect(canvas.getByRole('link', { name: 'Open in Comms' })).toHaveAttribute(
			'href',
			`/comms?project=${encodeURIComponent(PROJECT)}&thread=${FLEET_UUID}`
		);

		await userEvent.click(canvas.getByRole('button', { name: 'All threads' }));
		await waitFor(() =>
			expect(canvas.getByText('Plain thread without a fleet')).toBeInTheDocument()
		);
		await expect(canvasElement.querySelector('.transcript')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div class="w-[560px] bg-[var(--surface-bg)] p-6">
			<ProjectComms {...args} />
		</div>
	{/snippet}
</Story>
