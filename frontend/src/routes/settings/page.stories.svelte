<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, waitFor } from 'storybook/test';
	import SettingsPage from './+page.svelte';
	import type { CompressionEffective, FeatureDto, SettingMeta } from '$lib/api/settings';
	import type { CapabilityRow } from '$lib/api/capabilities';
	import type { SettingsPageData } from './+page';

	const FEATURES: FeatureDto[] = [
		{ key: 'reminders', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'recall_injector', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'file_read_compression', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'shell_output_compression', enabled: false, scope: 'session', applies: 'new_sessions' },
		{ key: 'agent_lessons', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'proxy_tools', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'weasel_intercept', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'steerer', enabled: true, scope: 'session', applies: 'new_sessions' },
		{ key: 'lesson_worker', enabled: true, scope: 'global', applies: 'on_restart' },
		{ key: 'harvester', enabled: true, scope: 'global', applies: 'on_restart' },
		{ key: 'db_resync', enabled: true, scope: 'global', applies: 'on_restart' },
		{ key: 'dossier_builder', enabled: true, scope: 'global', applies: 'immediate' },
		{ key: 'fleet_auto_start', enabled: true, scope: 'global', applies: 'immediate' }
	];

	function meta(o: Partial<SettingMeta>): SettingMeta {
		return {
			key: 'k',
			group: 'Group',
			kind: 'string',
			scope: 'session',
			applies: 'new_sessions',
			default: '',
			value: '',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const MODE_DESCRIPTION =
		'Forward sends every new session shortened requests, whatever the share. Shadow sends shortened requests only to the share of new sessions set below and records what would be shortened for the rest; shadow with a share of 0 never forwards. Sessions already running keep the policy they started with.';

	const FILE_READ_EFFECTIVE: CompressionEffective = {
		state: 'sampled_forwarding',
		mode: 'shadow',
		forward_share: 20,
		applies: 'new_sessions',
		enabled: true,
		summary:
			'20% of new sessions (by session id) are sent shortened requests; the rest are the shadow control.',
		session_arm:
			"each request's manifest records its session's arm in the file_read_compression stage's resolved_parameters.arm"
	};
	const SHELL_OUTPUT_EFFECTIVE: CompressionEffective = {
		state: 'off',
		mode: 'shadow',
		forward_share: 0,
		applies: 'new_sessions',
		enabled: false,
		summary:
			'Shell-output compression is off: no session is sent shortened requests and nothing is recorded. Mode and share take effect for new sessions once it is turned on.',
		session_arm:
			"each request's manifest records its session's arm in the shell_output_compression stage's resolved_parameters.arm"
	};

	const SETTINGS: SettingMeta[] = [
		meta({
			key: 'file_read_compression.mode',
			group: 'File read compression',
			kind: 'enum',
			default: 'shadow',
			value: 'shadow',
			enum_variants: ['shadow', 'forward'],
			label: 'Mode',
			description: MODE_DESCRIPTION
		}),
		meta({
			key: 'file_read_compression.forward_share',
			group: 'File read compression',
			kind: 'int',
			default: 0,
			value: 20,
			min_i64: 0,
			max_i64: 100,
			label: 'Share of sessions forwarded',
			description:
				"In shadow mode, the percent of new sessions, chosen by session id, that are sent shortened requests; the rest are the control. Forward mode ignores it. Sessions already running keep their arm, which each request's manifest records. The compressor scorecard adjusts it."
		}),
		meta({
			key: 'shell_output_compression.mode',
			group: 'Shell-output compression',
			kind: 'enum',
			default: 'shadow',
			value: 'shadow',
			enum_variants: ['shadow', 'forward'],
			label: 'Mode',
			description: MODE_DESCRIPTION
		}),
		meta({
			key: 'shell_output_compression.forward_share',
			group: 'Shell-output compression',
			kind: 'int',
			default: 0,
			value: 0,
			min_i64: 0,
			max_i64: 100,
			label: 'Share of sessions forwarded',
			description:
				"In shadow mode, the percent of new sessions, chosen by session id, that are sent shortened Bash output; the rest are the control. Forward mode ignores it. Each request's manifest records its session's arm."
		}),
		meta({
			key: 'analysis.enabled',
			group: 'Analysis',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Enable analysis',
			description: 'Run weasel-pattern analysis across captured responses.'
		}),
		meta({
			key: 'reminders.max_lessons_to_inject',
			group: 'Reminders',
			kind: 'int',
			default: 5,
			value: 5,
			min_i64: 1,
			max_i64: 20,
			label: 'Max lessons',
			description: 'Maximum project lessons injected per turn.'
		}),
		meta({
			key: 'steerer.model',
			group: 'Steerer',
			kind: 'string',
			scope: 'global',
			applies: 'on_restart',
			default: 'openai/gpt-5.6-luna',
			value: 'openai/gpt-5.6-luna',
			max_len: 256,
			label: 'LLM model',
			description: 'OpenRouter model used for steerer verdict calls.'
		}),
		meta({
			key: 'log_level',
			group: 'Logging',
			kind: 'enum',
			default: 'info',
			value: 'info',
			enum_variants: ['error', 'warn', 'info', 'debug'],
			label: 'Log level',
			description: 'Proxy log verbosity.'
		}),
		meta({
			key: 'judge_worker.interval_secs',
			group: 'Turn judge',
			kind: 'int',
			scope: 'global',
			applies: 'next_cycle',
			default: 60,
			value: 60,
			min_i64: 10,
			max_i64: 86400,
			label: 'Seconds between sweeps',
			description: 'How long the turn judge waits between sweeps.'
		}),
		meta({
			key: 'session_judge.poll_interval_secs',
			group: 'Session judge',
			kind: 'int',
			scope: 'global',
			applies: 'next_cycle',
			default: 120,
			value: 120,
			min_i64: 10,
			max_i64: 86400,
			label: 'Seconds between polls',
			description: 'How long the session judge waits between polls.'
		})
	];

	const CAPABILITIES: CapabilityRow[] = [
		{
			id: 'steering',
			label: 'Steering & judges',
			state: 'inert',
			detail:
				'Steering is enabled but no OpenRouter API key is configured — every evaluation fails fast and skips',
			fix: 'Set MIKE_STEERER_OPENROUTER_API_KEY (or OPENROUTER_API_KEY)'
		},
		{
			id: 'semantic_recall',
			label: 'Semantic recall',
			state: 'active',
			detail: 'Semantic recall active against the embedding server at http://127.0.0.1:7997',
			fix: ''
		},
		{
			id: 'weasel_intercept',
			label: 'Weasel intercept',
			state: 'active',
			detail: 'Weasel intercept active — scanning responses against 42 forbidden words',
			fix: ''
		},
		{
			id: 'persistence',
			label: 'Persistence',
			state: 'active',
			detail: 'Persistence active — writing captures to Postgres',
			fix: ''
		}
	];

	function buildData(): SettingsPageData {
		return {
			features: FEATURES,
			loadError: null,
			settings: SETTINGS,
			fileReadCompressionEffective: FILE_READ_EFFECTIVE,
			shellOutputCompressionEffective: SHELL_OUTPUT_EFFECTIVE,
			steererOpenrouterConfiguredViaEnv: false,
			settingsLoadError: null,
			capabilities: CAPABILITIES,
			capabilitiesLoadError: null
		};
	}

	const patchCalls: { url: string; method: string; body: string }[] = [];

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
			patchCalls.push({
				url,
				method: init?.method ?? 'GET',
				body: typeof init?.body === 'string' ? init.body : ''
			});
			const key = url.split('/').pop() ?? 'x';
			let enabled = true;
			if (init && typeof init.body === 'string') {
				try {
					enabled = JSON.parse(init.body).enabled ?? true;
				} catch {}
			}
			return json({ key, enabled, scope: 'session', applies: 'new_sessions' });
		}
		if (url.includes('/api/settings/steerer/openrouter/verify'))
			return json({
				env_var_name: 'MIKE_STEERER_OPENROUTER_API_KEY',
				configured: false,
				valid: false
			});
		if (url.includes('/api/settings/') && !url.includes('/features')) {
			const key = decodeURIComponent(url.split('/api/settings/').pop() ?? 'x');
			let value: unknown;
			if (init && typeof init.body === 'string') {
				try {
					value = JSON.parse(init.body).value;
				} catch {}
			}
			return json(meta({ key, value: (value ?? '') as SettingMeta['value'] }));
		}
		if (url.includes('/api/settings/features')) return json({ features: FEATURES });
		if (url.includes('/api/settings'))
			return json({
				settings: SETTINGS,
				steerer_openrouter_configured_via_env: false,
				file_read_compression_effective: FILE_READ_EFFECTIVE,
				shell_output_compression_effective: SHELL_OUTPUT_EFFECTIVE
			});
		if (url.includes('/api/steering/models')) return json({ models: [] });
		if (url.includes('/api/sync/status'))
			return json({
				last_run_at: null,
				dest_reachable: false,
				last_report: { tables: [], total_rows: 0 },
				last_error: null
			});
		if (url.includes('/api/capabilities')) return json({ capabilities: CAPABILITIES });
		return Promise.resolve(
			new Response(JSON.stringify({ error: 'unexpected fetch' }), {
				status: 503,
				headers: { 'content-type': 'application/json' }
			})
		);
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/Page',
		component: SettingsPage,
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
		await expect(canvasElement.textContent).toContain('General settings');
		await expect(canvasElement.querySelectorAll('[role="switch"]').length).toBeGreaterThan(0);
		await expect(canvasElement.querySelector('#reminders')).not.toBeNull();
		await expect(canvasElement.querySelector('#logging')).not.toBeNull();
		await expect(canvasElement.querySelector('[role="switch"][aria-label*="steerer"]')).toBeNull();
		await expect(canvasElement.querySelector('#steerer')).toBeNull();
		await expect(canvasElement.querySelector('#analysis')).toBeNull();
		await expect(canvasElement.querySelector('#turn-judge')).toBeNull();
		await expect(canvasElement.querySelector('#session-judge')).toBeNull();
		await expect(canvasElement.textContent).toContain('Agent lessons');
		await expect(canvasElement.textContent).toContain("that sub-agent's prompts");
		await expect(canvasElement.querySelector('[data-testid="capability-panel"]')).not.toBeNull();
		await expect(canvasElement.textContent).toContain('Capability status');
		await expect(canvasElement.textContent).toContain('no OpenRouter API key is configured');
		const shell = canvasElement.querySelector('#shell-output-compression');
		await expect(shell).not.toBeNull();
		await expect(shell?.textContent).toContain('Off');
		await expect(shell?.textContent).toContain('Shell-output compression is off');
		await expect(
			canvasElement.querySelector('#file-read-compression')?.textContent
		).toContain('Sampled forwarding: 20% of new sessions');
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<SettingsPage {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Global settings"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			patchCalls.length = 0;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		const section = canvasElement.querySelector('#global-settings');
		await expect(section).not.toBeNull();
		await expect(section?.textContent).toContain('Global settings');
		await expect(section?.textContent).toContain('Start fleets on their own');
		await expect(section?.textContent).toContain(
			'starts a fleet as soon as its description is complete'
		);
		await expect(section?.textContent).toContain('mike fleet start');
		const toggle = section?.querySelector('[role="switch"][aria-label^="fleet_auto_start"]');
		await expect(toggle).not.toBeNull();
		await expect(toggle?.getAttribute('aria-checked')).toBe('true');
		await userEvent.click(toggle as Element);
		await waitFor(() => expect(patchCalls.length).toBe(1));
		await expect(patchCalls[0]!.url).toContain('/api/settings/features/fleet_auto_start');
		await expect(patchCalls[0]!.method).toBe('PATCH');
		await expect(JSON.parse(patchCalls[0]!.body).enabled).toBe(false);
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<SettingsPage {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Opened from search"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: { ...buildData(), find: 'Mike tools' } }}
	play={async ({ canvasElement }) => {
		const find = canvasElement.querySelector(
			'input[placeholder="Find a setting"]'
		) as HTMLInputElement;
		await expect(find.value).toBe('Mike tools');
		const toggle = canvasElement.querySelector('[role="switch"][aria-label^="proxy_tools"]');
		await expect(toggle).not.toBeNull();
		await expect(canvasElement.querySelector('#reminders')).toBeNull();
		await userEvent.clear(find);
		await waitFor(() => expect(canvasElement.querySelector('#reminders')).not.toBeNull());
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<SettingsPage {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Search finds switches"
	loaders={[
		async () => {
			globalThis.fetch = fetchStub as typeof globalThis.fetch;
			return {};
		}
	]}
	args={{ data: buildData() }}
	play={async ({ canvasElement }) => {
		const find = canvasElement.querySelector('input[placeholder="Find a setting"]');
		await expect(find).not.toBeNull();
		const firstSection = canvasElement.querySelector('section');
		await expect(
			find!.compareDocumentPosition(firstSection as Node) & Node.DOCUMENT_POSITION_FOLLOWING
		).toBeTruthy();

		await userEvent.type(find as Element, 'proxy_tools');
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-testid="capability-panel"]')).toBeNull()
		);
		await expect(canvasElement.textContent).toContain('Mike tools in every session');
		await expect(canvasElement.textContent).not.toContain('Agent lessons');
		await expect(canvasElement.querySelector('.cfg-find-count')?.textContent).toContain('1 match');

		await userEvent.clear(find as Element);
		await userEvent.type(find as Element, 'correction');
		const flagging = await waitFor(() => {
			const section = canvasElement.querySelector('#other-page-flagging');
			if (!section) throw new Error('no Flagging result yet');
			return section;
		});
		await expect(flagging.textContent).toContain('Live correction');
		await expect(flagging.querySelector('a[href$="/settings/flagging"]')).not.toBeNull();
		await expect(
			flagging.querySelector('[role="switch"][aria-label^="weasel_intercept"]')
		).not.toBeNull();

		await userEvent.clear(find as Element);
		await userEvent.type(find as Element, 'forbidden');
		await waitFor(() =>
			expect(canvasElement.querySelector('.cfg-find-count')?.textContent).toContain('1 match')
		);
		await expect(canvasElement.textContent).toContain('Live correction');

		await userEvent.clear(find as Element);
		await userEvent.type(find as Element, 'no-such-setting');
		await waitFor(() => expect(canvasElement.textContent).toContain('No setting matches'));

		await userEvent.clear(find as Element);
		await waitFor(() =>
			expect(canvasElement.querySelector('[data-testid="capability-panel"]')).not.toBeNull()
		);
		await expect(canvasElement.querySelector('#other-page-flagging')).toBeNull();
	}}
>
	{#snippet template(args)}
		<div style="width:100%; min-height:100vh; background:var(--surface-bg);">
			<SettingsPage {...args} />
		</div>
	{/snippet}
</Story>
