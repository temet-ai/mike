<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import EmbeddingPage from './+page.svelte';
	import type { SettingMeta } from '$lib/api/settings';
	import type { EmbeddingPageData } from './+page';

	function meta(
		o: Partial<SettingMeta> & Pick<SettingMeta, 'key' | 'kind' | 'value' | 'default'>
	): SettingMeta {
		return {
			group: 'Embedding',
			scope: 'session',
			applies: 'new_sessions',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const SETTINGS: SettingMeta[] = [
		meta({
			key: 'embedding.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Enable embedding',
			description: 'Generates semantic embeddings of session turns.'
		}),
		meta({
			key: 'embedding.model',
			kind: 'string',
			default: 'nomic-embed-text-v1.5',
			value: 'nomic-embed-text-v1.5',
			label: 'Model',
			description: 'Embedding model id.',
			max_len: 256
		}),
		meta({
			key: 'embedding.url',
			kind: 'string',
			default: 'http://localhost:11434/v1',
			value: 'http://localhost:11434/v1',
			label: 'Embedding API URL',
			description: 'Base URL for the embedding service (Ollama or compatible).',
			max_len: 2048
		}),
		meta({
			key: 'embedding.chunk_max_chars',
			kind: 'int',
			default: 3000,
			value: 3000,
			label: 'Chunk max chars',
			description: 'Maximum characters per text chunk before splitting.',
			min_i64: 100,
			max_i64: 32000
		})
	];

	function buildData(): EmbeddingPageData {
		return { settings: SETTINGS, settingsLoadError: null };
	}

	let savedReason: string | null = null;
	let enabled = true;

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url =
			typeof input === 'string'
				? input
				: input instanceof URL
					? input.href
					: (input as Request).url;
		const json = (body: unknown) =>
			Promise.resolve(
				new Response(JSON.stringify(body), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				})
			);
		if (url.includes('/api/settings/features/')) {
			const key = url.split('/').pop() ?? 'x';
			let enabled = true;
			if (init && typeof init.body === 'string') {
				try {
					enabled = JSON.parse(init.body).enabled ?? true;
				} catch {}
			}
			return json({ key, enabled, scope: 'session', applies: 'new_sessions' });
		}
		if (url.includes('/api/settings/') && !url.includes('/features')) {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? 'x');
			let value: unknown;
			if (init && typeof init.body === 'string') {
				try {
					value = JSON.parse(init.body).value;
				} catch {}
			}
			if (key === 'embedding.enabled' && value === false) {
				savedReason = new Headers(init?.headers).get('X-Change-Reason');
				enabled = false;
			}
			return json(
				meta({ key, kind: 'string', default: '', value: (value ?? '') as SettingMeta['value'] })
			);
		}
		if (url.includes('/api/settings/features')) return json({ features: [] });
		if (url.includes('/api/settings'))
			return json({
				settings: SETTINGS.map((s) =>
					s.key === 'embedding.enabled' ? { ...s, value: enabled } : s
				),
				steerer_openrouter_configured_via_env: false
			});
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Embedding',
		component: EmbeddingPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			savedReason = null;
			enabled = true;
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvasElement.textContent).toContain('Embedding');
		await userEvent.click(canvas.getByRole('switch'));
		await expect(savedReason).toBeNull();
		await expect(canvas.getByRole('button', { name: 'Disable embedding' })).toBeDisabled();
		await userEvent.click(canvas.getByRole('button', { name: 'Cancel' }));
		await expect(canvas.queryByLabelText('Reason for disabling embedding')).not.toBeInTheDocument();
		await expect(savedReason).toBeNull();
		await userEvent.click(canvas.getByRole('switch'));
		await userEvent.type(
			canvas.getByLabelText('Reason for disabling embedding'),
			'Maintenance — replacing the local model'
		);
		await userEvent.click(canvas.getByRole('button', { name: 'Disable embedding' }));
		await expect(savedReason).toBe(encodeURIComponent('Maintenance — replacing the local model'));
		await expect(
			canvas.getByText('Embedding disabled. Your reason was recorded in settings history.')
		).toBeInTheDocument();
		await expect(canvasElement.querySelectorAll('[role="switch"]').length).toBeGreaterThan(0);
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<EmbeddingPage {...args} />
		</div>
	{/snippet}
</Story>
