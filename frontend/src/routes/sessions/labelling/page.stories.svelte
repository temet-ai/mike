<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { VOCABULARY } from '$lib/test-support/evalReviewFixtures';
	import { expect, userEvent, waitFor, within } from 'storybook/test';
	import TurnPage from './+page.svelte';
	import type { JudgePageData } from '$lib/components/judges/judgePageData';
	import type { OpenRouterUsageDto } from '$lib/api/usage';
	import {
		ALARMS,
		BUNDLES,
		GATES,
		JUDGE_FEATURES,
		JUDGE_SETTINGS,
		REJUDGE_RUNNING,
		TURN_RUNS,
		WORDING_BANNED,
		textOutsideCode
	} from '$lib/test-support/judgesFixtures';

	const OPENROUTER_USAGE: OpenRouterUsageDto = {
		configured: true,
		label: 'sk-or-v1-4d2...d99',
		limit: null,
		limit_remaining: null,
		limit_reset: null,
		usage_total: 12.5,
		usage_daily: 0.4,
		usage_weekly: 2.1,
		usage_monthly: 6.3,
		credits_total: 50,
		credits_used: 12.5,
		credits_remaining: 37.5,
		is_free_tier: false
	};

	const LLM_SPEND = {
		window: { since: null, until: null, defaulted: true },
		today: {
			spent_usd: 0.1,
			ceiling_usd: 2.5,
			ceiling_enabled: true,
			alert_fraction: 0.8,
			alert_threshold_usd: 2,
			over_alert_threshold: false,
			over_ceiling: false
		},
		totals: { call_count: 21, cost_usd: 0.62, prompt_tokens: 0, completion_tokens: 0, reasoning_tokens: 0 },
		by_day: [],
		by_model: [],
		by_role: [
			{ key: 'judge_worker', call_count: 18, cost_usd: 0.5, prompt_tokens: 0, completion_tokens: 0, reasoning_tokens: 0 },
			{ key: 'session_judge', call_count: 3, cost_usd: 0.12, prompt_tokens: 0, completion_tokens: 0, reasoning_tokens: 0 },
			{ key: 'steerer', call_count: 40, cost_usd: 1.4, prompt_tokens: 0, completion_tokens: 0, reasoning_tokens: 0 }
		]
	};

	const DATA: JudgePageData = {
		role: 'turn_judge',
		features: JUDGE_FEATURES,
		settings: JUDGE_SETTINGS,
		bundles: BUNDLES,
		runs: TURN_RUNS,
		gates: GATES,
		alarms: { total: ALARMS.length, limit: 100, offset: 0, alarms: ALARMS },
		rejudgeRuns: [REJUDGE_RUNNING],
		counts: {
			reaction: { total: 40, reviewed: 0 },
			delivery: { total: 38, reviewed: 0 }
		},
		vocabulary: VOCABULARY,
		errors: [],
		editId: null
	};

	function json(body: unknown) {
		return Promise.resolve(
			new Response(JSON.stringify(body), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	function fetchStub(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;

		if (url.includes('/api/usage/openrouter')) return json(OPENROUTER_USAGE);
		if (url.includes('/api/llm/spend')) return json(LLM_SPEND);

		if (url.includes('/api/settings/features/')) {
			const key = url.split('/').pop() ?? 'x';
			let enabled = true;
			if (init && typeof init.body === 'string') {
				try {
					enabled = JSON.parse(init.body).enabled ?? true;
				} catch {}
			}
			const existing = JUDGE_FEATURES.find((f) => f.key === key);
			return json({
				key,
				enabled,
				scope: existing?.scope ?? 'global',
				applies: existing?.applies ?? 'on_restart'
			});
		}
		if (url.includes('/api/settings/features')) return json({ features: JUDGE_FEATURES });

		if (url.includes('/api/settings/') && !url.includes('/features')) {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? 'x');
			let value: unknown;
			if (init && typeof init.body === 'string') {
				try {
					value = JSON.parse(init.body).value;
				} catch {}
			}
			const existing = JUDGE_SETTINGS.find((s) => s.key === key);
			if (existing) return json({ ...existing, value: value ?? existing.value });
			return json({ key, group: key, kind: 'int', scope: 'global', applies: 'on_restart', value });
		}
		if (url.includes('/api/settings'))
			return json({ settings: JUDGE_SETTINGS, steerer_openrouter_configured_via_env: false });

		if (url.includes('/api/eval/alarms'))
			return json({ total: ALARMS.length, limit: 100, offset: 0, alarms: ALARMS });

		if (url.includes('/api/eval/runs/') && url.includes('/items'))
			return json({ items: [], total: 40, reviewed: 0, limit: 1, offset: 0 });

		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Sessions/Labelling',
		component: TurnPage,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen'
		},
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
	args={{ data: DATA }}
	play={async ({ canvasElement }) => {
		const c = within(canvasElement);

		await expect(c.getByRole('heading', { level: 1, name: 'Judge' })).toBeInTheDocument();
		await expect(c.getByRole('tab', { name: 'Turns' })).toHaveAttribute('data-state', 'active');
		await expect(c.getByRole('tab', { name: 'Sessions' })).toBeInTheDocument();
		await expect(c.getByRole('link', { name: 'What the judge did' })).toHaveAttribute(
			'href',
			'/sessions/improvement'
		);
		const strip = c.getByTestId('judge-strip');
		const tabs = c.getByRole('tablist', { name: 'What is labelled' });
		await expect(
			tabs.compareDocumentPosition(strip) & Node.DOCUMENT_POSITION_FOLLOWING
		).toBeTruthy();
		await expect(within(strip).getByTestId('strip-config').textContent?.trim()).toBe('14');
		await waitFor(() =>
			expect(within(strip).getByTestId('strip-cost-week').textContent?.trim()).toBe('$0.62')
		);
		const how = c.getByRole('button', { name: 'How the judge works' });
		await expect(how).toHaveAttribute('aria-expanded', 'false');
		await userEvent.click(how);
		await expect(canvasElement.textContent).toContain('Turns are labelled by the turn judge');
		await expect(c.getByRole('link', { name: 'Review labels' }).getAttribute('href')).toBe(
			'/sessions/labelling/accuracy/6/review'
		);
		await waitFor(() => expect(c.getByRole('region', { name: 'In use now' })).toBeInTheDocument());
		await expect(c.getByTestId('in-use-config').textContent?.trim()).toBe('14');

		await expect(textOutsideCode(canvasElement)).not.toMatch(WORDING_BANNED);
	}}
/>
