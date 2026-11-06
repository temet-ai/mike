<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, waitFor, userEvent } from 'storybook/test';
	import SessionControlsPanel from './SessionControlsPanel.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type { SessionSteeringRow, SessionEnrolmentRow } from '$lib/api/steering';
	import type { AgentEntry } from '$lib/agentDispatches';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';

	const AGENTS: AgentEntry[] = [
		{
			subagentType: 'rust-developer',
			model: 'claude-sonnet-4-6',
			description: 'Fix the mailbox delivery race in persistence/mailbox.rs.',
			events: [
				{
					requestId: 2001,
					createdAt: '2026-07-02T09:10:00Z',
					outcome: 'completed',
					description: 'Fix the mailbox delivery race in persistence/mailbox.rs.'
				},
				{
					requestId: 2010,
					createdAt: '2026-07-02T09:25:00Z',
					outcome: 'error',
					description: 'Add a regression test for the race.'
				}
			]
		},
		{
			subagentType: 'general-purpose',
			model: null,
			description: 'Investigate why the /steering endpoint 404s for older sessions.',
			events: [
				{
					requestId: 2020,
					createdAt: '2026-07-02T09:40:00Z',
					outcome: null,
					description: 'Investigate why the /steering endpoint 404s for older sessions.'
				}
			]
		}
	];

	const STORY_SESSION = 'sess-controls-11111111-2222-3333-4444-555555555555';
	const STATIC_NOW = new Date('2026-07-02T12:00:00Z').getTime();

	const ENROLMENTS: SessionEnrolmentRow[] = [
		{
			enrolment_id: 501,
			session_id: STORY_SESSION,
			session_uuid: STORY_SESSION,
			steerer_id: 1,
			steerer_name: 'verifier',
			steerer_display_name: 'VerifierSteerer',
			steerer_enabled: true,
			mode: 'augment',
			authority: 'redirect',
			enrolled_at: '2026-07-02T09:00:00Z',
			enrolled_by: 'system',
			disenrolled_at: null
		}
	];

	function stubSteeringFetch(): () => void {
		const orig = globalThis.fetch;
		globalThis.fetch = (async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/enrolments')) {
				return new Response(JSON.stringify(ENROLMENTS), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			if (url.includes('/steering')) {
				return new Response(JSON.stringify([] as SessionSteeringRow[]), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404, statusText: 'Not Found' });
		}) as typeof fetch;
		return () => {
			globalThis.fetch = orig;
		};
	}

	const { Story } = defineMeta({
		title: 'Shell/SessionControlsPanel',
		component: SessionControlsPanel,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			localStorage.clear();
			const restore = stubSteeringFetch();
			return { restore };
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('Session controls')).toBeInTheDocument();
		await expect(canvas.getByText(/1\/1 steerers/)).toBeInTheDocument();
		await expect(canvas.getByText(/8 analyzers/)).toBeInTheDocument();

		const panelToggle = canvas.getByRole('button', { name: /session controls/i });
		await expect(panelToggle).toHaveAttribute('aria-expanded', 'false');
		await expect(canvas.queryByText('Steering activity')).not.toBeInTheDocument();
		await userEvent.click(panelToggle);
		await expect(panelToggle).toHaveAttribute('aria-expanded', 'true');

		await waitFor(() => expect(canvas.getByText('Steering activity')).toBeInTheDocument());
		await waitFor(() => expect(canvas.getByText('1 enrolled')).toBeInTheDocument());
		await waitFor(() =>
			expect(
				canvas.getByRole('button', { name: /disenrol from verifiersteerer/i })
			).toBeInTheDocument()
		);

		await waitFor(() => expect(canvas.getByText('Enrolled')).toBeInTheDocument());
		await expect(canvas.getByText('enrolled — not fired yet')).toBeInTheDocument();
		await expect(
			canvas.queryByText('No steerers enrolled or active in this session.')
		).not.toBeInTheDocument();

		await expect(
			canvas.queryByText('No sub-agents were dispatched in this session.')
		).not.toBeInTheDocument();
		await expect(canvas.getByText('rust-developer')).toBeInTheDocument();
		await expect(canvas.getByText('2 dispatches')).toBeInTheDocument();
		await expect(canvas.getByText('general-purpose')).toBeInTheDocument();
		await expect(canvas.getByText('1 dispatch')).toBeInTheDocument();
		await expect(canvas.queryByText('unknown')).not.toBeInTheDocument();

		const enrolToggle = canvas.getByRole('button', { name: /collapse verifiersteerer/i });
		await expect(enrolToggle).toHaveAttribute('aria-expanded', 'true');
		const disenrolBtn = canvas.getByRole('button', { name: /disenrol from verifiersteerer/i });
		await expect(disenrolBtn).toBeInTheDocument();
		await userEvent.click(enrolToggle);
		await expect(enrolToggle).toHaveAttribute('aria-expanded', 'false');
		await waitFor(() =>
			expect(
				canvas.queryByRole('button', { name: /disenrol from verifiersteerer/i })
			).not.toBeInTheDocument()
		);
		await userEvent.click(enrolToggle);
		await expect(enrolToggle).toHaveAttribute('aria-expanded', 'true');

		await userEvent.click(panelToggle);
		await expect(panelToggle).toHaveAttribute('aria-expanded', 'false');
		await waitFor(() => expect(canvas.queryByText('Steering activity')).not.toBeInTheDocument());
		await userEvent.click(panelToggle);
		await expect(panelToggle).toHaveAttribute('aria-expanded', 'true');
		await expect(canvas.getByText('Steering activity')).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<CollapseStoreProvider>
			{#snippet children()}
				<div style="max-width: 760px; background: var(--surface-bg); padding: 16px;">
					<SessionControlsPanel
						sessionId={STORY_SESSION}
						now={STATIC_NOW}
						steerersEnrolled={1}
						steerersTotal={1}
						gradersActive={0}
						rulesActive={0}
						analyzersActive={8}
						agents={AGENTS}
						onJumpTo={() => {}}
					/>
				</div>
			{/snippet}
		</CollapseStoreProvider>
	{/snippet}
</Story>
