<!--
Page stories for /wright. The running proxy serves the remedies, investigations, repairs and
conversation routes only after their backends deploy, so these states run the real page against
fixtures in the shapes of artifacts/2026-11-14/wright/BUILD_PLAN.md (installWrightFetch).
-->
<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, screen, userEvent, waitFor, within } from 'storybook/test';
	import WrightPage from './+page.svelte';
	import { openDialog } from '$lib/components/__fixtures__/dialogs';
	import {
		CONFLICT_REASON,
		emptyScenario,
		installWrightFetch,
		messageFixture,
		messages,
		normalScenario,
		type TriageCall,
		type WrightScenario
	} from '$lib/components/__fixtures__/wright';
	import { PAIR_STORAGE_KEY } from '$lib/wright/pairing';

	const { Story } = defineMeta({
		title: 'Routes/Wright',
		component: WrightPage,
		parameters: { layout: 'fullscreen' }
	});

	let posted: unknown[] = [];
	let triage: TriageCall[] = [];

	function setup(scenario: WrightScenario, paired: boolean) {
		return async () => {
			try {
				if (paired) localStorage.setItem(PAIR_STORAGE_KEY, 'story-token');
				else localStorage.removeItem(PAIR_STORAGE_KEY);
			} catch {
				// Storage blocked: the page renders unpaired.
			}
			({ posted, triage } = installWrightFetch(scenario));
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
			canvas.getAllByText('Tell undelivered senders', { selector: '.action__text' }).length
		).toBeGreaterThanOrEqual(2);
		await userEvent.click(canvas.getByRole('button', { name: /Known fixes/ }));
		await expect(
			await canvas.findByText('Tell undelivered senders', { selector: '.fix__title' })
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

		// A click opens the cause: its score, whether the engineer takes it and every open problem.
		await userEvent.click(
			within(rows[0]!).getByRole('button', { name: /decisions_health/, expanded: false })
		);
		await expect(await within(rows[0]!).findByText('How it is ranked')).toBeVisible();
		await expect(
			within(rows[0]!).getByText('The engineer takes it when it reaches the top of the queue.')
		).toBeVisible();
		const problem = await within(rows[0]!).findByRole('button', {
			name: /A tool called with the wrong inputs/
		});
		// A problem of the cause opens to the same detail a problem row opens.
		await userEvent.click(problem);
		await expect(
			await within(rows[0]!).findByText('work_thread_checkpoint is called without a tldr')
		).toBeVisible();
	}}
/>

<Story
	name="Triage queue actions"
	loaders={[setup(normalScenario(), true)]}
	play={async ({ canvas }) => {
		const band = await canvas.findByRole('button', { name: /Triage queue/ });
		await waitFor(() => expect(band).toHaveTextContent('1 waiting · 5 causes'));
		const queue = canvas.getByRole('region', { name: 'Triage queue' });
		const order = (): string[] =>
			[...queue.querySelectorAll('[data-slot="wright-cause-row"]')]
				.map((li) => li.id)
				.filter((id) => /^cause-\d+$/.test(id));
		await expect(order()).toEqual(['cause-10', 'cause-18', 'cause-21', 'cause-22', 'cause-23']);

		// Pin: the cause goes first at once, and the write carries the operator token.
		await userEvent.click(within(queue).getByRole('button', { name: 'Pin log_warnings' }));
		await waitFor(() => expect(order()[0]).toBe('cause-23'));
		await expect(within(queue.querySelector('#cause-23')!).getByText('Pinned')).toBeVisible();
		await expect(triage.at(0)).toMatchObject({
			method: 'PATCH',
			path: '/api/wright/causes/23',
			body: { pinned: true },
			authorization: 'Bearer story-token'
		});
		await userEvent.click(within(queue).getByRole('button', { name: 'Unpin log_warnings' }));
		await waitFor(() => expect(order()[0]).toBe('cause-10'));

		// Move down puts the cause between its new neighbours; Reset gives the formula back.
		await userEvent.click(
			within(queue).getByRole('button', { name: 'Move decisions_health down' })
		);
		await waitFor(() => expect(order().slice(0, 2)).toEqual(['cause-18', 'cause-10']));
		const moved = queue.querySelector('#cause-10') as HTMLElement;
		await expect(within(moved).getByText('Moved by you')).toBeVisible();
		await userEvent.click(
			within(moved).getByRole('button', { name: 'Reset the order of decisions_health' })
		);
		await waitFor(() => expect(order()[0]).toBe('cause-10'));
		await expect(within(moved).queryByText('Moved by you')).toBeNull();

		// Merge two causes, then separate one again.
		await userEvent.click(within(queue).getByRole('checkbox', { name: 'Select undelivered_mail' }));
		await userEvent.click(within(queue).getByRole('checkbox', { name: 'Select log_warnings' }));
		await userEvent.click(within(queue).getByRole('button', { name: 'Merge into' }));
		const dialog = within(await openDialog('Merge 2 causes'));
		await userEvent.click(dialog.getByRole('button', { name: 'Merge' }));
		await waitFor(() => expect(band).toHaveTextContent('1 waiting · 4 causes'));
		await expect(order()).toEqual(['cause-10', 'cause-18', 'cause-21', 'cause-22']);
		// The dialog leaves the page inert for a moment as it closes.
		await waitFor(() => expect(getComputedStyle(document.body).pointerEvents).not.toBe('none'));
		const root = queue.querySelector('#cause-22') as HTMLElement;
		await expect(within(root).getByText('1 merged')).toBeVisible();
		await userEvent.click(
			within(root).getByRole('button', { name: /undelivered_mail/, expanded: false })
		);
		await userEvent.click(
			await within(root).findByRole('button', { name: 'Separate log_warnings' })
		);
		const sep = within(await openDialog('Separate this merge'));
		await userEvent.click(sep.getByRole('button', { name: 'Separate' }));
		await waitFor(() => expect(band).toHaveTextContent('1 waiting · 5 causes'));
	}}
/>

<Story
	name="Triage queue not paired"
	loaders={[setup(normalScenario(), false)]}
	play={async ({ canvas }) => {
		const queue = await canvas.findByRole('region', { name: 'Triage queue' });
		await waitFor(() =>
			expect(within(queue).getAllByRole('button', { name: /^Pin / })).not.toHaveLength(0)
		);
		// The reason is on the page once, and every action is off.
		await expect(
			within(queue).getByText(/This browser is not paired, so you can read the queue/)
		).toBeVisible();
		for (const button of within(queue).getAllByRole('button', { name: /^(Pin|Add) / })) {
			await expect(button).toBeDisabled();
		}
		// Selecting is only a choice on the page, so it works; what it leads to is off.
		await userEvent.click(within(queue).getByRole('checkbox', { name: 'Select undelivered_mail' }));
		await userEvent.click(within(queue).getByRole('checkbox', { name: 'Select log_warnings' }));
		await expect(within(queue).getByRole('button', { name: 'Merge into' })).toBeDisabled();
		await expect(within(queue).getByRole('button', { name: 'Group' })).toBeDisabled();
		// Reading still works: the view switch and a row open.
		await userEvent.click(within(queue).getByRole('tab', { name: 'By area' }));
		await expect(within(queue).getAllByRole('heading', { level: 3 }).length).toBeGreaterThan(0);
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
			name: /^Undo: Tell undelivered senders, action 43/
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
		await expect(canvas.getByText('Paused: Tell undelivered senders')).toBeVisible();
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
