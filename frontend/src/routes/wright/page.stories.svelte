<!--
Page stories for /wright. The running proxy serves the remedies, investigations, repairs and
conversation routes only after their backends deploy, so these states run the real page against
fixtures in the shapes of artifacts/2026-11-14/wright/BUILD_PLAN.md (installWrightFetch).
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import WrightPage from './+page.svelte';
	import {
		CONFLICT_REASON,
		emptyScenario,
		installWrightFetch,
		messageFixture,
		messages,
		normalScenario,
		type WrightScenario
	} from '$lib/components/__fixtures__/wright';
	import { PAIR_STORAGE_KEY } from '$lib/wright/pairing';

	const { Story } = defineMeta({
		title: 'Routes/Wright',
		component: WrightPage,
		parameters: { layout: 'fullscreen' }
	});

	let posted: unknown[] = [];

	function setup(scenario: WrightScenario, paired: boolean) {
		return async () => {
			try {
				if (paired) localStorage.setItem(PAIR_STORAGE_KEY, 'story-token');
				else localStorage.removeItem(PAIR_STORAGE_KEY);
			} catch {
				// Storage blocked: the page renders unpaired.
			}
			posted = installWrightFetch(scenario).posted;
			return {};
		};
	}

	const PENDING: WrightScenario = {
		...normalScenario(),
		messages: [
			...messages(),
			messageFixture({ id: 5, body: 'Why is the checkpoint tool failing?' })
		],
		pending: true
	};
</script>

<Story
	name="Empty"
	loaders={[setup(emptyScenario(), true)]}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText('No conversation yet')).toBeVisible();
		await expect(canvas.getByText(/Nothing open\./)).toBeVisible();
		await expect(canvas.getByText(/Nothing to triage\./)).toBeVisible();
		await expect(canvas.getByText('Nothing done yet.')).toBeVisible();
		await expect(canvas.getByRole('button', { name: 'Send' })).toBeDisabled();
	}}
/>

<Story
	name="Open and closed problems"
	loaders={[setup(normalScenario(), true)]}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText('A tool called with the wrong inputs')).toBeVisible();
		await expect(canvas.getByText('Mail waiting for a seat that is not reading')).toBeVisible();

		const row = canvas
			.getByText('A tool called with the wrong inputs')
			.closest('li') as HTMLElement;
		await userEvent.click(within(row).getAllByRole('button')[0]!);
		await expect(
			await within(row).findByText('work_thread_checkpoint is called without a tldr')
		).toBeVisible();
		await expect(within(row).getByRole('list', { name: 'Repair 2 progress' })).toBeVisible();

		const earlier = canvas.getByRole('button', { name: /Healthy again/ });
		await expect(earlier).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(earlier);
		await expect(await canvas.findByText("mike's own tools failing")).toBeVisible();
		await expect(canvas.getAllByText('Healthy').length).toBeGreaterThan(0);
		await userEvent.click(earlier);

		await expect(
			canvas.getAllByText('Close orphan launch', { selector: '.action__text' }).length
		).toBeGreaterThanOrEqual(2);
		await userEvent.click(canvas.getByRole('button', { name: /Known fixes/ }));
		await expect(
			await canvas.findByText('Notify undelivered sender', { selector: '.fix__title' })
		).toBeVisible();
		await expect(canvas.getAllByText('Paused').length).toBeGreaterThan(0);
		await userEvent.click(canvas.getByRole('button', { name: /Known fixes/ }));
	}}
/>

<Story
	name="Triage queue"
	loaders={[setup(normalScenario(), true)]}
	play={async ({ canvas }) => {
		const band = await canvas.findByRole('button', { name: /Triage queue/ });
		await waitFor(() => expect(band).toHaveTextContent('1 waiting · 5 causes'));
		const queue = canvas.getByRole('region', { name: 'Triage queue' });
		const rows = within(queue)
			.getAllByRole('listitem', { name: '' })
			.filter((li) => li.matches('[data-slot="wright-cause-row"]'));
		// The server's order is kept: waiting first, then the rest by priority.
		await expect(rows.map((li) => li.getAttribute('data-state'))).toEqual([
			'waiting',
			'found',
			'filed',
			'known_fix',
			'queued'
		]);
		await expect(within(rows[0]!).getByText('blocked_dialog')).toBeVisible();
		await expect(within(rows[0]!).getByText(/^16 problems/)).toBeVisible();
		await expect(within(rows[2]!).getByRole('link', { name: /Issue #272/ })).toHaveAttribute(
			'href',
			'https://github.com/temet-ai/mike/issues/272'
		);

		// A click opens the representative problem, the same detail a problem row opens.
		await userEvent.click(within(rows[0]!).getByRole('button', { name: /decisions_health/ }));
		await expect(
			await within(rows[0]!).findByText('work_thread_checkpoint is called without a tldr')
		).toBeVisible();
	}}
/>

<Story
	name="Triage queue not deployed yet"
	loaders={[setup({ ...normalScenario(), causes: null }, true)]}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText('A tool called with the wrong inputs')).toBeVisible();
		await expect(
			await canvas.findByText('Not in the running mike yet.', {
				selector: '#wright-band-triage p'
			})
		).toBeVisible();
		await expect(canvas.queryByRole('alert')).toBeNull();
	}}
