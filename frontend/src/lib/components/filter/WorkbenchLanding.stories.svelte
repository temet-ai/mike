<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { setContext } from 'svelte';
	import { expect, userEvent, within } from 'storybook/test';
	import WorkbenchLanding from './WorkbenchLanding.svelte';
	import { createCollapseStore } from '$lib/stores/collapseState.svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CatalogueClass } from './promptCatalogue.svelte';
	import type { FilterRuleDto, PromptClassVersionDto } from '$lib/types';

	const SONNET_COLLAPSE_KEY = 'filter-workbench:model:claude-sonnet-5';

	function seedCollapsed(key: string): void {
		try {
			const raw = localStorage.getItem('proxy-inspector:collapsed');
			const parsed: unknown = raw ? JSON.parse(raw) : [];
			const keys = Array.isArray(parsed)
				? parsed.filter((v): v is string => typeof v === 'string')
				: [];
			if (!keys.includes(key)) keys.push(key);
			localStorage.setItem('proxy-inspector:collapsed', JSON.stringify(keys));
		} catch {}
	}

	const RULE_SYSTEM_REMINDER: FilterRuleDto = {
		id: 'rule-landing-1',
		kind: 'xml_tag',
		value: 'system-reminder',
		action: 'drop',
		text: null,
		title: null,
		enabled: true,
		source: 'ui',
		updated_at: '2026-07-28T09:00:00Z',
		trigger_token: null,
		sort_order: 1,
		fires_24h: 12,
		fires_7d: 84,
		last_fired_at: '2026-07-30T10:00:00Z'
	};

	const RULE_SVELTE: FilterRuleDto = {
		id: 'rule-landing-2',
		kind: 'text_substring',
		value: 'Svelte 5',
		action: 'replace',
		text: 'Svelte',
		title: null,
		enabled: true,
		source: 'ui',
		updated_at: '2026-07-28T09:05:00Z',
		trigger_token: null,
		sort_order: 2,
		fires_24h: 3,
		fires_7d: 9,
		last_fired_at: '2026-07-29T14:00:00Z'
	};

	const ORDERED_RULES = [RULE_SYSTEM_REMINDER, RULE_SVELTE];

	function versionEntry(
		version: string,
		requestIdFable: string | null,
		requestIdSonnet: string | null = null
	): PromptClassVersionDto {
		const models: PromptClassVersionDto['models'] = [];
		if (requestIdFable !== null) {
			models.push({
				model: 'claude-fable-5-1',
				observation_count: 9,
				first_seen: '2026-09-05T00:00:00Z',
				last_seen: '2026-09-07T15:09:31Z',
				latest_request_id: requestIdFable,
				latest_session_id: '655150'
			});
		}
		if (requestIdSonnet !== null) {
			models.push({
				model: 'claude-sonnet-5',
				observation_count: 2,
				first_seen: '2026-09-06T00:00:00Z',
				last_seen: '2026-09-06T11:00:00Z',
				latest_request_id: requestIdSonnet,
				latest_session_id: '655140'
			});
		}
		return {
			version,
			builds: 2,
			observation_count: models.reduce((n, m) => n + m.observation_count, 0),
			first_seen: '2026-09-05T00:00:00Z',
			last_seen: '2026-09-07T15:09:31Z',
			models
		};
	}

	const CLASSES: CatalogueClass[] = [
		{
			key: 'main:',
			lane: 'main',
			persona: null,
			title: 'Main session',
			registered: false,
			registrationLabel: null,
			blockCount: 4,
			observationCount: 532,
			lastSeen: '2026-09-07T15:09:31Z',
			firstSeen: '2026-07-01T00:00:00Z',
			noRoute: false,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [
				versionEntry('2.1.263', '235311', '235290'),
				versionEntry('2.1.257', '235280', '235270'),
				versionEntry('2.1.252', '235260'),
				versionEntry('2.1.248', '235250'),
				versionEntry('2.1.245', '235240'),
				versionEntry('2.1.243', '235230'),
				versionEntry('2.1.241', '235220')
			],
			models: [
				{
					model: 'claude-fable-5-1',
					unscoped: false,
					windowedCount: 520,
					latestObservation: {
						requestId: '235311',
						sessionId: '655150',
						createdAt: '2026-09-07T15:09:31Z'
					},
					sampleBlocks: [
						{ type: 'text', text: 'You are Claude Code, an AI coding assistant.' },
						{
							type: 'text',
							text: '<system-reminder>\nRun tests before committing.\n</system-reminder>'
						}
					],
					observationsTotalCount: null,
					status: 'ready',
					error: null
				},
				{
					model: 'claude-sonnet-5',
					unscoped: false,
					windowedCount: 12,
					latestObservation: {
						requestId: '235290',
						sessionId: '655140',
						createdAt: '2026-09-06T11:00:00Z'
					},
					sampleBlocks: null,
					observationsTotalCount: null,
					status: 'loading',
					error: null
				}
			]
		},
		{
			key: 'main:steer-author',
			lane: 'main',
			persona: 'steer-author',
			title: 'steer-author',
			registered: true,
			registrationLabel: 'Steering Author',
			blockCount: 0,
			observationCount: 0,
			lastSeen: null,
			firstSeen: null,
			noRoute: true,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [],
			models: []
		},
		{
			key: 'subagent:db-dev',
			lane: 'subagent',
			persona: 'db-dev',
			title: 'db-dev',
			registered: true,
			registrationLabel: 'db-dev',
			blockCount: 2,
			observationCount: 4,
			lastSeen: '2026-09-05T00:00:00Z',
			firstSeen: '2026-08-20T00:00:00Z',
			noRoute: false,
			versionsStatus: 'error',
			versionsError: 'network error: ECONNREFUSED',
			versions: [],
			models: [
				{
					model: 'claude-fable-5-1',
					unscoped: false,
					windowedCount: 4,
					latestObservation: {
						requestId: '236100',
						sessionId: '656200',
						createdAt: '2026-09-05T00:00:00Z'
					},
					sampleBlocks: [{ type: 'text', text: 'You are a Postgres and sqlx specialist.' }],
					observationsTotalCount: null,
					status: 'ready',
					error: null
				}
			]
		},
		{
			key: 'subagent:Frontend Development Expert',
			lane: 'subagent',
			persona: 'Frontend Development Expert',
			title: 'Frontend Development Expert',
			registered: true,
			registrationLabel: 'Frontend Development Expert',
			blockCount: 3,
			observationCount: 44,
			lastSeen: '2026-09-07T12:00:00Z',
			firstSeen: '2026-08-01T00:00:00Z',
			noRoute: false,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [versionEntry('2.1.260', null, '235400')],
			models: [
				{
					model: 'claude-sonnet-5',
					unscoped: false,
					windowedCount: 44,
					latestObservation: {
						requestId: '235400',
						sessionId: '655200',
						createdAt: '2026-09-07T12:00:00Z'
					},
					sampleBlocks: [
						{
							type: 'text',
							text: 'You are a Frontend Development Expert. Build production-grade UIs with Svelte 5.'
						}
					],
					observationsTotalCount: null,
					status: 'ready',
					error: null
				}
			]
		},
		{
			key: 'subagent:Rust Development Expert',
			lane: 'subagent',
			persona: 'Rust Development Expert',
			title: 'Rust Development Expert',
			registered: true,
			registrationLabel: 'Rust Development Expert',
			blockCount: 2,
			observationCount: 6,
			lastSeen: '2026-09-06T09:00:00Z',
			firstSeen: '2026-08-05T00:00:00Z',
			noRoute: false,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [versionEntry('2.1.255', '236050')],
			models: [
				{
					model: 'claude-fable-5-1',
					unscoped: false,
					windowedCount: 6,
					latestObservation: null,
					sampleBlocks: null,
					observationsTotalCount: null,
					status: 'error',
					error: 'network error: ECONNREFUSED'
				}
			]
		},
		{
			key: 'subagent:TUI Engineer',
			lane: 'subagent',
			persona: 'TUI Engineer',
			title: 'TUI Engineer',
			registered: false,
			registrationLabel: null,
			blockCount: 0,
			observationCount: 3,
			lastSeen: '2026-09-04T09:00:00Z',
			firstSeen: '2026-08-22T00:00:00Z',
			noRoute: false,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [versionEntry('2.1.250', null, '207933')],
			models: [
				{
					model: 'claude-sonnet-5',
					unscoped: false,
					windowedCount: 3,
					latestObservation: {
						requestId: '207933',
						sessionId: 'sess-tui-1',
						createdAt: '2026-09-04T09:00:00Z'
					},
					sampleBlocks: [],
					observationsTotalCount: null,
					status: 'ready',
					error: null
				}
			]
		},
		{
			key: 'subagent:ui-designer',
			lane: 'subagent',
			persona: 'ui-designer',
			title: 'ui-designer',
			registered: true,
			registrationLabel: 'ui-designer',
			blockCount: 0,
			observationCount: 8,
			lastSeen: '2026-09-06T00:00:00Z',
			firstSeen: '2026-08-25T00:00:00Z',
			noRoute: false,
			versionsStatus: 'loading',
			versionsError: null,
			versions: [],
			models: [
				{
					model: 'claude-fable-5-1',
					unscoped: false,
					windowedCount: 8,
					latestObservation: {
						requestId: '236200',
						sessionId: '656300',
						createdAt: '2026-09-06T00:00:00Z'
					},
					sampleBlocks: null,
					observationsTotalCount: null,
					status: 'loading',
					error: null
				}
			]
		},
		{
			key: 'subagent:unit-tester',
			lane: 'subagent',
			persona: 'unit-tester',
			title: 'unit-tester',
			registered: true,
			registrationLabel: 'unit-tester',
			blockCount: 0,
			observationCount: 0,
			lastSeen: null,
			firstSeen: null,
			noRoute: false,
			versionsStatus: 'ready',
			versionsError: null,
			versions: [],
			models: []
		}
	];

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchLanding',
		component: WorkbenchLanding,
		tags: ['autodocs']
	});
