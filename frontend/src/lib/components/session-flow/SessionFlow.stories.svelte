<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within, waitFor } from 'storybook/test';
	import SessionFlowHarness from './__harness__/SessionFlowHarness.svelte';
	import CollapseStoreProvider from '$lib/components/__harness__/CollapseStoreProvider.svelte';
	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow',
		component: SessionFlowHarness,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="Representative"
	loaders={[
		async () => {
			localStorage.clear();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const titleOrder = ['Session flow', 'Timeline', 'Selected', 'Filter'];
		const titleEls = titleOrder.map((title) => canvas.getByText(title));
		for (let i = 1; i < titleEls.length; i += 1) {
			const relation = titleEls[i - 1]!.compareDocumentPosition(titleEls[i]!);
			await expect(Boolean(relation & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(true);
		}
		// Board = the card: no "Board" header row, and the whole accessible-list
		// "Details" section (actors/coverage/browser observations) is gone.
		await expect(canvas.queryByText('Board')).not.toBeInTheDocument();
		await expect(canvas.queryByText('Details')).not.toBeInTheDocument();

		const boardCard = canvasElement.querySelector('.flow-board-card')!;
		const before = boardCard.getBoundingClientRect();
		const loadingStatus = within(canvas.getByRole('status')).getByText('Loading flow data…');
		await expect(loadingStatus).not.toBeVisible();
		await userEvent.click(canvas.getByRole('button', { name: 'Toggle loading state' }));
		await expect(loadingStatus).toBeVisible();
		await expect(boardCard.getBoundingClientRect().top).toBe(before.top);
		await expect(boardCard.getBoundingClientRect().left).toBe(before.left);
		await userEvent.click(canvas.getByRole('button', { name: 'Toggle loading state' }));
		await expect(loadingStatus).not.toBeVisible();
		await expect(boardCard.getBoundingClientRect().top).toBe(before.top);

		await expect(canvasElement.querySelectorAll('.node-chip--muted')).toHaveLength(0);

		await userEvent.click(canvas.getByRole('button', { name: /^Seek to Peak concurrent agents/ }));

		await expect(canvas.getAllByRole('button', { name: /· busy$/ }).length).toBeGreaterThan(0);
		await expect(canvas.getByText('REPLAY')).toBeVisible();
		await expect(canvas.queryByText('LIVE')).not.toBeInTheDocument();

		// Board sits left of the Selected pane on desktop widths.
		const selectedPane = canvasElement.querySelector('.flow-selected-pane')!;
		await expect(
			boardCard.getBoundingClientRect().right <= selectedPane.getBoundingClientRect().left
		).toBe(true);

		await expect(canvas.getByText('Pick anything on the board')).toBeVisible();
		await userEvent.click(canvas.getByRole('button', { name: 'Inspect Agent 1 · busy' }));
		await expect(canvas.queryByText('Pick anything on the board')).not.toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: 'Clear' })).toBeVisible();

		await userEvent.click(canvas.getByRole('button', { name: 'Clear' }));
		await expect(canvas.getByText('Pick anything on the board')).toBeVisible();

		// The board zoom control is present and reachable inside the board card.
		const zoomGroup = boardCard.querySelector('[role="group"]');
		await expect(zoomGroup).toHaveAttribute('aria-label', expect.stringContaining('zoom'));

		const view = canvasElement.ownerDocument.defaultView!;
		view.scrollTo({ top: 0, behavior: 'instant' });
		const beforeWheel = view.scrollY;
		const frame = boardCard.querySelector<HTMLElement>('.fbv-frame')!;
		const diagonal = new WheelEvent('wheel', {
			deltaX: 3,
			deltaY: 300,
			bubbles: true,
			cancelable: true
		});
		frame.dispatchEvent(diagonal);
		await expect(diagonal.defaultPrevented).toBe(true);
		await waitFor(() => expect(view.scrollY).toBeGreaterThan(beforeWheel));
		const horizontalDominant = new WheelEvent('wheel', {
			deltaX: 300,
			deltaY: 3,
			bubbles: true,
			cancelable: true
		});
		frame.dispatchEvent(horizontalDominant);
		await expect(horizontalDominant.defaultPrevented).toBe(false);
		const ctrlZoom = new WheelEvent('wheel', {
			deltaX: 3,
			deltaY: -200,
			ctrlKey: true,
			bubbles: true,
			cancelable: true
		});
		frame.dispatchEvent(ctrlZoom);
		await expect(ctrlZoom.defaultPrevented).toBe(true);

		await userEvent.click(canvas.getByRole('button', { name: 'Follow live' }));
		await expect(canvas.getByText('LIVE')).toBeInTheDocument();
	}}
>
	{#snippet template(args)}
		<CollapseStoreProvider>
			<SessionFlowHarness {...args} />
		</CollapseStoreProvider>
	{/snippet}
</Story>
