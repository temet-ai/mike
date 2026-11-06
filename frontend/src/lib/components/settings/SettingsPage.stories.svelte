<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import SettingsPage from './SettingsPage.svelte';
	import SettingsSection from './SettingsSection.svelte';

	const onRetry = fn();

	const { Story } = defineMeta({
		title: 'Settings/SettingsPage',
		component: SettingsPage,
		argTypes: {
			lede: { control: false },
			children: { control: false }
		}
	});
</script>

<Story
	name="Default"
	args={{
		title: 'Workers',
		current: '/settings/workers',
		loadErrors: [null, 'Settings: the request failed with 503'],
		onRetry
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Workers' })).toBeVisible();
		const navigation = canvas.getByRole('navigation', { name: 'Settings sections' });
		await expect(within(navigation).getByRole('link', { name: 'Workers' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		const alert = canvas.getByRole('alert');
		await expect(alert).toHaveTextContent('Settings: the request failed with 503');
		await userEvent.click(within(alert).getByRole('button', { name: 'Retry' }));
		await expect(onRetry).toHaveBeenCalledOnce();
		await expect(canvas.getByRole('heading', { level: 2, name: 'Lesson worker' })).toBeVisible();
	}}
>
	{#snippet template(args)}
		<div style="height: 640px;">
			<SettingsPage
				title={args.title}
				current={args.current}
				loadErrors={args.loadErrors}
				onRetry={args.onRetry}
			>
				{#snippet lede()}
					Background workers that run across all sessions.
				{/snippet}
				<SettingsSection title="Lesson worker" id="lesson-worker">
					<p>Rows for this group go here.</p>
				</SettingsSection>
			</SettingsPage>
		</div>
	{/snippet}
</Story>
