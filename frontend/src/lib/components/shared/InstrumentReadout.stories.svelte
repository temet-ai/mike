<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import InstrumentReadout from './InstrumentReadout.svelte';

	const { Story } = defineMeta({
		title: 'Shared/InstrumentReadout',
		component: InstrumentReadout,
		tags: ['autodocs']
	});
</script>

<Story
	name="Readout"
	args={{ label: 'requests / min', value: 42, accentToken: '--signal-active' }}
	play={async ({ canvas }) => {
		await expect(canvas.getByText('requests / min')).toBeVisible();
		const value = canvas.getByText('42');
		await expect(value).toBeVisible();
		await expect(value.className).toContain('instrument-readout__value');
	}}
/>
