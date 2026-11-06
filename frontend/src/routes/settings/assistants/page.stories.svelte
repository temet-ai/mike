<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent } from 'storybook/test';
	import AssistantsPage from './+page.svelte';
	import type { FeatureDto, SettingMeta } from '$lib/api/settings';
	import type { AssistantHealth } from '$lib/api/assistant';
	import type { AssistantsPageData } from './+page';

	const FEATURES: FeatureDto[] = [
		{ key: 'prompt_assistant', enabled: true, scope: 'global', applies: 'immediate' }
	];

	const BUILT_IN_INSTRUCTIONS =
		'You help the operator edit the Claude Code system prompt that mike captures and rewrites. ' +
		'You can read: the captured prompt they are looking at, their filter edits, and the ' +
		"project's CLAUDE.md and rules. When asked to change something, answer with a before/after " +
		'pair. Match on the shortest text that is unique to the paragraph, say which models and ' +
		'Claude Code versions the change will reach, and point out where it contradicts the ' +
		"project's own instructions. Never save an edit yourself. The operator applies it.";

	function meta(
		o: Partial<SettingMeta> & Pick<SettingMeta, 'key' | 'kind' | 'value' | 'default'>
	): SettingMeta {
		return {
			group: 'Prompt assistant',
			scope: 'global',
			applies: 'new_sessions',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const SETTINGS: SettingMeta[] = [
		meta({
			key: 'prompt_assistant.model',
			kind: 'string',
			default: 'claude-sonnet-5',
			value: 'claude-sonnet-5',
			label: 'Model',
			max_len: 256
		}),
		meta({
			key: 'prompt_assistant.effort',
			kind: 'enum',
			default: 'medium',
			value: 'medium',
			label: 'Effort',
			description: 'How long the model thinks before answering.',
			enum_variants: ['low', 'medium', 'high']
		}),
		meta({
			key: 'prompt_assistant.instructions',
			kind: 'text',
			default: BUILT_IN_INSTRUCTIONS,
			value: BUILT_IN_INSTRUCTIONS,
			label: 'Instructions',
			description: 'What the assistant is told before every conversation.',
			max_len: 16384
		}),
		meta({
			key: 'prompt_assistant.read_prompt',
			kind: 'bool',
			default: true,
			value: true,
			label: 'The prompt you are looking at'
		}),
		meta({
			key: 'prompt_assistant.read_edits',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Your filter edits'
		}),
		meta({
			key: 'prompt_assistant.read_project_instructions',
			kind: 'bool',
			default: true,
			value: true,
			label: "The project's CLAUDE.md and rules"
		}),
		meta({
			key: 'prompt_assistant.read_versions',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Other models and Claude Code versions',
			applies: 'next_conversation'
		}),
		meta({
			key: 'prompt_assistant.read_memory',
			kind: 'bool',
			default: false,
			value: false,
			label: 'Project memory'
		}),
		meta({
			key: 'prompt_assistant.may_apply',
			kind: 'enum',
			default: 'suggest',
			value: 'suggest',
			label: 'What it can change',
			applies: 'next_message',
			enum_variants: ['suggest', 'apply']
		}),
		meta({
			key: 'prompt_assistant.capture',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Capture its own traffic'
		}),
		meta({
			key: 'prompt_assistant.max_turns',
			kind: 'int',
			default: 8,
			value: 8,
			label: 'Steps per answer',
			min_i64: 1,
			max_i64: 40
		}),
		meta({
			key: 'prompt_assistant.daily_budget_usd',
			kind: 'float',
			default: 1.0,
			value: 1.0,
			label: 'Daily spending limit',
			applies: 'new_sessions',
			min_f64: 0,
			max_f64: 100
		}),
		meta({
			key: 'prompt_assistant.keep_days',
			kind: 'int',
			default: 30,
			value: 30,
			label: 'Keep conversations for',
			min_i64: 1,
			max_i64: 365
		})
	];

	const HEALTH: AssistantHealth = {
		ok: true,
		enabled: true,
		model: 'claude-sonnet-5',
		effort: 'medium',
		may_apply: 'suggest',
		spent_today_usd: 0.14,
		daily_budget_usd: 1.0,
		auth_mode: 'claude_code_login',
		credentials_available: true,
		provider: 'anthropic'
	};

	const HEALTH_API_KEY: AssistantHealth = {
		...HEALTH,
		auth_mode: 'api_key',
		credentials_available: true
	};

	const OBSERVED_CLAUDE_MODEL_IDS = [
		'claude-sonnet-5',
		'claude-opus-5',
		'claude-haiku-4-5-20251001',
		'claude-fable-5-1',
		'claude-fable-5',
		'claude-opus-5-5',
		'claude-opus-4-8',
		'claude-haiku-4-5'
	];

	function buildData(health: AssistantHealth = HEALTH): AssistantsPageData {
		return {
			features: FEATURES,
			loadError: null,
			settings: SETTINGS,
			settingsLoadError: null,
			health,
			healthError: null,
			observedClaudeModelIds: OBSERVED_CLAUDE_MODEL_IDS
		};
	}

	function fetchStub(
		input: RequestInfo | URL,
		init: RequestInit | undefined,
		health: AssistantHealth
	): Promise<Response> {
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
			const existing = FEATURES.find((f) => f.key === key);
			return json({
				key,
				enabled,
				scope: existing?.scope ?? 'global',
				applies: existing?.applies ?? 'immediate'
			});
		}
		if (url.includes('/api/settings/') && !url.includes('/features')) {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? 'x');
			let value: unknown;
			if (init && typeof init.body === 'string') {
				try {
					value = JSON.parse(init.body).value;
				} catch {}
			}
			const existing = SETTINGS.find((s) => s.key === key);
			if (existing)
				return json({ ...existing, value: (value ?? existing.value) as SettingMeta['value'] });
			return json(
				meta({ key, kind: 'string', default: '', value: (value ?? '') as SettingMeta['value'] })
			);
		}
		if (url.includes('/api/settings/features')) return json({ features: FEATURES });
		if (url.includes('/api/settings'))
			return json({ settings: SETTINGS, steerer_openrouter_configured_via_env: false });
		if (url.includes('/api/assistant/health')) return json(health);
		if (url.includes('/api/usage/models'))
			return json({
				days: 30,
				models: OBSERVED_CLAUDE_MODEL_IDS.map((model, i) => ({
					model,
					requests: 100 - i,
					input_tokens: 1000,
					output_tokens: 500,
					cache_creation_tokens: 0,
					cache_read_tokens: 0,
					last_seen: new Date().toISOString()
				}))
			});
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Assistants',
		component: AssistantsPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) =>
				fetchStub(input, init, HEALTH)) as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData(HEALTH) }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('Assistants');
		await expect(canvasElement.textContent).toContain(
			'Prompt assistant — rewords prompts in the filter workbench'
		);
		await expect(canvasElement.textContent).toContain('Claude Code login on this machine');
		await expect(canvasElement.textContent).toContain('Today: $0.14 of $1.00');
		await expect(canvasElement.textContent).toContain('change in Integrations');
		await expect(canvasElement.textContent).toContain('Running on');
		// G10: "applies" copy for the on-demand assistant, never "next cycle" (a standing
		// worker's polling loop it doesn't have).
		await expect(canvasElement.textContent).toContain('Next message');
		await expect(canvasElement.textContent).toContain('Next conversation');
		await expect(canvasElement.textContent).not.toContain('next cycle');

		// The model picker must show every Claude model captured traffic has actually
		// used, not just the three the maintained catalogue used to hardcode.
		for (const modelId of OBSERVED_CLAUDE_MODEL_IDS) {
			await expect(canvasElement.textContent).toContain(modelId);
		}

		await expect(canvasElement.textContent).toContain('What it can read');
		await expect(canvasElement.textContent).toContain('What it can change');
		await expect(canvasElement.textContent).toContain('Limits');

		const opusRadio = canvasElement.querySelector('input[value="claude-opus-5-5"]');
		await expect(opusRadio).not.toBeNull();
		await userEvent.click(opusRadio as Element);
	}}
/>

<!--
	Explicitly named coverage, not a per-state duplicate under storybook-single-representative:
	the dispatched task asked by name for both Anthropic auth-mode states (`claude_code_login`
	above, `api_key` here) to be covered "across stories" with `getAssistantHealth`'s
	`auth_mode`/`credentials_available` varied — this is the operator-named-variant carve-out
	that rule allows, not a design-exploration or empty/loading/error duplicate.
-->
<Story
	name="ApiKeyMode"
	loaders={[
		async () => {
			globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) =>
				fetchStub(input, init, HEALTH_API_KEY)) as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData(HEALTH_API_KEY) }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('Anthropic access for the prompt assistant');
		await expect(canvasElement.textContent).toContain('API key configured');
		await expect(canvasElement.textContent).not.toContain('Claude Code login on this machine');

		const link = canvasElement.querySelector('.st-env-label a[href*="/settings/integrations"]');
		await expect(link).not.toBeNull();
		await expect(link?.getAttribute('href')).toContain('#anthropic');
	}}
/>
