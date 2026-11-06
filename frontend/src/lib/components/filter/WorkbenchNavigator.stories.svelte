<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import WorkbenchNavigator from './WorkbenchNavigator.svelte';
	import type { CatalogueClass } from './promptCatalogue.svelte';
	import type { PromptClassVersionDto } from '$lib/types';
	import type { PromptOutlineHeading } from './blockParagraphs';

	const OUTLINE: PromptOutlineHeading[] = [
		{ key: '0:0', blockIndex: 0, paragraphIndex: 0, level: 1, title: 'Identity', hasEdits: false },
		{
			key: '3:0',
			blockIndex: 3,
			paragraphIndex: 0,
			level: 1,
			title: 'Session-specific guidance',
			hasEdits: true
		}
	];

	function versionEntry(
		version: string,
		requestIdFable: string,
		requestIdSonnet: string | null = null
	): PromptClassVersionDto {
		const models: PromptClassVersionDto['models'] = [
			{
				model: 'claude-fable-5-1',
				observation_count: 9,
				first_seen: '2026-09-05T00:00:00Z',
				last_seen: '2026-09-07T15:09:31Z',
				latest_request_id: requestIdFable,
				latest_session_id: '655150'
			}
		];
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

	const MAIN_CLASS: CatalogueClass = {
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
			versionEntry('2.1.257', '235280'),
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
				sampleBlocks: null,
				observationsTotalCount: null,
				status: 'ready',
				error: null
			}
		]
	};

	const { Story } = defineMeta({
		title: 'Routes/Filter/WorkbenchNavigator',
		component: WorkbenchNavigator,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let lastJump = $state<string | null>(null);
</script>

<Story
	name="Selected"
	tags={['interactive', 'data-driven']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText('claude-fable-5-1')).toBeInTheDocument();
		await expect(await canvas.findByText('Main session')).toBeInTheDocument();
		await expect(await canvas.findByText('main')).toBeInTheDocument();

		await expect(canvas.queryByText('Claude Code 2.1.241')).toBeNull();
		await expect(
			await canvas.findByRole('button', { name: '2 older versions' })
		).toBeInTheDocument();

		const selectedLink = await canvas.findByRole('link', { current: true });
		await expect(selectedLink).toHaveTextContent('Claude Code 2.1.263');
		await expect(selectedLink).toHaveClass('wbn-version-row--selected');
		await expect(selectedLink).toHaveAttribute('href', '/filter/workbench?from=request:235311');

		const sonnetLink = await canvas.findByRole('link', {
			name: "Open claude-sonnet-5's current release"
		});
		await expect(sonnetLink).toHaveAttribute('href', '/filter/workbench?from=request:235290');

		await expect(await canvas.findByText(/#386063/)).toBeInTheDocument();
		const openRequestLink = await canvas.findByRole('link', { name: 'open request' });
		await expect(openRequestLink).toHaveAttribute('href', '/sessions/sess-1/requests/386063');

		// "In this prompt": every heading listed, an edit marker only on the section that has one.
		const identityJump = await canvas.findByRole('button', { name: 'Identity' });
		await expect(identityJump).toBeInTheDocument();
		const editedJump = await canvas.findByRole('button', {
			name: 'Session-specific guidance — has edits'
		});
		await userEvent.click(editedJump);
		await expect(lastJump).toBe('3:0');
	}}
>
	{#snippet template()}
		<div style="width:260px; height:640px; border:1px solid var(--border-default,#333);">
			<WorkbenchNavigator
				requestId={386063}
				sessionId="sess-1"
				model="claude-fable-5-1"
				backHref="/sessions/sess-1/requests/386063"
				blockCount={5}
				totalChars={18077}
				classIdentity={{ lane: 'main', persona: null }}
				catalogueClasses={[MAIN_CLASS]}
				catalogueStatus="ready"
				requestRelease="2.1.263"
				onRetryVersions={() => {}}
				outline={OUTLINE}
				onJumpToParagraph={(key) => (lastJump = key)}
				onToggleCollapse={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="NoMatchingRelease"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.querySelector('.wbn-version-row--selected')).toBeNull();
		await expect(
			await canvas.findByText(
				'this request carries release 2.1.199, outside the captured history above'
			)
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="width:260px; height:640px; border:1px solid var(--border-default,#333);">
			<WorkbenchNavigator
				requestId={386070}
				sessionId="sess-1"
				model="claude-fable-5-1"
				backHref="/sessions/sess-1/requests/386070"
				blockCount={5}
				totalChars={17500}
				classIdentity={{ lane: 'main', persona: null }}
				catalogueClasses={[MAIN_CLASS]}
				catalogueStatus="ready"
				requestRelease="2.1.199"
				onRetryVersions={() => {}}
				outline={[]}
				onJumpToParagraph={() => {}}
				onToggleCollapse={() => {}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="Uncatalogued"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(
			await canvas.findByText(
				"not catalogued — this request's turn origin isn't part of the prompt catalogue"
			)
		).toBeInTheDocument();
		await expect(canvasElement.querySelector('[aria-label="Prompt"]')).toBeNull();
		await expect(canvasElement.querySelector('[aria-label="Versions"]')).toBeNull();
		await expect(canvasElement.querySelector('[aria-label="Other models"]')).toBeNull();
		await expect(await canvas.findByText('claude-sonnet-5')).toBeInTheDocument();
		await expect(await canvas.findByText(/#9001/)).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div style="width:260px; height:400px; border:1px solid var(--border-default,#333);">
			<WorkbenchNavigator
				requestId={9001}
				sessionId="sess-2"
				model="claude-sonnet-5"
				backHref="/sessions/sess-2/requests/9001"
				blockCount={1}
				totalChars={40}
				classIdentity={null}
				catalogueClasses={[MAIN_CLASS]}
				catalogueStatus="ready"
				requestRelease={null}
				onRetryVersions={() => {}}
				outline={[]}
				onJumpToParagraph={() => {}}
				onToggleCollapse={() => {}}
			/>
		</div>
	{/snippet}
</Story>
