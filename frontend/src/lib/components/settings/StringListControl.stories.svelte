<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import StringListControl from './StringListControl.svelte';

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/StringListControl',
		component: StringListControl,
		tags: ['autodocs']
	});
</script>

<script lang="ts">
	let items = $state<string[]>([
		'I have successfully',
		'The implementation is complete',
		'Everything is working correctly',
		'All tests pass'
	]);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const values = [...canvasElement.querySelectorAll('input')].map(
			(i) => (i as HTMLInputElement).value
		);
		await expect(values.length).toBeGreaterThanOrEqual(4);
		await expect(values).toContain('I have successfully');
		await expect(values).toContain('All tests pass');
	}}
>
	{#snippet template()}
		<div style="max-width:560px;">
			<StringListControl {items} maxLen={256} onSave={(next) => (items = next)} />
		</div>
	{/snippet}
</Story>
