<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import FlowBoardViewport from './FlowBoardViewport.svelte';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/FlowBoardViewport',
		component: FlowBoardViewport,
		parameters: { layout: 'padded' }
	});
</script>

<Story
	name="Board"
	play={async ({ canvasElement }) => {
		const fitBtn = await waitFor(() => {
			const el = canvasElement.querySelector<HTMLButtonElement>('[aria-label="Fit board to view"]');
			if (!el) throw new Error('fit button not mounted yet');
			return el;
		});
		await expect(fitBtn).toHaveTextContent('Fit');
		await expect(fitBtn).toHaveAttribute('aria-pressed', 'true');

		const inner = canvasElement.querySelector<HTMLDivElement>('.fbv-inner')!;
		const scaleOf = (el: HTMLElement): number => {
			const m = getComputedStyle(el).transform.match(/matrix\(([^,]+),/);
			return m ? Number(m[1]) : 1;
		};
		const measure = canvasElement.querySelector<HTMLDivElement>('.fbv-measure')!;
		const wideFrame = canvasElement.querySelector<HTMLDivElement>('.fbv-frame')!;

		let scale = 0;
		let naturalContentH = 0;
		let frameRect: DOMRect = wideFrame.getBoundingClientRect();
		await waitFor(() => {
			scale = scaleOf(inner);
			naturalContentH = measure.offsetHeight;
			frameRect = wideFrame.getBoundingClientRect();
			expect(scale).toBeGreaterThan(0);
			expect(frameRect.height).toBeCloseTo(naturalContentH * scale, 0);
		});
		await expect(scale).toBeLessThanOrEqual(1);

		const innerRect = inner.getBoundingClientRect();
		await expect(innerRect.bottom).toBeLessThanOrEqual(frameRect.bottom + 5);
		await expect(innerRect.right).toBeLessThanOrEqual(frameRect.right + 1);
		await expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
			document.documentElement.clientWidth + 1
		);

		const zoomInBtn = canvasElement.querySelector<HTMLButtonElement>('[aria-label="Zoom in"]')!;
		await userEvent.click(zoomInBtn);
		await waitFor(() => expect(fitBtn).not.toHaveTextContent('Fit'));
		await expect(fitBtn).toHaveAttribute('aria-pressed', 'false');

		const zoomOutBtn = canvasElement.querySelector<HTMLButtonElement>('[aria-label="Zoom out"]')!;
		await userEvent.click(zoomOutBtn);
		await userEvent.click(zoomOutBtn);

		await userEvent.click(fitBtn);
		await waitFor(() => expect(fitBtn).toHaveTextContent('Fit'));

		const frame = canvasElement.querySelector<HTMLDivElement>('.fbv-frame')!;
		const ctrlWheel = new WheelEvent('wheel', {
			deltaY: -200,
			ctrlKey: true,
			bubbles: true,
			cancelable: true,
			clientX: 100,
			clientY: 100
		});
		frame.dispatchEvent(ctrlWheel);
		await expect(ctrlWheel.defaultPrevented).toBe(true);
		await waitFor(() => expect(fitBtn).not.toHaveTextContent('Fit'));

		const group = canvasElement.querySelector('[role="group"]');
		await expect(group).toHaveAttribute('aria-label', expect.stringContaining('Ctrl + scroll'));

		await userEvent.click(fitBtn);
		await waitFor(() => expect(fitBtn).toHaveTextContent('Fit'));
	}}
>
	{#snippet template()}
		<div style="max-width: 1800px;" data-design="funk">
			<FlowBoardViewport>
				<div
					style="position: relative; width: 1372px; height: 900px; background: repeating-linear-gradient(45deg, var(--surface-2) 0 12px, var(--surface-card) 12px 24px);"
				>
					{#each Array.from({ length: 8 }, (_, i) => i) as i (i)}
						<div
							style="position: absolute; left: {40 + i * 160}px; top: {40 +
								(i % 3) *
									140}px; width: 140px; height: 40px; background: var(--surface-card); box-shadow: inset 0 0 0 1px var(--border-default); display: flex; align-items: center; justify-content: center; font-family: var(--font-mono); font-size: 11px; color: var(--text-1);"
						>
							Node {i + 1}
						</div>
					{/each}
					<div
						style="position: absolute; left: 40px; top: 820px; width: 300px; height: 40px; background: var(--surface-card); box-shadow: inset 0 0 0 1px var(--border-default); display: flex; align-items: center; justify-content: center; font-family: var(--font-mono); font-size: 11px; color: var(--text-1);"
					>
						Tools row (Shell / Files / Mike MCP)
					</div>
				</div>
			</FlowBoardViewport>
		</div>
	{/snippet}
</Story>
