import type { FeatureDto, SettingMeta, SettingValue } from '$lib/api/settings';
import type { FeaturePreset, LlmProvidersResponse, SetupCheck, SetupState } from '$lib/api/setup';

const SWITCHES = [
	'recall_injector',
	'agent_lessons',
	'reminders',
	'steerer',
	'judge_worker',
	'session_judge',
	'judge_lessons',
	'turn_guard',
	'weasel_intercept'
] as const;

const RECOMMENDED = ['recall_injector', 'agent_lessons', 'reminders'];

export function buildFeatures(on: readonly string[] = []): FeatureDto[] {
	return SWITCHES.map((key) => ({
		key,
		enabled: on.includes(key),
		scope: 'session',
		applies: 'new_sessions'
	}));
}

export function buildPresets(): FeaturePreset[] {
	const make = (id: FeaturePreset['id'], on: (key: string) => boolean): FeaturePreset => ({
		id,
		toggles: SWITCHES.map((key) => ({ key, enabled: on(key) })),
		lessons_setting: 'reminders.inject_lessons',
		session_lessons: id !== 'watch-only'
	});
	return [
		make('watch-only', () => false),
		make('recommended', (key) => RECOMMENDED.includes(key)),
		make('everything', () => true)
	];
}

export function buildChecks(): SetupCheck[] {
	const check = (
		id: string,
		name: string,
		status: SetupCheck['status'],
		detail: string,
		step: string | null = null
	): SetupCheck => ({
		id,
		name,
		status,
		detail,
		setup_step: step,
		fix: step === null ? null : `mike setup ${step}`
	});
	return [
		check(
			'config_file',
			'config file',
			'ok',
			'/home/ada/.config/mike/config.toml holds 6 settings'
		),
		check('version', 'mike version', 'ok', 'mike 0.1.0 (abc1234)'),
		check('database_url', 'database url', 'ok', 'postgres://mike@127.0.0.1:5440/claude_proxy'),
		check('database_reachable', 'database connection', 'ok', 'connected with a read-only session'),
		check('postgres_version', 'postgres version', 'ok', 'PostgreSQL 19rc1'),
		check('database_migrations', 'database migrations', 'ok', 'every migration is applied'),
		check('inspect_port', 'inspect port', 'ok', '127.0.0.1:9989 answers'),
		check(
			'background_service',
			'background service',
			'warn',
			'mike is not installed as a background service',
			'service'
		),
		check('claude', 'claude', 'ok', '/usr/local/bin/claude, 2.1.294'),
		check(
			'mcp_registered',
			'mike tools in Claude Code',
			'ok',
			'registered with /usr/local/bin/mike-mcp'
		),
		check(
			'claude_route',
			'claude route',
			'warn',
			'no route is chosen for starting Claude Code',
			'claude'
		),
		check('models', 'background models', 'ok', 'claude, codex'),
		check(
			'embeddings',
			'search by meaning',
			'warn',
			'search by meaning is off, so recall uses text matching',
			'embeddings'
		),
		check('launch_backend', 'terminal backend', 'ok', 'herdr')
	];
}

export function buildSetupState(): SetupState {
	return {
		doctor: {
			version: 'mike 0.1.0 (abc1234)',
			passed: true,
			ok: 11,
			warn: 3,
			fail: 0,
			checks: buildChecks()
		},
		claude_code: { route: null, function_name: 'mcc', auto_trust: null, mods_in_place: false },
		feature_presets: buildPresets()
	};
}

export function buildProviders(): LlmProvidersResponse {
	return {
		llm_provider: 'per_role',
		providers: [
			{
				id: 'claude',
				label: 'Claude',
				prefix: 'claude/',
				available: true,
				detail: 'claude (2.1.294) at /usr/local/bin/claude is logged in',
				fix: '',
				key_env_var: null,
				models: [
					{ id: 'haiku', ref: 'claude/haiku', label: 'haiku' },
					{ id: 'sonnet', ref: 'claude/sonnet', label: 'sonnet' },
					{ id: 'opus', ref: 'claude/opus', label: 'opus' }
				],
				models_error: null
			},
			{
				id: 'codex',
				label: 'Codex',
				prefix: 'codex/',
				available: true,
				detail: 'Logged in with ChatGPT',
				fix: '',
				key_env_var: null,
				models: [
					{ id: 'gpt-6-luna', ref: 'codex/gpt-6-luna', label: 'gpt-6-luna' },
					{ id: 'gpt-6-sol', ref: 'codex/gpt-6-sol', label: 'gpt-6-sol' }
				],
				models_error: null
			},
			{
				id: 'openrouter',
				label: 'OpenRouter',
				prefix: 'openrouter/',
				available: false,
				detail: 'No OpenRouter API key is configured.',
				fix: 'Set MIKE_STEERER_OPENROUTER_API_KEY in the environment of the background service.',
				key_env_var: 'MIKE_STEERER_OPENROUTER_API_KEY',
				models: [],
				models_error: null
			}
		]
	};
}

export function meta(overrides: Partial<SettingMeta> & { key: string }): SettingMeta {
	return {
		group: 'Group',
		kind: 'string',
		scope: 'global',
		applies: 'next_cycle',
		default: '',
		value: '',
		label: overrides.key,
		description: 'Description.',
		...overrides
	};
}

function text(key: string, value: SettingValue, label: string): SettingMeta {
	return meta({ key, value, default: value, label, description: `${label}.` });
}

export function buildSettings(): SettingMeta[] {
	return [
		meta({
			key: 'llm.provider',
			kind: 'enum',
			enum_variants: ['claude', 'codex', 'openrouter', 'per_role'],
			value: 'per_role',
			default: 'per_role',
			label: 'Provider for background jobs',
			description: 'Which provider the background jobs use.'
		}),
		text('steerer.model', 'claude/sonnet', 'Steerer model'),
		text('judge_worker.model', 'codex/gpt-6-sol', 'Turn judge model'),
		meta({
			key: 'llm.daily_spend_ceiling_usd',
			kind: 'float',
			value: 5,
			default: 0,
			min_f64: 0,
			max_f64: 1000,
			label: 'Daily spend limit in US dollars',
			description: 'OpenRouter calls only. 0 means no limit.'
		}),
		meta({
			key: 'llm.ceiling_max_calls',
			kind: 'int',
			value: 100,
			default: 100,
			min_i64: 1,
			max_i64: 100000,
			label: 'Most calls in the rolling window',
			description: 'OpenRouter calls only.'
		}),
		meta({
			key: 'llm.rate_limit_per_minute',
			kind: 'int',
			value: 12,
			default: 12,
			min_i64: 1,
			max_i64: 600,
			label: 'Most calls in one minute',
			description: 'OpenRouter calls only.'
		}),
		meta({
			key: 'reminders.inject_lessons',
			kind: 'bool',
			value: false,
			default: false,
			label: 'Lessons in reminders',
			description: 'Add earlier lessons to reminders.'
		}),
		meta({
			key: 'embedding.enabled',
			kind: 'bool',
			value: false,
			default: false,
			label: 'Search by meaning',
			description: 'Find similar earlier turns by meaning.'
		}),
		text('embedding.url', 'http://127.0.0.1:11434/v1', 'Embedding URL'),
		text('embedding.model', 'nomic-embed-text', 'Embedding model'),
		meta({
			key: 'launch_backend',
			kind: 'enum',
			enum_variants: ['none', 'herdr', 'tmux'],
			value: 'herdr',
			default: 'none',
			label: 'Terminal backend',
			description: 'Where mike opens sessions it starts.'
		})
	];
}
