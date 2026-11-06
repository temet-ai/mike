<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import { Switch } from './index';

	const onCheckedChange = fn();

	const { Story } = defineMeta({
		title: 'UI/Switch',
		component: Switch,
		tags: ['autodocs']
	});

	let on = $state(true);
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const tail = canvas.getByRole('switch', { name: 'Tail' });
		await expect(tail).toHaveAttribute('aria-checked', 'true');
		const before = tail.getBoundingClientRect();
		await userEvent.click(tail);
		await expect(tail).toHaveAttribute('aria-checked', 'false');
		await expect(onCheckedChange).toHaveBeenLastCalledWith(false);
		const after = tail.getBoundingClientRect();
		await expect(after.width, 'a flip must not resize the switch').toBe(before.width);
		await expect(after.height).toBe(before.height);
		tail.focus();
		await userEvent.keyboard(' ');
		await expect(tail).toHaveAttribute('aria-checked', 'true');
		const locked = canvas.getByRole('switch', { name: 'Locked' });
		await expect(locked).toBeDisabled();
	}}
>
	{#snippet template()}
		<div class="flex items-center gap-6 bg-[var(--surface-bg)] p-6">
			<Switch bind:checked={on} aria-label="Tail" {onCheckedChange} />
			<Switch checked={false} aria-label="Snapshot" />
			<Switch checked disabled aria-label="Locked" />
		</div>
	{/snippet}
</Story>
