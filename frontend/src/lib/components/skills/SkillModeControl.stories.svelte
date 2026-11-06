<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
	import SkillModeControl from './SkillModeControl.svelte';
	import type { SkillMode } from '$lib/api/skills';

	let calls = 0;
	const onChange = fn(async (mode: SkillMode) => {
		calls += 1;
		if (calls > 1) throw new Error(`Could not save ${mode}: the proxy did not answer`);
	});

	const { Story } = defineMeta({
		title: 'Skills/SkillModeControl',
		component: SkillModeControl,
		tags: ['autodocs']
	});
</script>

<Story
	name="Saves and falls back on failure"
	args={{ mode: 'ask', projectName: 'storefront', onChange }}
	play={async ({ canvasElement }) => {
		calls = 0;
		onChange.mockClear();
		const canvas = within(canvasElement);
		const ask = canvas.getByRole('radio', { name: /Ask me first/ });
		const auto = canvas.getByRole('radio', { name: /Ship when it passes review/ });
		await expect(ask).toBeChecked();
		await userEvent.click(auto);
		await waitFor(() => expect(onChange).toHaveBeenCalledWith('auto'));
		await expect(auto).toBeChecked();
		await userEvent.click(ask);
		await waitFor(() => expect(canvas.getByRole('alert')).toHaveTextContent(/Could not save ask/));
		await expect(auto).toBeChecked();
	}}
>
	{#snippet template(args)}
		<div class="w-[32rem] bg-[var(--surface-bg)] p-4">
			<SkillModeControl {...args} />
		</div>
	{/snippet}
</Story>
