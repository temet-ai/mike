<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import IntegrationsPage from './+page.svelte';
	import type { ModelUsageRow, OpenRouterUsageDto } from '$lib/api/usage';
	import type { SteererDto } from '$lib/types';
	import type { AnthropicIntegrationStatus, CodexIntegrationStatus } from '$lib/api/integrations';
	import type { IntegrationsPageData } from './+page';
	import {
		buildFixtureSpend,
		buildFixtureSpendWithoutCallerBreakdown,
		buildFixtureUsageWithoutSpendFields
	} from '$lib/components/llmSpend/llmSpendFixtures';

	const ANTHROPIC_STATUS: AnthropicIntegrationStatus = {
		mode: 'claude_code_login',
		configured: false,
		masked_key: null
	};
	const CODEX_STATUS: CodexIntegrationStatus = {
		installed: true,
		logged_in: true,
		account_type: 'chatgpt',
		plan_type: 'pro',
		email: 'operator@example.com',
		selected: false,
		rate_limits: {
			rateLimits: {
				primary: { usedPercent: 24, windowDurationMins: 300, resetsAt: 1790496000 },
				secondary: { usedPercent: 41, windowDurationMins: 10080, resetsAt: 1790928000 }
			}
		},
		usage: {
			summary: { lifetimeTokens: 12800000, peakDailyTokens: 840000 },
			dailyUsageBuckets: []
		},
		error: null
	};

	const OPENROUTER_UNCONFIGURED: OpenRouterUsageDto = {
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

	const ALL_CLAUDE_MODEL_IDS = [
		'claude-sonnet-5',
		'claude-opus-5',
		'claude-haiku-4-5-20251001',
		'claude-fable-5-1',
		'claude-fable-5',
		'claude-opus-5-5',
		'claude-opus-4-8',
		'claude-haiku-4-5'
	];

	const JUNK_MODEL_ID =
		'bedrock/anthropic.claude-3-5-sonnet-20241022-v2:0##legacy-alias-DO-NOT-USE-experimental-routing-path';

	const PROMPT_FRAGMENT_JUNK_MODEL_ID =
		'fable i want you to take the above work and create us a detailed step by step list of work';

	const MODEL_ROWS: ModelUsageRow[] = [
		{
			model: 'claude-sonnet-5',
			requests: 86705,
			input_tokens: 8183126,
			output_tokens: 60484970,
			cache_creation_tokens: 559421838,
			cache_read_tokens: 20156033465,
			last_seen: '2026-09-23T08:30:15Z'
		},
		{
			model: 'claude-opus-5',
			requests: 16246,
			input_tokens: 1776262,
			output_tokens: 14206465,
			cache_creation_tokens: 218389827,
			cache_read_tokens: 4036119359,
			last_seen: '2026-09-22T19:10:21Z'
		},
		{
			model: 'claude-haiku-4-5-20251001',
			requests: 10677,
			input_tokens: 19111579,
			output_tokens: 1508304,
			cache_creation_tokens: 2717023,
			cache_read_tokens: 30644598,
			last_seen: '2026-09-23T08:29:29Z'
		},
		{
			model: 'claude-fable-5-1',
			requests: 4699,
			input_tokens: 1152435,
			output_tokens: 6602025,
			cache_creation_tokens: 63070946,
			cache_read_tokens: 1391318865,
			last_seen: '2026-09-23T06:05:43Z'
		},
		{
			model: 'claude-fable-5',
			requests: 3941,
			input_tokens: 2224101,
			output_tokens: 6018538,
			cache_creation_tokens: 66442047,
			cache_read_tokens: 931919958,
			last_seen: '2026-09-02T08:41:14Z'
		},
		{
			model: 'claude-opus-5-5',
			requests: 627,
			input_tokens: 68679,
			output_tokens: 471663,
			cache_creation_tokens: 3755650,
			cache_read_tokens: 121789239,
			last_seen: '2026-09-23T08:29:46Z'
		},
		{
			model: 'claude-opus-4-8',
			requests: 302,
			input_tokens: 28327,
			output_tokens: 402023,
			cache_creation_tokens: 4800547,
			cache_read_tokens: 68920821,
			last_seen: '2026-09-09T10:28:25Z'
		},
		{
			model: 'claude-haiku-4-5',
			requests: 9,
			input_tokens: 6384,
			output_tokens: 3631,
			cache_creation_tokens: 132487,
			cache_read_tokens: 118584,
			last_seen: '2026-09-23T04:14:03Z'
		},
		{
			model: JUNK_MODEL_ID,
			requests: 4,
			input_tokens: 998,
			output_tokens: 512,
			cache_creation_tokens: 0,
			cache_read_tokens: 0,
			last_seen: '2026-06-02T00:00:00Z'
		},
		{
			model: PROMPT_FRAGMENT_JUNK_MODEL_ID,
			requests: 1,
			input_tokens: 214,
			output_tokens: 89,
			cache_creation_tokens: 0,
			cache_read_tokens: 0,
			last_seen: '2026-05-14T00:00:00Z'
		},
		{
			model: null,
			requests: 2,
			input_tokens: 340,
			output_tokens: 120,
			cache_creation_tokens: 0,
			cache_read_tokens: 0,
			last_seen: '2026-04-30T00:00:00Z'
		}
	];

	function steerer(
		overrides: Partial<SteererDto> & Pick<SteererDto, 'id' | 'display_name' | 'model' | 'enabled'>
	): SteererDto {
		return {
			uuid: `00000000-0000-0000-0000-00000000000${overrides.id}`,
			name: overrides.display_name.toLowerCase().replace(/\s+/g, '-'),
			description: '',
			agent_id: overrides.id,
			authority: 'supply_context',
			mode: 'augment',
			priority: 0,
			auto_enrol: false,
			trigger_point: 'response',
			invocation: 'every_turn',
			pattern: null,
			config: { confidence_threshold: 0.7, debounce_turns: 1 },
			created_at: '2026-06-01T00:00:00Z',
			updated_at: '2026-06-01T00:00:00Z',
			...overrides
		};
	}

	const STEERERS: SteererDto[] = [
		steerer({
			id: 1,
			display_name: 'Refocus',
			model: 'google/gemini-3.5-flash-lite',
			enabled: true
		}),
		steerer({
			id: 2,
			display_name: 'Verifier',
			model: 'google/gemini-3.5-flash-lite',
			enabled: false
		}),
		steerer({
			id: 3,
			display_name: 'Memory Keeper',
			model: 'google/gemini-3.7-flash',
			enabled: true
		})
	];

	function buildData(): IntegrationsPageData {
		return {
			openrouterUsage: OPENROUTER_UNCONFIGURED,
			openrouterError: null,
			openrouterUnavailable: false,
			modelUsage: MODEL_ROWS,
			modelUsageError: null,
			modelUsageUnavailable: false,
			steerers: STEERERS,
			steerersError: null,
			steerersUnavailable: false,
			anthropicStatus: ANTHROPIC_STATUS,
			anthropicError: null,
			anthropicUnavailable: false,
			codexStatus: CODEX_STATUS,
			codexError: null,
			codexUnavailable: false,
			provider: 'anthropic',
			spend: buildFixtureSpend(),
			spendError: null
		};
	}

	const APPLIED_KEY_LABEL = 'sk-or-v1-4d2...d99';
	const OPENROUTER_APPLIED: OpenRouterUsageDto = {
		...OPENROUTER_UNCONFIGURED,
		configured: true,
		label: APPLIED_KEY_LABEL,
		credits_unavailable: true
	};
	let currentOpenRouterUsage: OpenRouterUsageDto = OPENROUTER_UNCONFIGURED;

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url =
			typeof input === 'string'
				? input
				: input instanceof URL
					? input.href
					: (input as Request).url;
		const json = (body: unknown, status = 200) =>
			Promise.resolve(
				new Response(JSON.stringify(body), {
					status,
					headers: { 'content-type': 'application/json' }
				})
			);
		if (url.includes('/api/settings/steerer/openrouter/key')) {
			const parsed =
				typeof init?.body === 'string' ? (JSON.parse(init.body) as { key?: string }) : {};
			const key = parsed.key ?? '';
			if (!key.startsWith('sk-or-')) {
				return json(
					{ valid: false, error: 'OpenRouter keys start with "sk-or-" — this one does not.' },
					422
				);
			}
			currentOpenRouterUsage = OPENROUTER_APPLIED;
			return json({ valid: true, label: APPLIED_KEY_LABEL });
		}
		if (url.includes('/api/settings/assistant.provider') && init?.method === 'PATCH') {
			return json({ key: 'assistant.provider', kind: 'enum', value: 'codex' });
		}
		if (url.includes('/api/usage/openrouter')) return json(currentOpenRouterUsage);
		if (url.includes('/api/usage/models')) return json({ days: 30, models: MODEL_ROWS });
		if (url.includes('/api/llm/spend')) return json(buildFixtureSpend());
		if (url.includes('/api/steerers')) return json(STEERERS);
		if (url.includes('/api/settings/assistant/anthropic/cli-status')) {
			return json({
				installed: true,
				binary: 'claude',
				version: '1.0.0',
				logged_in: true,
				auth_method: 'oauth',
				email: 'operator@example.com',
				org_id: null,
				org_name: null,
				subscription_type: 'pro',
				error: null
			});
		}
		if (url.includes('/api/settings/assistant/anthropic')) return json(ANTHROPIC_STATUS);
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Integrations',
		component: IntegrationsPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			currentOpenRouterUsage = OPENROUTER_UNCONFIGURED;
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.querySelector('#anthropic')).not.toBeNull();
		await expect(canvasElement.textContent).toContain('Anthropic');
		await expect(canvasElement.textContent).toContain(
			'Credentials for the Claude Agent SDK-powered assistants'
		);
		await expect(canvasElement.textContent).toContain('Claude Code login on this machine');
		await expect(canvasElement.querySelector('#codex')).not.toBeNull();
		await expect(canvasElement.textContent).toContain('76% remaining');
		await expect(canvasElement.textContent).toContain('12.8M');
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getByRole('button', { name: /Codex/ }));
		await waitFor(() =>
			expect(canvasElement.querySelector('#codex')?.textContent).toContain('Selected')
		);

		await expect(canvasElement.textContent).toContain('No OpenRouter key configured');
		await expect(canvasElement.querySelector('[role="alert"]')).toBeNull();

		const table = canvasElement.querySelector('[data-testid="model-usage-table"]');
		await expect(table).not.toBeNull();
		await expect(canvasElement.textContent).toContain('8.2M');

		// Every Claude model the capture DB has actually seen must render here — the "Models
		// in use" table must never silently drop one (mike Integrations page bug).
		for (const modelId of ALL_CLAUDE_MODEL_IDS) {
			await expect(canvasElement.textContent).toContain(modelId);
		}

		await expect(canvasElement.textContent).toContain(JUNK_MODEL_ID);
		await expect(canvasElement.textContent).toContain(PROMPT_FRAGMENT_JUNK_MODEL_ID);

		await expect(canvasElement.textContent).toContain('(unknown model)');

		const setupLink = canvasElement.querySelector('.ori-section a[href="/settings/steering"]');
		await expect(setupLink).not.toBeNull();
		await expect(setupLink?.textContent?.trim()).toBe('Set it up in Steering settings');

		const topupLink = canvasElement.querySelector(
			'a[href="https://openrouter.ai/settings/credits"]'
		);
		await expect(topupLink).not.toBeNull();
		await expect(topupLink?.getAttribute('target')).toBe('_blank');
		await expect(topupLink?.getAttribute('rel')).toContain('noopener');
		await expect(topupLink?.textContent).toContain('Top up credits');

		const keyInput = canvas.getByLabelText('OpenRouter API key');
		const applyButton = canvas.getByRole('button', { name: 'Apply key' });
		await userEvent.type(keyInput, 'not-a-real-key');
		await userEvent.click(applyButton);
		await waitFor(() =>
			expect(canvasElement.textContent).toContain('OpenRouter keys start with "sk-or-"')
		);
		await expect(canvasElement.textContent).toContain('No OpenRouter key configured');

		await userEvent.clear(keyInput);
		await userEvent.type(keyInput, 'sk-or-v1-test-key');
		await userEvent.click(applyButton);
		await waitFor(() =>
			expect(canvasElement.textContent).toContain(`Key verified — now using ${APPLIED_KEY_LABEL}`)
		);
		await waitFor(() =>
			expect(canvasElement.textContent).toContain("Balance isn't available for this key type")
		);
		await expect(canvasElement.textContent).toContain(APPLIED_KEY_LABEL);
		await expect(canvasElement.textContent).not.toContain('No OpenRouter key configured');

		await expect(canvasElement.textContent).toContain('Models in use');
		const steererModels = canvasElement.querySelector('[data-testid="steerer-models-list"]');
		await expect(steererModels).not.toBeNull();
		await expect(canvasElement.textContent).toContain('google/gemini-3.5-flash-lite');
		await expect(canvasElement.textContent).toContain('google/gemini-3.7-flash');
		await expect(canvasElement.textContent).toContain('Refocus');
		await expect(canvasElement.textContent).toContain('Memory Keeper');
		await expect(canvasElement.textContent).toContain('Verifier');
		await expect(canvasElement.textContent).toContain('(disabled)');

		await expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
			document.documentElement.clientWidth + 1
		);

		// The key is now configured, so the spend section (fed by the same usage DTO) mounts here.
		// Callers are grouped by role — "Steerers" is the group header, "verifier" the short
		// member name (the "Steerer · " prefix is redundant once the header says it).
		await expect(canvasElement.textContent).toContain('Steerers');
		await expect(canvasElement.textContent).toContain('verifier');
		await expect(canvasElement.textContent).toContain('$0.044');
		const spendWindow = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === '30d'
		) as HTMLButtonElement;
		await expect(spendWindow).toBeTruthy();
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<IntegrationsPage {...args} />
		</div>
	{/snippet}
</Story>

<!-- Operator-requested (2026-09-28): prove the page degrades honestly, never crashes, when the
     live proxy's /api/llm/spend predates by_caller/by_day_caller and /api/usage/openrouter
     predates the recorded/unrecorded/days-left fields — the exact bug report that sent this
     view from a standalone /llm-spend route to living here. -->
<Story
	name="SpendBreakdownUnavailable"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{
		data: {
			...buildData(),
			openrouterUsage: { ...buildFixtureUsageWithoutSpendFields(), label: APPLIED_KEY_LABEL },
			spend: buildFixtureSpendWithoutCallerBreakdown()
		}
	}}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('OpenRouter');
		await expect(canvasElement.textContent).toContain(
			'Per-caller spend breakdown is not available from this proxy build yet.'
		);
		await expect(canvasElement.textContent).toContain('Not available from this proxy build yet.');
		await expect(canvasElement.textContent).not.toContain('undefined');
		await expect(canvasElement.textContent).not.toContain('NaN');
		await expect(canvasElement.textContent).toContain('$7.72');
		await expect(canvasElement.textContent).toContain('near ceiling');
		await expect(canvasElement.querySelector('[role="alert"]')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<IntegrationsPage {...args} />
		</div>
	{/snippet}
</Story>