</script>

<Story
	name="Populated"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const modelTitles = Array.from(canvasElement.querySelectorAll('.wbl-model__title')).map((el) =>
			el.textContent?.trim()
		);
		await expect(modelTitles).toEqual([
			'claude-fable-5-1',
			'claude-sonnet-5',
			'Registered, never observed'
		]);

		const sonnetModel = canvasElement.querySelectorAll('.wbl-model')[1] as HTMLElement;
		const sonnetToggle = within(sonnetModel).getByRole('button', { name: 'claude-sonnet-5' });
		await expect(sonnetToggle).toHaveAttribute('aria-expanded', 'false');
		await expect(within(sonnetModel).queryByText('checking prompt…')).toBeNull();
		await expect(within(sonnetModel).getByText(/obs \(7d\)/)).toBeInTheDocument();

		await userEvent.click(sonnetToggle);
		await expect(sonnetToggle).toHaveAttribute('aria-expanded', 'true');

		const fableModel = canvasElement.querySelectorAll('.wbl-model')[0] as HTMLElement;
		const fableClassTitles = Array.from(fableModel.querySelectorAll('.wbl-class__title')).map(
			(el) => el.textContent?.replace(/\s+/g, ' ').trim()
		);
		await expect(fableClassTitles).toEqual([
			'Main session main',
			'db-dev subagent',
			'Rust Development Expert subagent',
			'ui-designer subagent'
		]);

		await expect((await canvas.findAllByText('1 edit changes this')).length).toBe(2);
		await expect(await canvas.findByText('Removes “system-reminder”')).toBeInTheDocument();
		await expect(await within(fableModel).findByText('Claude Code 2.1.263')).toBeInTheDocument();

		await expect(canvas.queryByText('Claude Code 2.1.241')).toBeNull();
		const disclosure = await canvas.findByRole('button', { name: '2 older versions' });
		await userEvent.click(disclosure);
		await expect(await canvas.findByText('Claude Code 2.1.241')).toBeInTheDocument();

		await expect(await canvas.findByText('checking prompt versions…')).toBeInTheDocument();

		await expect(await canvas.findByText(/couldn't load prompt content/)).toBeInTheDocument();

		await expect(await canvas.findByText(/couldn't load release history/)).toBeInTheDocument();
		await expect((await canvas.findAllByRole('button', { name: 'Retry' })).length).toBeGreaterThan(
			1
		);

		await expect(await canvas.findByText('checking prompt…')).toBeInTheDocument();
		await expect(await canvas.findByText('Rewrites “Svelte 5”')).toBeInTheDocument();
		await expect(
			await canvas.findByText(
				'no prompt content sampled — open in workbench to match rules against the last captured prompt'
			)
		).toBeInTheDocument();

		await expect(await canvas.findByText('steer-author')).toBeInTheDocument();
		await expect(await canvas.findByText('registered · Steering Author')).toBeInTheDocument();
		await expect(await canvas.findByText('unit-tester')).toBeInTheDocument();

		const openLink = await canvas.findByRole('link', {
			name: 'Open Main session (claude-fable-5-1, Claude Code 2.1.263) in the workbench'
		});
		await expect(openLink).toHaveAttribute('href', '/filter/workbench?from=request:235311');
		await expect(within(openLink).getByText('Claude Code 2.1.263')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		{@const _seed = seedCollapsed(SONNET_COLLAPSE_KEY)}
		{@const collapseStore = createCollapseStore()}
		{@const _ctx = setContext(COLLAPSE_STORE_KEY, collapseStore)}
		<div
			style="width:560px; height:760px; display:flex; flex-direction:column; border:1px solid var(--border-default,#333); background:var(--surface-bg,#13141a); font-family:var(--font-mono,'JetBrains Mono Variable',monospace);"
		>
			<WorkbenchLanding
				orderedRules={ORDERED_RULES}
				classes={CLASSES}
				status="ready"
				error={null}
				onRetryCatalogue={() => {}}
				onRetryModel={() => {}}
				onRetryVersions={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="Loading"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText('Loading captured prompts…')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="width:480px; height:200px;">
			<WorkbenchLanding
				orderedRules={ORDERED_RULES}
				classes={[]}
				status="loading"
				error={null}
				onRetryCatalogue={() => {}}
				onRetryModel={() => {}}
				onRetryVersions={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="CatalogueError"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(await canvas.findByText("Couldn't load the prompt catalogue")).toBeInTheDocument();
		await expect(await canvas.findByText('network error: ECONNREFUSED')).toBeInTheDocument();
		await expect(await canvas.findByRole('button', { name: 'Retry' })).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="width:480px; height:200px;">
			<WorkbenchLanding
				orderedRules={ORDERED_RULES}
				classes={[]}
				status="error"
				error="network error: ECONNREFUSED"
				onRetryCatalogue={() => {}}
				onRetryModel={() => {}}
				onRetryVersions={() => {}}
			/>
		</div>
	{/snippet}
</Story>
