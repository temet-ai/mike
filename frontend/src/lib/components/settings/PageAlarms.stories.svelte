<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor, within } from 'storybook/test';
	import PageAlarms from './PageAlarms.svelte';
	import {
		ALARMS,
		BUNDLES,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	function json(body: unknown) {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function fetchStub(input: RequestInfo | URL): Promise<Response> {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
		if (url.includes('/api/eval/alarms')) {
			return json({ total: ALARMS.length, limit: 100, offset: 0, alarms: ALARMS });
		}
		if (url.includes('/api/eval/bundles')) {
			return json({ bundles: BUNDLES });
		}
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Settings/PageAlarms',
		component: PageAlarms,
		tags: ['autodocs'],
		args: { home: 'steering' }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		await waitFor(() =>
			expect(c.getByRole('heading', { name: 'Needs attention · 4' })).toBeInTheDocument()
		);
		await expect(canvasElement.textContent).toContain(
			'Configuration 11 is in use without its accuracy measured against the accuracy bar. First seen 26 Sep 2026.'
		);
		await expect(canvasElement.textContent).toContain(
			'Configuration 10 is in use without its accuracy measured against the accuracy bar. First seen 26 Sep 2026.'
		);
		await expect(canvasElement.textContent).toContain(
			'Configuration 6 is in use without its accuracy measured against the accuracy bar. First seen 26 Sep 2026.'
		);
		await expect(canvasElement.textContent).toContain(
			'Configuration 4 is in use without its accuracy measured against the accuracy bar. First seen 26 Sep 2026.'
		);
		await expect(canvasElement.textContent).not.toContain('finished turns have not been judged');

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
