<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import DecisionsCalibrationPanel from './DecisionsCalibrationPanel.svelte';
	import { calibrationFixture } from '$lib/decisions/__fixtures__/calibration';

	const onModeChange = fn();

	const { Story } = defineMeta({
		title: 'Settings/DecisionsCalibrationPanel',
		component: DecisionsCalibrationPanel,
		tags: ['autodocs'],
		argTypes: { calibration: { control: false } }
	});

	function box(el: Element | null): { top: number; height: number; width: number } {
		if (el === null) throw new Error('Expected an element to measure');
		const r = el.getBoundingClientRect();
		return { top: Math.round(r.top), height: Math.round(r.height), width: Math.round(r.width) };
	}
</script>

<script lang="ts">
	let layerOn = $state(true);
	const calibration = $derived(calibrationFixture(layerOn));
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={{ onModeChange }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const row = (point: string) =>
			canvasElement.querySelector<HTMLElement>(`[data-point="${point}"]`) as HTMLElement;

		await expect(canvas.getByText('Turn judge')).toBeVisible();
		await expect(canvas.getAllByRole('columnheader')).toHaveLength(9);
		await expect(row('session_judge').textContent).toContain('$0.0025');
		await expect(row('rule_check').textContent).toContain('not run');
		await expect(row('commit_message').textContent).toContain('off');
		await expect(row('fleet_stop_check').textContent).toContain('per fleet');
		await expect(row('turn_guard_precheck').textContent).toContain('7/9');

		const before = box(canvasElement.querySelector('.dcp-table'));
		const rowBefore = box(row('turn_judge'));
		const nextBefore = box(row('session_judge'));

		const select = canvas.getByLabelText('Turn judge mode') as HTMLSelectElement;
		await userEvent.selectOptions(select, 'shadow');
		await waitFor(() => expect(onModeChange).toHaveBeenCalledWith('turn_judge', 'shadow'));
		await expect(box(row('turn_judge'))).toEqual(rowBefore);
		await expect(box(row('session_judge')).top).toBe(nextBefore.top);
		await expect(box(canvasElement.querySelector('.dcp-table')).height).toBe(before.height);

		await userEvent.click(within(row('turn_judge')).getByRole('button', { name: 'Turn judge' }));
		const detail = canvasElement.querySelector('[data-point-detail="turn_judge"]');
		await expect(detail?.textContent).toContain('failed open is 5.0%');
		await expect(box(row('turn_judge'))).toEqual(rowBefore);

		await userEvent.click(canvas.getByRole('button', { name: /Judges/ }));
		await expect(canvasElement.querySelector('[data-point="turn_judge"]')).toBeNull();
		await userEvent.click(canvas.getByRole('button', { name: /Judges/ }));

		await userEvent.type(
			canvas.getByRole('searchbox', { name: 'Filter decision points' }),
			'lesson'
		);
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-point="turn_judge"]')).toBeNull()
		);
		await expect(canvas.getByText('Lesson supply floor')).toBeVisible();
		await userEvent.clear(canvas.getByRole('searchbox', { name: 'Filter decision points' }));

		layerOn = false;
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-slot="decisions-off"]')).not.toBeNull()
		);
		await expect(canvasElement.textContent).toContain('no OpenRouter key');
		await expect(canvasElement.querySelector('[data-point="turn_judge"]')).not.toBeNull();
		layerOn = true;
	}}
>
	{#snippet template(args)}
		<div style="width: 70rem; background: var(--surface-bg); padding: var(--space-4);">
			<DecisionsCalibrationPanel {...args} {calibration} now={Date.parse('2026-11-14T08:00:00Z')} />
		</div>
	{/snippet}
</Story>
