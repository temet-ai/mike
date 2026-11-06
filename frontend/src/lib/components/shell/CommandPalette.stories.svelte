<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import { __setRetryDelaysForTesting, __resetRetryDelaysForTesting } from '$lib/api';
	import { shellState } from '$lib/stores/shellState.svelte';
	import CommandPaletteHarness from '../__harness__/CommandPaletteHarness.svelte';

	const { Story } = defineMeta({
		title: 'Shell/CommandPalette',
		component: CommandPaletteHarness,
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
					params: { sessionId: 'example' },
					route: { id: '/sessions/[sessionId]' }
				}
			}
		}
	}}
	play={async () => {
		const body = within(document.body);
		const input = await body.findByRole('combobox', {
			name: 'Search pages, sessions and commands'
		});
		await waitFor(() => expect(input).toHaveFocus());
		const options = body.getAllByRole('option');
		const secondOption = options[1];
		if (!secondOption) throw new Error('Expected at least two command options');
		await expect(options[0]).toHaveAttribute('aria-selected', 'true');
		await userEvent.keyboard('{ArrowDown}');
		await expect(options[1]).toHaveAttribute('aria-selected', 'true');
		await expect(input).toHaveAttribute('aria-activedescendant', secondOption.id);

		const originalFetch = globalThis.fetch;
		__setRetryDelaysForTesting([0, 0, 0]);
		globalThis.fetch = async (request, init) => {
			const url = request instanceof Request ? request.url : String(request);
			if (url.includes('/api/sessions')) {
				return new Response(JSON.stringify({ error: 'session store unavailable' }), {
					status: 503,
					headers: { 'content-type': 'application/json' }
				});
			}
			return originalFetch(request, init);
		};
		try {
			await userEvent.type(input, 'unmatched-session');
			await expect(await body.findByRole('alert', {}, { timeout: 3000 })).toHaveTextContent(
				'Sessions search failed'
			);
			await userEvent.keyboard('{Home}');
			await expect((input as HTMLInputElement).selectionStart).toBe(0);
			await userEvent.clear(input);
			await waitFor(() => expect(body.queryByRole('alert')).not.toBeInTheDocument());
			await waitFor(() =>
				expect(body.getByRole('option', { name: /Pin current session/ })).not.toHaveAttribute(
					'aria-disabled',
					'true'
				)
			);
		} finally {
			globalThis.fetch = originalFetch;
			__resetRetryDelaysForTesting();
		}

		await userEvent.click(body.getByRole('button', { name: 'Close command palette' }));
		await waitFor(() => expect(body.queryByRole('dialog')).not.toBeInTheDocument());
		shellState.commandPaletteOpen = true;
		const reopenedInput = await body.findByRole('combobox');
		await userEvent.hover(body.getByRole('option', { name: /Open Help drawer/ }));
		reopenedInput.focus();
		await userEvent.keyboard('{Tab}');
		await expect(body.getByRole('button', { name: 'Close command palette' })).toHaveFocus();
		await userEvent.keyboard('{Enter}');
		await waitFor(() => expect(body.queryByRole('dialog')).not.toBeInTheDocument());
		await expect(shellState.helpDrawerOpen).toBe(false);
		shellState.commandPaletteOpen = true;
		await body.findByRole('combobox');
		await userEvent.keyboard('{Escape}');
		await waitFor(() => expect(body.queryByRole('dialog')).not.toBeInTheDocument());
		shellState.commandPaletteOpen = true;
		await body.findByRole('combobox');
	}}
/>
