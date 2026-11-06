<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import SetupPage from './+page.svelte';
	import type { SetupPageData } from './+page';
	import {
		buildFeatures,
		buildProviders,
		buildSetupState,
		buildSettings,
		meta
	} from '$lib/components/settings/setup/setupFixtures';
	import type { SettingMeta } from '$lib/api/settings';

	const FEATURES = buildFeatures(['steerer', 'judge_worker']);
	const SETTINGS = buildSettings();
	const sent: { method: string; url: string; body: string }[] = [];

	function buildData(): SetupPageData {
		return {
			state: buildSetupState(),
			stateError: null,
			features: FEATURES,
			settings: SETTINGS,
			settingsError: null,
			providers: buildProviders(),
			providersError: null
		};
	}

	function json(body: unknown): Promise<Response> {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url =
			typeof input === 'string' ? input : input instanceof URL ? input.pathname : input.url;
		const method = init?.method ?? 'GET';
		if (method !== 'GET') {
			sent.push({ method, url, body: typeof init?.body === 'string' ? init.body : '' });
		}
		if (url.startsWith('/api/settings/features/')) {
			const key = decodeURIComponent(url.split('/').pop() ?? '');
			return json({ key, enabled: true, scope: 'session', applies: 'new_sessions' });
		}
		if (url.startsWith('/api/settings/') && method === 'PATCH') {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? '');
			const value = JSON.parse(String(init?.body ?? '{}')).value;
			return json(meta({ key, value } as Partial<SettingMeta> & { key: string }));
		}
		if (url === '/api/settings/features') return json({ features: FEATURES });
		if (url === '/api/settings')
			return json({ settings: SETTINGS, steerer_openrouter_configured_via_env: false });
		if (url === '/api/setup/state') return json(buildSetupState());
		if (url === '/api/llm/providers') return json(buildProviders());
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Setup',
		component: SetupPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			sent.length = 0;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('heading', { level: 1, name: 'Setup' })).toBeVisible();
		for (const title of [
			'Checks',
			'Database',
			'Background service',
			'Claude Code',
			'Features',
			'Background models',
			'Search by meaning',
			'Terminal'
		]) {
			await expect(canvas.getByRole('heading', { level: 2, name: title })).toBeVisible();
		}
		await expect(canvas.getByText('mike setup service')).toBeVisible();
		await expect(canvas.getByText('mike claude-code trust off')).toBeVisible();
		await expect(canvas.getByText('mike claude-code route plain')).toBeVisible();

		await userEvent.click(canvas.getByRole('radio', { name: /^Recommended/ }));
		await userEvent.click(canvas.getByRole('button', { name: /^Apply, / }));
		await waitFor(() =>
			expect(sent.some((s) => s.url.endsWith('/reminders.inject_lessons'))).toBe(true)
		);
		const bodyOf = (suffix: string) => sent.find((s) => s.url.endsWith(suffix))?.body ?? '';
		await expect(bodyOf('/recall_injector')).toContain('"enabled":true');
		await expect(bodyOf('/steerer')).toContain('"enabled":false');

		const steerers = canvas.getByText('Steerers').closest('[data-slot="setup-row"]') as HTMLElement;
		await userEvent.selectOptions(within(steerers).getByLabelText('Provider'), 'codex');
		await waitFor(() => expect(sent.some((s) => s.url.endsWith('/steerer.model'))).toBe(true));
		await expect(sent.find((s) => s.url.endsWith('/steerer.model'))?.body).toContain(
			'codex/gpt-6-luna'
		);
	}}
>
	{#snippet template(args)}
		<div style="width:100%; height:100vh; background:var(--surface-bg);">
			<SetupPage {...args} />
		</div>
	{/snippet}
</Story>
