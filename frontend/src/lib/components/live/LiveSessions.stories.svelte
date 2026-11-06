<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import LiveSessions from './LiveSessions.svelte';
	import type { SessionSummary } from '$lib/types';
	import { buildSessionSummary } from '$lib/components/__fixtures__/session';
	import { buildMockSessionListStore } from '$lib/test-support/storyMocks';

	const STATIC_NOW = new Date('2026-04-17T12:10:00Z').getTime();

	const { Story } = defineMeta({
		title: 'Routes/Home/LiveSessions',
		component: LiveSessions,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});

	const FEW: SessionSummary[] = [
		buildSessionSummary({
			id: 'sess-a1',
			project: '/Users/dev/project-a',
			last_seen: '2026-04-17T12:08:00Z',
			request_count: 23,
			avg_latency_ms: 4_200,
			last_status: 'Complete',
			title: 'Refactor auth middleware'
		}),
		buildSessionSummary({
			id: 'sess-b1',
			project: '/Users/dev/other',
			last_seen: '2026-04-17T12:04:00Z',
			request_count: 5,
			avg_latency_ms: 1_100,
			last_status: 'Complete'
		}),
		buildSessionSummary({
			id: 'sess-c1',
			project: null,
			last_seen: '2026-04-17T11:40:00Z',
			request_count: 1,
			avg_latency_ms: null,
			last_status: 'Error: upstream 529'
		})
	];

	const STREAMING: SessionSummary[] = [
		buildSessionSummary({
			id: 'sess-live',
			project: '/Users/dev/project-a',
			last_seen: '2026-04-17T12:09:55Z',
			request_count: 31,
			avg_latency_ms: 6_400,
			last_status: 'Streaming',
			title: 'Live debugging session'
		}),
		...FEW
	];

	const MANY: SessionSummary[] = Array.from({ length: 30 }, (_, i) =>
		buildSessionSummary({
			id: `sess-${i}`,
			project: i % 3 === 0 ? '/Users/dev/project-a' : '/Users/dev/project-b',
			last_seen: `2026-04-17T1${1 - (i % 2)}:${(59 - i).toString().padStart(2, '0')}:00Z`,
			request_count: (i * 7) % 40,
			avg_latency_ms: i % 5 === 0 ? null : 800 + i * 120,
			last_status: i % 4 === 0 ? 'Streaming' : 'Complete',
			title: i % 2 === 0 ? `Session number ${i} with a descriptive title` : null
		})
	);
</script>

<Story
	name="Empty"
	args={{ store: buildMockSessionListStore({ sessions: [] }), now: STATIC_NOW }}
/>

<Story
	name="Loading"
	args={{ store: buildMockSessionListStore({ sessions: [], loading: true }), now: STATIC_NOW }}
/>

<Story
	name="FewSessions"
	args={{ store: buildMockSessionListStore({ sessions: FEW }), now: STATIC_NOW }}
/>

<Story
	name="LiveStreaming"
	args={{
		store: buildMockSessionListStore({ sessions: STREAMING, liveIds: ['sess-live'] }),
		now: STATIC_NOW
	}}
/>

<Story name="ManySessions">
	{#snippet template(args)}
		<div style="height: 420px; display: flex; max-width: 720px;">
			<LiveSessions
				{...args}
				store={buildMockSessionListStore({ sessions: MANY })}
				now={STATIC_NOW}
			/>
		</div>
	{/snippet}
</Story>