/>

<Story
	name="Follow-ups"
	loaders={[setup(normalScenario(), true)]}
	play={async ({ canvas }) => {
		const band = await canvas.findByRole('button', { name: /Follow-ups/ });
		await waitFor(() => expect(band).toHaveTextContent('1 stuck · 6 listed'));
		const stuck = (
			await canvas.findByText('Mail to a seat that never reads is retried forever', {
				exact: false
			})
		).closest('li') as HTMLElement;
		await expect(
			within(stuck).getByText('3 sessions ended without a fix. Wright has stopped trying.')
		).toBeVisible();
		await expect(within(stuck).getByRole('link', { name: /Open issue #277/ })).toBeVisible();
		await expect(within(stuck).getByRole('link', { name: /Problem 703/ })).toHaveAttribute(
			'href',
			'#problem-703'
		);

		const show = canvas.getByRole('combobox', { name: 'Show' });
		await userEvent.selectOptions(show, 'handed_off');
		await waitFor(() =>
			expect(canvas.queryByText(/Mail to a seat that never reads is retried/)).toBeNull()
		);
		await expect(
			canvas.getByText('Raise the slow-query threshold for the capture schema')
		).toBeVisible();
		await waitFor(() => expect(band).toHaveTextContent('1 listed'));
		await userEvent.selectOptions(show, 'all');
		await expect(
			await canvas.findByText(/Mail to a seat that never reads is retried/)
		).toBeVisible();
	}}
/>

<Story
	name="Undo conflict"
	loaders={[setup({ ...normalScenario(), undo: 'conflict' }, true)]}
	play={async ({ canvas }) => {
		const undo = await canvas.findByRole('button', {
			name: /^Undo: Close orphan launch, action 43/
		});
		await userEvent.click(undo);
		await waitFor(() =>
			expect(canvas.getByRole('alert', { name: '' })).toHaveTextContent(
				`Couldn't undo: ${CONFLICT_REASON}`
			)
		);
	}}
/>

<Story
	name="Unpaired"
	loaders={[setup(normalScenario(), false)]}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText(/This browser is not paired/)).toBeVisible();
		await expect(canvas.getByText('mike wright pair --open')).toBeVisible();
		await expect(canvas.getByRole('textbox', { name: 'Message Wright' })).toBeDisabled();
		await expect(canvas.getByRole('button', { name: 'Send' })).toBeDisabled();
		await expect(await canvas.findByRole('button', { name: /^Undo: Paused/ })).toBeDisabled();
		await expect(canvas.getByText('Paused: Close orphan launch')).toBeVisible();
	}}
/>

<Story
	name="Pending reply"
	loaders={[
		async () => {
			// The pairing link: the fragment holds the token once, the page keeps it and drops it.
			try {
				localStorage.removeItem(PAIR_STORAGE_KEY);
			} catch {
				// Storage blocked: the page renders unpaired.
			}
			posted = installWrightFetch(PENDING).posted;
			window.location.hash = '#pair=story-token';
			return {};
		}
	]}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText('Wright is reading the evidence')).toBeVisible();
		const box = canvas.getByRole('textbox', { name: 'Message Wright' });
		await waitFor(() => expect(box).toBeEnabled());
		await userEvent.type(box, 'And the launch fixer?');
		await userEvent.click(canvas.getByRole('button', { name: 'Send' }));
		await waitFor(() => expect(posted.length).toBe(1));
		await expect(posted[0]).toMatchObject({
			body: 'And the launch fixer?',
			authorization: 'Bearer story-token'
		});
		await expect(await canvas.findByText('And the launch fixer?')).toBeVisible();
		history.replaceState(null, '', window.location.pathname + window.location.search);
	}}
/>

<Story
	name="Not deployed yet"
	loaders={[
		setup(
			{
				...normalScenario(),
				actions: null,
				remedies: null,
				messages: null,
				findings: null,
				repairs: null,
				investigations: null,
				followups: null,
				causes: null
			},
			true
		)
	]}
	play={async ({ canvas }) => {
		await expect(await canvas.findByText('The conversation is not running yet')).toBeVisible();
		await expect(
			canvas.getByText(/Wright's known fixes appear here once they are deployed/)
		).toBeVisible();
		await expect(canvas.getByText('A tool called with the wrong inputs')).toBeVisible();
		await expect(
			await canvas.findByText('Not in the running mike yet.', {
				selector: '#wright-band-followups p'
			})
		).toBeVisible();
		await expect(canvas.queryByRole('alert')).toBeNull();
	}}
/>
