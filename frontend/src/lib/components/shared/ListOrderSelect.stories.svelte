<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import AcceptChance from './AcceptChance.svelte';
	import ListOrderSelect from './ListOrderSelect.svelte';

	const onChange = fn();

	const { Story } = defineMeta({
		title: 'Shared/ListOrderSelect',
		component: ListOrderSelect,
		tags: ['autodocs'],
		args: { order: 'default', onChange }
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const select = canvas.getByLabelText('Order') as HTMLSelectElement;
		await expect(select.value).toBe('default');
		await userEvent.selectOptions(select, 'suggested');
		await expect(onChange).toHaveBeenCalledWith('suggested');

		const slots = [...canvasElement.querySelectorAll<HTMLElement>('[data-slot="accept-chance"]')];
		await expect(slots.map((s) => s.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
			'accept 62%',
			'accept —'
		]);
		await expect(new Set(slots.map((s) => Math.round(s.getBoundingClientRect().width))).size).toBe(
			1
		);
		await expect(canvasElement.querySelectorAll('[data-slot="accept-chance"]')).toHaveLength(2);
	}}
>
	{#snippet template(args)}
		<div style="display: flex; flex-direction: column; gap: var(--space-3); width: 24rem;">
			<ListOrderSelect {...args} />
			<AcceptChance chance={0.624} />
			<AcceptChance chance={null} />
			<AcceptChance chance={0.5} show={false} />
		</div>
	{/snippet}
</Story>
