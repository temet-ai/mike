<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, waitFor, userEvent, within } from 'storybook/test';
	import SteeringPage from './+page.svelte';
	import type { FeatureDto, SettingMeta } from '$lib/api/settings';
	import type { OpenRouterUsageDto } from '$lib/api/usage';
	import type { SteererDto, ModelEntry } from '$lib/types';
	import type { SteeringPageData } from './+page';
	import { buildProviders } from '$lib/components/settings/setup/setupFixtures';
	import { calibrationFixture } from '$lib/decisions/__fixtures__/calibration';

	const FEATURES: FeatureDto[] = [
		{ key: 'steerer', enabled: true, scope: 'global', applies: 'live_disable' },
		{ key: 'judge_worker', enabled: true, scope: 'global', applies: 'live_disable' },
		{ key: 'session_judge', enabled: true, scope: 'global', applies: 'live_disable' },
		{ key: 'decisions', enabled: true, scope: 'global', applies: 'immediate' }
	];

	function meta(
		o: Partial<SettingMeta> & Pick<SettingMeta, 'key' | 'kind' | 'value' | 'default'>
	): SettingMeta {
		return {
			group: 'Steerer',
			scope: 'global',
			applies: 'on_restart',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const SETTINGS: SettingMeta[] = [
		meta({
			group: 'Decisions',
			key: 'decisions.timeout_ms',
			kind: 'int',
			default: 3000,
			value: 3000,
			label: 'Decision timeout (ms)',
			description: 'How long mike waits for Jev before acting as if the layer were off.',
			min_i64: 200,
			max_i64: 30000
		}),
		meta({
			group: 'Decisions',
			key: 'decisions.turn_judge.mode',
			kind: 'enum',
			default: 'on',
			value: 'on',
			label: 'Turn judge mode',
			description: 'Mode of the turn judge decision point.',
			enum_variants: ['off', 'shadow', 'on']
		}),
		meta({
			key: 'steerer.model',
			kind: 'string',
			default: 'openai/gpt-5.6-luna',
			value: 'openai/gpt-5.6-luna',
			label: 'LLM model',
			description: 'OpenRouter model used for steerer verdict calls.',
			max_len: 256
		}),
		meta({
			key: 'steerer.request_timeout_secs',
			kind: 'int',
			default: 30,
			value: 30,
			label: 'Request timeout (s)',
			description: 'Timeout for steerer LLM calls.',
			min_i64: 5,
			max_i64: 300
		}),
		meta({
			key: 'steerer.quiet_period_secs',
			kind: 'int',
			default: 60,
			value: 60,
			label: 'Quiet period (s)',
			description: 'Minimum gap between steerer interventions on one session.',
			min_i64: 0,
			max_i64: 3600
		}),
		meta({
			key: 'steerer.quiescence_minutes',
			kind: 'int',
			default: 5,
			value: 5,
			label: 'Quiescence fallback (mins)',
			description:
				'Minutes of silence on an open turn before the verifier steerer judges it without waiting.',
			min_i64: 1,
			max_i64: 60
		}),
		meta({
			key: 'steerer.refocus.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Refocus: enabled',
			description: 'Enable the steerer refocus mode (injects grounding messages on anchor drift).'
		}),
		meta({
			key: 'steerer.refocus.confidence_threshold',
			kind: 'float',
			default: 0.7,
			value: 0.7,
			label: 'Refocus: confidence threshold',
			description: 'LLM confidence score above which a refocus injection is triggered.',
			min_f64: 0.0,
			max_f64: 1.0
		}),
		meta({
			key: 'steerer.refocus.debounce_turns',
			kind: 'int',
			default: 2,
			value: 2,
			label: 'Refocus: debounce turns',
			description: 'Turns that must pass after a refocus injection before another can fire.',
			min_i64: 0,
			max_i64: 20
		}),
		meta({
			key: 'steerer.openrouter_api_key_env',
			kind: 'string',
			default: 'MIKE_STEERER_OPENROUTER_API_KEY',
			value: 'MIKE_STEERER_OPENROUTER_API_KEY',
			label: 'OpenRouter API key env var',
			description:
				'Name of the environment variable the steerer reads the OpenRouter API key from.',
			max_len: 256
		})
	];

	const OPENROUTER_USAGE_CONFIGURED: OpenRouterUsageDto = {
		configured: true,
		label: 'sk-or-v1-4d2...d99',
		limit: null,
		limit_remaining: null,
		limit_reset: null,
		usage_total: null,
		usage_daily: null,
		usage_weekly: null,
		usage_monthly: null,
		credits_total: null,
		credits_used: null,
		credits_remaining: null,
		is_free_tier: null
	};
	const OPENROUTER_USAGE_UNCONFIGURED: OpenRouterUsageDto = {
		configured: false,
		label: null,
		limit: null,
		limit_remaining: null,
		limit_reset: null,
		usage_total: null,
		usage_daily: null,
		usage_weekly: null,
		usage_monthly: null,
		credits_total: null,
		credits_used: null,
		credits_remaining: null,
		is_free_tier: null
	};
	let openrouterKeyConfigured = false;
	let remountKey = $state(0);
	let modelPatches: unknown[] = [];

	const STEERERS: SteererDto[] = [
		{
			id: 1,
			uuid: 'aaaaaaaa-0000-0000-0000-000000000001',
			name: 'refocus',
			display_name: 'Refocus',
			description: 'Injects a grounding message when the session drifts.',
			agent_id: 1,
			authority: 'supply_context',
			mode: 'augment',
			model: '',
			effective_model: 'openai/gpt-5.6-luna',
			model_inherited: true,
			priority: 10,
			enabled: true,
			auto_enrol: false,
			kind: 'refocus',
			prompt: null,
			trigger_point: 'response',
			invocation: 'every_turn',
			pattern: null,
			config: { confidence_threshold: 0.7, debounce_turns: 2 },
			created_at: '2026-06-29T00:00:00Z',
			updated_at: '2026-06-29T00:00:00Z'
		},
		{
			id: 2,
			uuid: 'aaaaaaaa-0000-0000-0000-000000000002',
			name: 'verifier',
			display_name: 'Verifier',
			description: 'Nudges when the assistant claims done without evidence.',
			agent_id: 2,
			authority: 'supply_context',
			mode: 'augment',
			model: 'google/gemini-3.8-flash',
			effective_model: 'google/gemini-3.8-flash',
			model_inherited: false,
			priority: 20,
			enabled: true,
			auto_enrol: true,
			kind: 'verifier',
			prompt: null,
			project_id: 7,
			project_path: '/Users/dev/projects/my-project',
			trigger_point: 'response',
			invocation: 'pattern',
			pattern: 'done\ncompleted\ntask is finished',
			config: { confidence_threshold: 0.7, debounce_turns: 1 },
			created_at: '2026-06-29T00:00:00Z',
			updated_at: '2026-06-29T00:00:00Z'
		}
	];

	const MODELS: ModelEntry[] = [
		{
			id: 'openai/gpt-5.6-luna',
			name: 'GPT 5.6 Luna',
			prompt_price_per_1m: 0.15,
			completion_price_per_1m: 0.6,
			context_length: 128000,
			known_good: true,
			supports_structured_output: true,
			description:
				"OpenAI's small, low-cost chat model. Fast and cheap — a strong default for steerer verdicts.",
			input_modalities: ['text', 'image'],
			supports_tools: true,
			supports_reasoning: false
		},
		{
			id: 'google/gemini-3.8-flash',
			name: 'Claude Haiku 4.5',
			prompt_price_per_1m: 1,
			completion_price_per_1m: 5,
			context_length: 200000,
			known_good: true,
			supports_structured_output: true
		},
		{
			id: 'moonshotai/kimi-k2.7-code',
			name: 'Kimi K2.7 Code',
			prompt_price_per_1m: 0.3,
			completion_price_per_1m: 1.2,
			context_length: 256000,
			known_good: false,
			supports_structured_output: true,
			supports_tools: true,
			supports_reasoning: true
		},
		{
			id: 'meta-llama/llama-3-8b-instruct',
			name: 'Llama 3 8B Instruct',
			prompt_price_per_1m: 0.05,
			completion_price_per_1m: 0.1,
			context_length: 8192,
			known_good: false,
			supports_structured_output: false
		}
	];

	function buildData(steererModel?: string): SteeringPageData {
		return {
			features: FEATURES,
			loadError: null,
			settings:
				steererModel === undefined
					? SETTINGS
					: SETTINGS.map((s) => (s.key === 'steerer.model' ? { ...s, value: steererModel } : s)),
			settingsLoadError: null,
			steerers: STEERERS,
			steerersLoadError: null,
			models: MODELS,
			modelsLoadError: null
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
		if (url.includes('/api/decisions/calibration')) return json(calibrationFixture());
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
				scope: existing?.scope ?? 'session',
				applies: existing?.applies ?? 'new_sessions'
			});
		}
		if (url.includes('/api/steering/guard/metrics'))
			return json({
				since: '2026-09-11T00:00:00Z',
				until: '2026-09-25T00:00:00Z',
				metrics: [],
				audit: { audit_set_sessions: 0, audit_set_fires: 0, offending_evaluations: [] }
			});
		if (url.includes('/api/steering/guard'))
			return json({
				enabled: false,
				steerer_id: 17,
				steerer_enabled: false,
				rubric_version: 'guard-v1-2026-09',
				gate_version: 'gate-v1',
				steering_facts_version: 1,
				labeller_version: 'guard-labeller-v1',
				labeller: { state: 'ok', since: null, consecutive_failures: 0 },
				settings: { max_calls_per_day: 150, max_label_calls_per_day: 80, recall_sample_pct: 10 },
				questions: []
			});
		if (url.includes('/api/llm/providers')) {
			const providers = buildProviders();
			for (const p of providers.providers) {
				if (p.id === 'openrouter') p.available = true;
				p.models = [...p.models, ...p.models];
			}
			return json(providers);
		}
		if (url.includes('/api/usage/openrouter'))
			return json(
				openrouterKeyConfigured ? OPENROUTER_USAGE_CONFIGURED : OPENROUTER_USAGE_UNCONFIGURED
			);
		if (url.includes('/api/settings/') && !url.includes('/features')) {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? 'x');
			let value: unknown;
			if (init && typeof init.body === 'string') {
				try {
					value = JSON.parse(init.body).value;
				} catch {}
			}
			const existing = SETTINGS.find((s) => s.key === key);
			if (key === 'steerer.model') modelPatches.push(value);
			if (existing)
				return json({ ...existing, value: (value ?? existing.value) as SettingMeta['value'] });
			return json(
				meta({ key, kind: 'string', default: '', value: (value ?? '') as SettingMeta['value'] })
			);
		}
		if (url.includes('/api/settings/features')) return json({ features: FEATURES });
		if (url.includes('/api/settings'))
			return json({ settings: SETTINGS, steerer_openrouter_configured_via_env: false });
		if (url.endsWith('/api/steerers/model')) return json({ count: STEERERS.length });
		if (url.includes('/api/steerers') && url.match(/\/api\/steerers\/\d+/)) {
			const id = parseInt(url.split('/').pop() ?? '1', 10);
			const s = STEERERS.find((x) => x.id === id);
			if (s) {
				let enabled = s.enabled;
				let model = s.model;
				if (init?.body && typeof init.body === 'string') {
					try {
						const body = JSON.parse(init.body);
						enabled = body.enabled ?? s.enabled;
						model = body.model ?? s.model;
					} catch {}
				}
				return json({
					steerer: {
						...s,
						enabled,
						model,
						model_inherited: model === '',
						effective_model: model || s.effective_model
					},
					warnings: []
				});
			}
		}
		if (url.includes('/api/steerers')) return json(STEERERS);
		if (url.includes('/api/models')) return json({ models: MODELS });
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Steering',
		component: SteeringPage,
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
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('Steering');
		await waitFor(() => expect(canvasElement.textContent).toContain('not configured'));
		const integrationsLink = canvasElement.querySelector('a[href="/settings/integrations"]');
		await expect(integrationsLink).not.toBeNull();
		await expect(integrationsLink?.textContent).toContain('Integrations');
		await expect(canvasElement.textContent).not.toContain('sk-or-v1-4d2...d99');
		await expect(canvasElement.textContent).not.toContain('Save & Verify');

		openrouterKeyConfigured = true;
		remountKey++;
		await waitFor(() => expect(canvasElement.textContent).toContain('manage in Integrations'));
		await expect(canvasElement.textContent).toContain('OpenRouter API key:');
		await expect(canvasElement.textContent).toContain('configured');
		await expect(canvasElement.textContent).not.toContain('not configured');
		await expect(canvasElement.textContent).not.toContain('sk-or-v1-4d2...d99');
		await expect(canvasElement.textContent).not.toContain('Save & Verify');
		const configuredIntegrationsLink = canvasElement.querySelector(
			'a[href="/settings/integrations"]'
		);
		await expect(configuredIntegrationsLink).not.toBeNull();
		await expect(configuredIntegrationsLink?.textContent).toContain('Integrations');
		await expect(canvasElement.textContent).toContain('Refocus');
		await expect(canvasElement.textContent).toContain('Verifier');
		await expect(canvasElement.querySelectorAll('[role="switch"]').length).toBeGreaterThan(0);
		await expect(canvasElement.textContent).toContain('Global');
		await expect(canvasElement.textContent).toContain('/Users/dev/projects/my-project');
		await expect(canvasElement.textContent).toContain('openai/gpt-5.6-luna');
		await expect(canvasElement.textContent).toContain('moonshotai/kimi-k2.7-code');
		await expect(canvasElement.querySelectorAll('.mt-badge--recommended').length).toBeGreaterThan(
			0
		);

		await expect(canvasElement.textContent).toContain('Which model judges each turn');
		await expect(canvasElement.textContent).toContain(
			'Every turn of an enrolled session is sent to this model.'
		);
		const nowBanner = canvasElement.querySelector('.mt-now');
		await expect(nowBanner).not.toBeNull();
		await expect(nowBanner?.querySelector('.mt-now__id')?.textContent).toBe('openai/gpt-5.6-luna');
		await expect(canvasElement.querySelector('.mt-group-label')?.textContent?.trim()).toBe(
			'In use'
		);
		await expect(canvasElement.querySelector('.mt-row--pinned .mt-model-id')?.textContent).toBe(
			'openai/gpt-5.6-luna'
		);

		const modelBadges = canvasElement.querySelectorAll('.st-fleet-model-badge');
		await expect(modelBadges.length).toBe(2);
		await expect(Array.from(modelBadges).map((b) => b.textContent?.trim())).toEqual([
			'inherit',
			'override'
		]);
		await expect(canvasElement.textContent).toContain('google/gemini-3.8-flash');

		function openPicker(): HTMLElement {
			const el = document.body.querySelector(
				'[data-slot="popover-content"][data-state="open"]'
			) as HTMLElement | null;
			if (!el) throw new Error('no picker popover is open');
			return el;
		}

		const bulkTrigger = Array.from(canvasElement.querySelectorAll('button')).find((b) =>
			/Set all steerers to/.test(b.textContent ?? '')
		);
		await expect(bulkTrigger).not.toBeUndefined();
		await userEvent.click(bulkTrigger as HTMLButtonElement);
		const bulkInheritRow = await waitFor(() => {
			const el = openPicker().querySelector('.mpp-row--inherit') as HTMLButtonElement | null;
			if (!el) throw new Error('bulk picker inherit row not rendered');
			return el;
		});
		await userEvent.click(bulkInheritRow);
		await waitFor(() =>
			expect(canvasElement.textContent).toContain('Set 2 steerers to inherit the default.')
		);

		const rowModelTriggers = Array.from(
			canvasElement.querySelectorAll('button[aria-label="Model"]')
		);
		await expect(rowModelTriggers.length).toBe(2);
		await userEvent.click(rowModelTriggers[0] as HTMLButtonElement);
		const kimiRow = await waitFor(() => {
			const rows = Array.from(openPicker().querySelectorAll('.mpp-row:not(.mpp-row--inherit)'));
			const el = rows.find((r) => /kimi-k2\.7-code/.test(r.textContent ?? '')) as
				HTMLButtonElement | undefined;
			if (!el) throw new Error('kimi model row not rendered in row picker');
			return el;
		});
		await userEvent.click(kimiRow);
		await waitFor(() => expect(canvasElement.textContent).toContain('moonshotai/kimi-k2.7-code'));

		await userEvent.click(rowModelTriggers[1] as HTMLButtonElement);
		const rowPicker = await waitFor(() => {
			const picker = openPicker();
			if (!picker.querySelector('.mpp-row--pinned')) {
				throw new Error('picker did not pin the current model');
			}
			return picker;
		});
		await expect(rowPicker.querySelector('.mpp-row--pinned')?.textContent).toContain(
			'google/gemini-3.8-flash'
		);
		await expect(rowPicker.querySelector('.mpp-section-label--inuse')?.textContent?.trim()).toBe(
			'In use'
		);
		const haikuRows = Array.from(rowPicker.querySelectorAll('.mpp-name')).filter(
			(el) => el.textContent?.trim() === 'google/gemini-3.8-flash'
		);
		await expect(haikuRows).toHaveLength(1);
		await userEvent.keyboard('{Escape}');

		await expect(canvasElement.textContent).not.toContain('Turn judge — scores past turns');
		await expect(canvasElement.textContent).not.toContain('Session judge — scores whole sessions');
		await expect(
			canvasElement.querySelector('button[aria-label="Enable Turn judge — scores past turns"]')
		).toBeNull();
		await expect(
			canvasElement.querySelector(
				'button[aria-label="Enable Session judge — scores whole sessions"]'
			)
		).toBeNull();
		await expect(canvasElement.textContent).toContain('Workers page');
		await expect(canvasElement.querySelector('a[href*="/settings/workers"]')).not.toBeNull();

		await expect(
			canvasElement.querySelector(
				'[aria-label="Turning it off takes about a minute, with no restart. Turning it back on needs a restart."]'
			)
		).not.toBeNull();
		const steererSwitch = canvasElement.querySelector(
			'button[aria-label="Enable Steerer — acts on live sessions"]'
		);
		await expect(steererSwitch?.getAttribute('aria-checked')).toBe('true');

		await expect(canvasElement.textContent).toContain('Refocus');
		await expect(canvasElement.textContent).toContain('Refocus: enabled');
		await expect(canvasElement.textContent).toContain('Refocus: confidence threshold');
		await expect(canvasElement.textContent).toContain('Refocus: debounce turns');
		await expect(canvasElement.textContent).not.toContain('Correction classifier');
		await expect(canvasElement.textContent).toContain('OpenRouter API key env var');
		const keyEnvInput = canvasElement.querySelector(
			'input[aria-label="OpenRouter API key env var"]'
		) as HTMLInputElement | null;
		await expect(keyEnvInput).not.toBeNull();
		await expect(keyEnvInput?.value).toBe('MIKE_STEERER_OPENROUTER_API_KEY');
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			{#key remountKey}
				<SteeringPage {...args} />
			{/key}
		</div>
	{/snippet}
</Story>

<Story
	name="SteererOnSubscription"
	loaders={[
		async () => {
			openrouterKeyConfigured = false;
			modelPatches = [];
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData('claude/sonnet') }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const provider = await canvas.findByLabelText('Provider');
		await waitFor(() => expect((provider as HTMLSelectElement).value).toBe('claude'));
		// The fleet rows below also have buttons labelled "Model"; the steerer's chooser is the <select>.
		const modelSelect = canvas
			.getAllByLabelText('Model')
			.find((el): el is HTMLSelectElement => el instanceof HTMLSelectElement);
		await expect(modelSelect?.value).toBe('claude/sonnet');
		await expect(canvasElement.textContent).toContain('runs on your own Claude plan');
		await expect(canvasElement.textContent).not.toContain('billed per call');
		await expect(canvasElement.textContent).not.toContain('Needs an OpenRouter key');
		await expect(canvasElement.textContent).not.toContain('OpenRouter API key');
		await expect(canvasElement.querySelector('.mt-now')).toBeNull();
		await expect(
			canvasElement
				.querySelector('button[aria-label="Enable Steerer — acts on live sessions"]')
				?.hasAttribute('disabled')
		).toBe(false);

		// Browsing OpenRouter shows its paid catalogue but saves nothing.
		await userEvent.selectOptions(provider, 'openrouter');
		await waitFor(() => expect(canvasElement.textContent).toContain('billed per call'));
		await expect(canvasElement.textContent).toContain('OpenRouter API key');
		await expect(modelPatches).toEqual([]);
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			{#key remountKey}
				<SteeringPage {...args} />
			{/key}
		</div>
	{/snippet}
</Story>
