<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, waitFor, expect } from 'storybook/test';
	import MobileNavigationHarness from './__harness__/MobileNavigationHarness.svelte';

	const { Story } = defineMeta({
		title: 'Shell/MobileNavigation',
		component: MobileNavigationHarness,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	parameters={{
		sveltekit_experimental: {
			state: {
				page: {
					url: new URL('http://localhost/sessions/example'),
					params: { sessionId: 'example' }
				}
			}
		}
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const trigger = canvas.getByRole('button', { name: 'Open navigation' });
		await userEvent.click(trigger);
		const body = within(document.body);
		const dialog = await body.findByRole('dialog', { name: 'Navigate' });
		const nav = within(dialog).getByRole('navigation', { name: 'Destinations' });
		await expect(within(nav).getByRole('link', { name: 'Sessions' })).toHaveAttribute(
			'aria-current',
			'page'
		);
		await expect(
			within(nav).queryByRole('button', { name: 'Collapse navigation column' })
		).not.toBeInTheDocument();
		await userEvent.keyboard('{Escape}');
		await waitFor(() => expect(trigger).toHaveFocus());
		await waitFor(() =>
			expect(body.queryByRole('dialog', { name: 'Navigate' })).not.toBeInTheDocument()
		);
		await waitFor(() => expect(getComputedStyle(trigger).pointerEvents).not.toBe('none'));
		await userEvent.click(trigger);
		const reopened = await body.findByRole('dialog', { name: 'Navigate' });
		await userEvent.click(within(reopened).getByRole('link', { name: 'Sessions' }));
		await waitFor(() =>
			expect(body.queryByRole('dialog', { name: 'Navigate' })).not.toBeInTheDocument()
		);
	}}
/>
