<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import FlaggingPage from './+page.svelte';
	import type { FeatureDto, SettingMeta } from '$lib/api/settings';
	import type { FlaggingPageData } from './+page';

	const FEATURES: FeatureDto[] = [
		{ key: 'weasel_intercept', enabled: true, scope: 'session', applies: 'new_sessions' }
	];

	function meta(
		o: Partial<SettingMeta> &
			Pick<SettingMeta, 'key' | 'kind' | 'value' | 'default'> & { group: string }
	): SettingMeta {
		return {
			scope: 'session',
			applies: 'new_sessions',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const SETTINGS: SettingMeta[] = [
		meta({
			group: 'Analysis',
			key: 'analysis.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Enable analysis',
			description: 'Master switch for weasel-pattern analysis.'
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.overselling.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Overselling',
			description: 'Flags unsubstantiated success claims.'
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.overselling.false_positive_filters',
			kind: 'string_list',
			default: [],
			value: [],
			label: 'False-positive filters',
			description: 'Phrases that suppress a match.',
			max_len: 256
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.justifying_shortcuts.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Justifying shortcuts',
			description: 'Flags rationalising a shortcut.'
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.justifying_shortcuts.false_positive_filters',
			kind: 'string_list',
			default: [],
			value: [],
			label: 'False-positive filters',
			description: 'Phrases that suppress a match.',
			max_len: 256
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.deflecting_blame.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Deflecting blame',
			description: 'Flags blaming tools or the environment.'
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.deflecting_blame.false_positive_filters',
			kind: 'string_list',
			default: [],
			value: [],
			label: 'False-positive filters',
			description: 'Phrases that suppress a match.',
			max_len: 256
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.avoiding_work.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Avoiding work',
			description: 'Flags deferring or skipping requested work.'
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.avoiding_work.false_positive_filters',
			kind: 'string_list',
			default: [],
			value: [],
			label: 'False-positive filters',
			description: 'Phrases that suppress a match.',
			max_len: 256
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.faking_understanding.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Faking understanding',
			description: 'Flags claiming to understand without verifying.'
		}),
		meta({
			group: 'Analysis',
			key: 'analysis.categories.faking_understanding.false_positive_filters',
			kind: 'string_list',
			default: [],
			value: [],
			label: 'False-positive filters',
			description: 'Phrases that suppress a match.',
			max_len: 256
		}),
		meta({
			group: 'Weasel intercept',
			key: 'weasel_intercept.forbidden_words',
			kind: 'string_list',
			default: [],
			value: ['Done.', 'verified', 'confirmed', 'all tests pass'],
			label: 'Forbidden words',
			description: 'Literal phrases scanned for in streamed output.',
			max_len: 256
		}),
		meta({
			group: 'Weasel intercept',
			key: 'weasel_intercept.correction_prompt',
			kind: 'text',
			default:
				"You used the word '{word}' in your previous response, which is a forbidden quality pattern. Rewrite your response. Start with: 'I used a weasel word, let me fix that:' then give the corrected answer without using '{word}'.",
			value: 'Please rephrase with supporting evidence rather than assertion.',
			label: 'Correction prompt',
			description: 'Injected when a forbidden phrase is detected.',
			max_len: 4096
		})
	];

	function buildData(): FlaggingPageData {
		return {
			features: FEATURES,
			loadError: null,
			settings: SETTINGS,
			settingsLoadError: null
		};
	}

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
			return json(
				meta({
					group: 'Weasel intercept',
					key,
					kind: 'string',
					default: '',
					value: (value ?? '') as SettingMeta['value']
				})
			);
		}
		if (url.includes('/api/settings/features')) return json({ features: FEATURES });
		if (url.includes('/api/settings'))
			return json({ settings: SETTINGS, steerer_openrouter_configured_via_env: false });
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Flagging',
		component: FlaggingPage,
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
			return {};
		}
	]}
	args={{ data: buildData() }}
	parameters={{
		sveltekit_experimental: {
			state: { page: { url: new URL('http://localhost/settings/flagging'), params: {} } }
		}
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('Response flagging');
		await expect(canvasElement.textContent).toContain('Categories');
		await expect(canvasElement.textContent).toContain('Live correction');
		await expect(canvasElement.querySelectorAll('.dsc-card').length).toBe(7);
		await expect(canvasElement.querySelectorAll('[role="switch"]').length).toBeGreaterThan(0);
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<FlaggingPage {...args} />
		</div>
	{/snippet}
</Story>
