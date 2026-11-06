<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import * as RadioGroup from './index';

	const { Story } = defineMeta({
		title: 'UI/RadioGroup',
		component: RadioGroup.Root,
		tags: ['autodocs']
	});

	let picked = $state('own');
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const group = canvas.getByRole('radiogroup', { name: 'Belongs to' });
		const own = within(group).getByRole('radio', { name: /Its own project/ });
		await expect(own).toBeChecked();
		own.focus();
		await userEvent.keyboard('{ArrowDown}');
		const path = within(group).getByRole('radio', { name: /worktrees/ });
		await expect(path).toBeChecked();
		await expect(within(group).getByRole('radio', { name: /Disabled choice/ })).toBeDisabled();
		await expect(canvas.getByTestId('picked')).toHaveTextContent('worktree');
	}}
>
	{#snippet template()}
		<div class="w-96 bg-[var(--surface-bg)] p-4">
			<span id="rg-label" class="machine mb-2 block text-xs">Belongs to</span>
			<div
				class="edge [--edge:var(--edge-sm)] [--edge-pad:0px] [--edge-line:var(--border-default)] [--edge-surface:var(--surface-bg)]"
			>
				<RadioGroup.Root
					name="story-belongs"
					bind:value={picked}
					aria-labelledby="rg-label"
					class="mech-group"
				>
					<RadioGroup.Item value="own" label="Its own project" description="Leave it as it is" />
					<RadioGroup.Item
						value="worktree"
						label="temet-proxy-wt-wp03"
						description="/mnt/data/projects/ankh-morpork/ai/svc-coord-eval-f0/worktrees/very-long-branch-name"
					/>
					<RadioGroup.Item value="off" label="Disabled choice" disabled />
				</RadioGroup.Root>
			</div>
			<p data-testid="picked" class="mt-2 text-xs">{picked}</p>
		</div>
	{/snippet}
</Story>
