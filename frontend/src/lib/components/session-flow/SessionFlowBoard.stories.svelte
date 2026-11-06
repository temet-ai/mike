<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import SessionFlowBoardHarness from './__harness__/SessionFlowBoardHarness.svelte';

	const { Story } = defineMeta({
		title: 'Sessions/SessionFlow/SessionFlowBoard',
		component: SessionFlowBoardHarness,
		tags: ['autodocs']
	});
</script>

<Story
	name="FleetAtPeak"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const busyAgentChips = canvasElement.querySelectorAll(
			'[data-chip-kind="agent"] [aria-label$="· busy"]'
		);
		await expect(busyAgentChips).toHaveLength(21);
		await expect(canvas.getByText('Shell')).toBeVisible();
		await expect(canvas.getByText('Files')).toBeVisible();
		await expect(canvas.getByText('Browser')).toBeVisible();
		await expect(canvas.getByText('Mike MCP')).toBeVisible();
		await expect(canvas.getByText('MIKE · PROXY · THE BOUNDARY')).toBeVisible();
		await expect(canvas.getByText('OUTSIDE THE BOUNDARY')).toBeVisible();
		await expect(canvasElement.querySelectorAll('.node-chip--muted')).toHaveLength(0);

		const a1Button = canvasElement.querySelector('[data-chip-key="w0"] button')!;
		await userEvent.click(a1Button);
		await expect(a1Button).toHaveAttribute('aria-pressed', 'true');
		const isMuted = (key: string): boolean =>
			canvasElement
				.querySelector(`[data-chip-key="${key}"] .node-chip`)!
				.classList.contains('node-chip--muted');
		await expect(isMuted('w0')).toBe(false);
		await expect(isMuted('coord')).toBe(false);
		await expect(isMuted('model')).toBe(false);
		await expect(isMuted('shell')).toBe(false);
		await expect(isMuted('w1')).toBe(true);

		await userEvent.click(a1Button);
		await expect(a1Button).toHaveAttribute('aria-pressed', 'false');
		await expect(canvasElement.querySelectorAll('.node-chip--muted')).toHaveLength(0);

		const selectionText = () => canvas.getByTestId('harness-selection').textContent ?? '';
		const traceHit = canvas.getByRole('button', {
			name: /Select toolcall route: tool → w0 → shell/
		});
		await userEvent.click(traceHit);
		await expect(selectionText()).toContain('"kind":"link"');
		await expect(selectionText()).toContain('"routeKey":"tool:w0:shell"');
		await expect(selectionText()).toContain('"label":"Shell"');
		await expect(traceHit).toHaveAttribute('aria-pressed', 'true');
		await userEvent.click(traceHit);
		await expect(selectionText()).toBe('null');

		const topPathsBefore = canvasElement.querySelectorAll(
			'.trace-layer__lit--top path:not(.trace-packet)'
		).length;
		await expect(topPathsBefore).toBeGreaterThan(0);
		await userEvent.click(canvas.getByTestId('harness-drop-toolcall'));
		const topPathsAfter = canvasElement.querySelectorAll(
			'.trace-layer__lit--top path:not(.trace-packet)'
		).length;
		await expect(topPathsAfter).toBeLessThan(topPathsBefore);
		await expect(
			[
				...canvasElement.querySelectorAll('.trace-layer__lit--top > path:not(.trace-packet)')
			].filter((p) => p.getAttribute('stroke-dasharray') === '8 3')
		).toHaveLength(0);

		const collapseControl = canvas.getByRole('button', { name: /collapse agents/i });
		await userEvent.click(collapseControl);
		await expect(canvasElement.querySelectorAll('[data-chip-key^="w"]')).toHaveLength(0);
		await expect(canvasElement.querySelector('[data-chip-key="agents"]')).toBeTruthy();
		await expect(canvas.getByRole('button', { name: /expand agents/i })).toBeInTheDocument();
	}}
/>
