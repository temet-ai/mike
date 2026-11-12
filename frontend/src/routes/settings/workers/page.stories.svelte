<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import WorkersPage from './+page.svelte';
	import { userEvent, waitFor, within } from 'storybook/test';
	import type { FeatureDto, SettingMeta, SyncStatusDto } from '$lib/api/settings';
	import type { OpenRouterUsageDto } from '$lib/api/usage';
	import type { WorkersPageData } from './+page';
	import { ALARMS, BUNDLES } from '$lib/test-support/judgesFixtures';

	const FEATURES: FeatureDto[] = [
		{ key: 'lesson_worker', enabled: true, scope: 'global', applies: 'live_disable' },
		{ key: 'harvester', enabled: true, scope: 'global', applies: 'immediate' },
		{ key: 'retention_sweep', enabled: true, scope: 'global', applies: 'live_disable' },
		{ key: 'codex_diagnostic_capture', enabled: false, scope: 'global', applies: 'immediate' },
		{ key: 'db_resync', enabled: true, scope: 'global', applies: 'on_restart' }
	];
	const featureWrites: { key: string; enabled: boolean }[] = [];

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

	function meta(
		o: Partial<SettingMeta> & Pick<SettingMeta, 'key' | 'kind' | 'value' | 'default'>
	): SettingMeta {
		return {
			group: 'Workers',
			scope: 'global',
			applies: 'on_restart',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const SETTINGS: SettingMeta[] = [
		meta({
			group: 'Steerer',
			key: 'steerer.model',
			kind: 'string',
			default: 'openai/gpt-5.6-luna',
			value: 'openai/gpt-5.6-luna',
			label: 'Default LLM model',
			description:
				"The steerer's model, which the turn judge inherits when it names none of its own.",
			max_len: 256
		}),
		meta({
			key: 'lesson_worker.dedup_threshold',
			kind: 'float',
			default: 0.85,
			value: 0.85,
			label: 'Dedup threshold',
			description: 'Cosine similarity above which two lessons are considered duplicates.',
			min_f64: 0.0,
			max_f64: 1.0
		}),
		meta({
			key: 'lesson_worker.max_lessons_per_project',
			kind: 'int',
			default: 50,
			value: 50,
			label: 'Max lessons per project',
			description: 'Maximum active lessons retained per project.',
			min_i64: 1,
			max_i64: 500
		}),
		meta({
			key: 'lesson_worker.session_consolidation_idle_secs',
			kind: 'int',
			default: 3600,
			value: 3600,
			label: 'Session idle threshold (s)',
			description:
				'Seconds of inactivity before a session is considered eligible for consolidation.',
			min_i64: 60
		}),
		meta({
			key: 'lesson_worker.nightly_consolidation_hour',
			kind: 'int',
			default: 3,
			value: 3,
			label: 'Nightly consolidation hour',
			description: 'Hour of the day (UTC) when full-project consolidation runs.',
			min_i64: 0,
			max_i64: 23
		}),
		meta({
			key: 'lesson_worker.gc_interval_secs',
			kind: 'int',
			default: 86400,
			value: 86400,
			label: 'GC interval (s)',
			description: 'How often expired lessons are garbage-collected.',
			min_i64: 3600
		}),
		meta({
			group: 'Lesson relations',
			key: 'lesson_relations.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Lesson relations: enabled',
			description:
				'Enable the relation detector, which proposes supersedes/refines/recurs/contradicts relations between lessons.'
		}),
		meta({
			group: 'Lesson relations',
			key: 'lesson_relations.proposer_model',
			kind: 'string',
			default: 'openai/gpt-5.6-luna',
			value: 'openai/gpt-5.6-luna',
			label: 'Proposer model',
			description:
				'OpenRouter model that proposes a relation from an evidence bundle. Must differ from the adjudicator model.',
			max_len: 256
		}),
		meta({
			group: 'Lesson relations',
			key: 'lesson_relations.adjudicator_model',
			kind: 'string',
			default: 'google/gemini-3.5-flash-lite',
			value: 'google/gemini-3.5-flash-lite',
			label: 'Adjudicator model',
			description:
				"OpenRouter model that independently reviews the proposer's claim against the same evidence. Must differ from the proposer model.",
			max_len: 256
		}),
		meta({
			group: 'Lesson relations',
			key: 'lesson_relations.candidate_distance_threshold',
			kind: 'float',
			default: 0.65,
			value: 0.65,
			label: 'Candidate distance threshold',
			description:
				'Maximum cosine distance between two lessons for them to be considered candidate pairs.',
			min_f64: 0.0,
			max_f64: 1.0
		})
	];

	const SYNC_STATUS: SyncStatusDto = {
		last_run_at: new Date(Date.now() - 42_000).toISOString(),
		dest_reachable: true,
		last_report: { tables: [['requests.requests', 7]], total_rows: 7 },
		last_error: null
	};

	const UNCLAIMED_ALARM = {
		id: 5,
		kind: 'change_without_gate',
		subject: 'bundle:999',
		first_seen_at: '2026-09-24T06:00:00Z',
		last_seen_at: '2026-09-24T09:00:00Z',
		cleared_at: null,
		evaluations: 12,
		quiet_evaluations: 0,
		detail: {},
		acknowledged_at: null,
		ack_note: null
	};
	const ALARM_LIST = [UNCLAIMED_ALARM, ...ALARMS];

	function buildData(): WorkersPageData {
		return {
			features: FEATURES,
			loadError: null,
			settings: SETTINGS,
			settingsLoadError: null,
			syncStatus: SYNC_STATUS,
			syncStatusError: null,
			alarms: { total: ALARM_LIST.length, limit: 100, offset: 0, alarms: ALARM_LIST },
			alarmsError: null,
			bundles: BUNDLES
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
			const existing = FEATURES.find((f) => f.key === key);
			featureWrites.push({ key, enabled });
			return json({
				key,
				enabled,
				scope: existing?.scope ?? 'global',
				applies: existing?.applies ?? 'on_restart'
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
				meta({ key, kind: 'int', default: 0, value: (value ?? 0) as SettingMeta['value'] })
			);
		}
		if (url.includes('/api/settings/features')) return json({ features: FEATURES });
		if (url.includes('/api/settings'))
			return json({ settings: SETTINGS, steerer_openrouter_configured_via_env: false });
		if (url.includes('/api/sync/status')) return json(SYNC_STATUS);
		if (url.includes('/api/usage/openrouter')) return json(OPENROUTER_USAGE);
		if (url.includes('/api/eval/alarms'))
			return json({ total: ALARM_LIST.length, limit: 100, offset: 0, alarms: ALARM_LIST });
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Workers',
		component: WorkersPage,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' },
		argTypes: { data: { control: false } }
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			featureWrites.length = 0;
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('Workers');
		await expect(canvasElement.querySelectorAll('[role="switch"]').length).toBeGreaterThan(0);
		await expect(canvasElement.textContent).toContain('Local DB resync');
		await expect(canvasElement.textContent).toContain('Remote DB reachable');
		await expect(canvasElement.textContent).toContain('requests.requests');
		await expect(canvasElement.textContent).toMatch(/7\s+rows synced/);

		for (const detail of [
			'Turning it off takes about a minute, with no restart. Turning it back on needs a restart.',
			'Applies at once, on or off, with no restart.',
			'Applies on the next restart.'
		]) {
			await expect(canvasElement.querySelector(`[aria-label="${detail}"]`)).not.toBeNull();
		}

		await expect(canvasElement.textContent).toContain('Needs attention · 1');
		await expect(canvasElement.textContent).toContain(
			'Configuration 999 is in use without its accuracy measured against the accuracy bar.'
		);
		await expect(canvasElement.textContent).not.toContain('finished turns have not been judged');
		await expect(canvasElement.textContent).not.toContain('Configuration 11 changed');
		const pointer = canvasElement.querySelector('[data-testid="judges-pointer"]') as HTMLElement;
		await expect(pointer.textContent?.replace(/\s+/g, ' ')).toMatch(
			/How turns and sessions are labelled is set on the Labelling page under Sessions\./
		);
		await expect([...pointer.querySelectorAll('a')].map((a) => a.getAttribute('href'))).toEqual([
			'/sessions/labelling',
			'/sessions/labelling?for=sessions'
		]);
		await expect(canvasElement.querySelector('button[aria-label^="Enable Turn judge"]')).toBeNull();
		await expect(canvasElement.textContent).not.toMatch(/precision|recall/i);
		await expect(canvasElement.textContent).toContain(
			'OpenRouter API key, used by the lesson relation detector'
		);

		await expect(canvasElement.textContent).toContain('Retention sweep');
		await expect(canvasElement.textContent).toContain('stale sessions and undelivered messages');
		const retentionSwitch = canvasElement.querySelector(
			'button[aria-label="Enable Retention sweep"]'
		);
		await expect(retentionSwitch).not.toBeNull();
		await expect(retentionSwitch?.getAttribute('aria-checked')).toBe('true');

		const captureSwitch = within(canvasElement).getByRole('switch', {
			name: 'Enable Codex diagnostic capture'
		});
		await expect(captureSwitch).not.toBeChecked();
		await expect(canvasElement.textContent).toContain('4 MiB/exchange, 32 MiB/session and 256 MiB');
		await expect(canvasElement.textContent).toContain('normal activity remains available when off');
		await userEvent.click(captureSwitch);
		await waitFor(() => expect(captureSwitch).toBeChecked());
		await expect(featureWrites).toContainEqual({ key: 'codex_diagnostic_capture', enabled: true });
		await expect(canvasElement.textContent).toContain('Complete or refused');
		await expect(canvasElement.textContent).toContain('Existing evidence expires');
		await userEvent.click(captureSwitch);
		await waitFor(() => expect(captureSwitch).not.toBeChecked());
		await expect(featureWrites).toContainEqual({ key: 'codex_diagnostic_capture', enabled: false });

		await expect(canvasElement.textContent).toContain('Relation detector');
		await expect(canvasElement.textContent).toContain('Lesson relations: enabled');
		await expect(canvasElement.textContent).toContain('Proposer model');
		await expect(canvasElement.textContent).toContain('Adjudicator model');
		await expect(canvasElement.textContent).toContain('Candidate distance threshold');
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<WorkersPage {...args} />
		</div>
	{/snippet}
</Story>
