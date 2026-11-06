<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WorkerCard from './WorkerCard.svelte';
	import DestinationNav from '$lib/components/shell/DestinationNav.svelte';

	const LESSON_CONFIG = [
		{
			label: 'Dedup threshold',
			value: '0.85',
			help: 'Similarity score (0–1) above which two lessons are treated as duplicates and merged. Higher = only near-identical lessons merge; lower = more aggressive deduplication.'
		},
		{
			label: 'Max lessons per project',
			value: '50',
			help: 'Cap on how many consolidated lessons are kept per project. Past this, the lowest-value lessons are evicted.'
		},
		{
			label: 'Session consolidation idle',
			value: '300s',
			help: 'How long a session must be idle before its raw lessons are consolidated into the store. 300s ≈ 5 minutes.'
		},
		{
			label: 'Nightly consolidation hour',
			value: '3',
			help: 'Hour of day (0–23, local time) for the nightly full consolidation pass. 3 = 3am.'
		},
		{
			label: 'GC interval',
			value: '86400s',
			help: 'How often the worker garbage-collects expired or low-value lessons. 86400s = once a day.'
		}
	];

	const RESYNC_CONFIG = [
		{
			label: 'Remote DB',
			value: 'unreachable',
			help: "Whether the remote infra database was reachable on the worker's most recent tick."
		},
		{
			label: 'Last run',
			value: 'never',
			help: "When the resync worker last ran. Stays 'never' until the first tick produces rows."
		},
		{
			label: 'Last sync',
			value: '0 rows',
			help: 'Rows merged from the local fallback back into the remote DB on the last run.'
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Settings/Workers',
		component: WorkerCard,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<script lang="ts">
	let lessonOn = $state(true);
	let harvesterOn = $state(true);
	let dbResyncOn = $state(true);
</script>

<Story
	name="Page"
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/settings/workers'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.querySelector('a[href="/settings/workers"]')).not.toBeNull();
		await expect(canvasElement.querySelectorAll('.dsc-card').length).toBe(3);
		const harvester = [...canvasElement.querySelectorAll('.dsc-card')].find(
			(c) => c.querySelector('.panel-hd')?.textContent?.trim() === 'Harvester'
		);
		await expect(harvester?.querySelector('.dsc-body')).toBeNull();
	}}
>
	{#snippet template()}
		<div class="shell">
			<div class="nav"><DestinationNav /></div>
			<div class="content">
				<header class="hd">
					<h1 class="title">Workers</h1>
					<p class="sub">
						Background workers that run across all sessions. Changes apply on the next proxy
						restart.
					</p>
				</header>

				<h2 class="sr-only">Worker list</h2>

				<WorkerCard
					name="lesson_worker"
					label="Lesson worker"
					description="Periodic worker that consolidates raw lessons mined from sessions into the project memory store, deduplicating before it writes."
					enabled={lessonOn}
					onToggle={(v) => (lessonOn = v)}
					config={LESSON_CONFIG}
				/>

				<WorkerCard
					name="harvester"
					label="Harvester"
					description="Mines completed operator turns for durable lessons — corrections the model made, conventions it followed, warnings it issued. Runs automatically with no tunable parameters."
					enabled={harvesterOn}
					onToggle={(v) => (harvesterOn = v)}
				/>

				<WorkerCard
					name="db_resync"
					label="Local DB resync"
					description="When the remote infra database is unreachable the proxy captures to a local Postgres; this worker merges those local-only rows back into the remote database when it returns."
					enabled={dbResyncOn}
					onToggle={(v) => (dbResyncOn = v)}
					config={RESYNC_CONFIG}
				/>
			</div>
		</div>
	{/snippet}
</Story>

<style>
	.shell {
		display: flex;
		height: 1100px;
		background: var(--surface-bg);
	}
	.nav {
		width: 240px;
		flex-shrink: 0;
		position: relative;
		display: flex;
		border-right: 1px solid var(--border-subtle);
	}
	.content {
		flex: 1 1 auto;
		min-width: 0;
		overflow-y: auto;
		padding: 28px 44px 64px;
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.hd {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.title {
		font-family: var(--font-display);
		font-size: 20px;
		font-weight: 700;
		color: var(--text-strong);
		margin: 0;
		letter-spacing: -0.01em;
	}
	.sub {
		margin: 0;
		font-size: 13px;
		color: var(--text-2);
		line-height: 1.6;
		max-width: 760px;
	}
</style>
