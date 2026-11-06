<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import { Bot } from '@lucide/svelte';
	import FlowPromptModal from './FlowPromptModal.svelte';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/FlowPromptModal',
		component: FlowPromptModal,
		parameters: { layout: 'padded' }
	});
</script>

<script lang="ts">
	const PROMPT = [
		'## Method',
		'1. **Interactive pair.** Find the first-turn request.',
		'- Read `report.md` in full.',
		'Plain paragraph line.'
	].join('\n');

	let open = $state(false);
	const onOpenDispatch = fn();
</script>

<Story
	name="Modal"
	play={async ({ canvasElement }) => {
		const trigger = canvasElement.querySelector<HTMLButtonElement>('[data-testid="open-modal"]')!;
		await userEvent.click(trigger);

		const body = within(document.body);
		const dialog = await waitFor(() => body.getByRole('dialog'));
		await waitFor(() => expect(dialog).toBeVisible());
		await expect(within(dialog).getByText('Method')).toBeInTheDocument();
		await expect(within(dialog).getByText(/Interactive pair\./)).toBeInTheDocument();
		await expect(within(dialog).getByText('report.md')).toBeInTheDocument();

		const rawBtn = within(dialog).getByRole('button', { name: 'Raw' });
		await userEvent.click(rawBtn);
		await waitFor(() => expect(within(dialog).getByText(/## Method/)).toBeInTheDocument());
		await userEvent.click(rawBtn);
		await waitFor(() => expect(within(dialog).queryByText(/## Method/)).not.toBeInTheDocument());

		const openDispatchBtn = within(dialog).getByRole('button', { name: /^Open dispatch/ });
		await userEvent.click(openDispatchBtn);
		await expect(onOpenDispatch).toHaveBeenCalled();

		await userEvent.keyboard('{Escape}');
		await waitFor(() => expect(body.queryByRole('dialog')).not.toBeInTheDocument());
	}}
>
	{#snippet template()}
		<div data-design="funk" style="padding: 16px;">
			<button
				type="button"
				data-testid="open-modal"
				onclick={() => (open = true)}
				style="padding: 8px 12px; font-family: var(--font-mono); background: var(--surface-card); color: var(--text-1); box-shadow: inset 0 0 0 1px var(--border-default); border: 0; cursor: pointer;"
			>
				Open prompt modal
			</button>
			<FlowPromptModal
				bind:open
				identityFamily="agent"
				icon={Bot}
				titleLabel="Prompt"
				name="general-purpose · 1"
				heading="Opus 5 vs 5.5 prompt comparison"
				note="184 chars · from dispatch #421902"
				text={PROMPT}
				footerNote="Sent by the main session when it dispatched this agent"
				openLabel="dispatch #421902"
				{onOpenDispatch}
			/>
		</div>
	{/snippet}
</Story>
