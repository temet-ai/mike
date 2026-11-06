<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, waitFor, expect, fn } from 'storybook/test';
	import TopBarHarness from './__harness__/TopBarHarness.svelte';
	import { shellState } from '$lib/stores/shellState.svelte';
	import { themeStore } from '$lib/stores/theme.svelte';
	import { THEMES } from '$lib/themes';
	import { buildStatsResponse } from '$lib/test-support';

	const { Story } = defineMeta({
		title: 'Shell/TopBar',
		component: TopBarHarness,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="Default"
	args={{ stats: buildStatsResponse({ request_count: 42, uptime_seconds: 3600 }), onRefresh: fn() }}
	parameters={{
		sveltekit_experimental: { state: { page: { url: new URL('http://localhost/'), params: {} } } }
	}}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: 'pause' }));
		await expect(shellState.paused).toBe(true);
		await userEvent.click(canvas.getByRole('button', { name: 'resume' }));
		await expect(shellState.paused).toBe(false);
		await userEvent.click(canvas.getByRole('button', { name: 'refresh' }));
		await expect(args.onRefresh).toHaveBeenCalledOnce();
		await userEvent.click(canvas.getByRole('button', { name: 'Search pages and commands' }));
		await expect(shellState.commandPaletteOpen).toBe(true);
		shellState.commandPaletteOpen = false;
		await expect(
			canvas.queryByRole('button', { name: /Open command palette/ })
		).not.toBeInTheDocument();

		const trigger = canvas.getByRole('button', { name: 'Choose colour theme' });
		await userEvent.click(trigger);
		const body = within(document.body);
		const options = await body.findByRole('group', { name: 'Colour theme' });
		await expect(within(options).getAllByRole('radio')).toHaveLength(THEMES.length);
		const forge = within(options).getByRole('radio', { name: 'Forge' });
		await userEvent.click(forge);
		await userEvent.keyboard('{ArrowRight}');
		await waitFor(() => expect(themeStore.current).toBe('glacier'));
		await expect(within(options).getByRole('radio', { name: 'Glacier' })).toHaveFocus();
		await userEvent.keyboard('{Escape}');
		await waitFor(() => expect(trigger).toHaveFocus());
		themeStore.set('forge');
	}}
/>
