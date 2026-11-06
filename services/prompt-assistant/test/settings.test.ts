import { describe, expect, it } from 'vitest';
import { InspectClient } from '../src/inspectClient.js';
import {
	BUILTIN_INSTRUCTIONS,
	SETTINGS_DEFAULTS,
	resolvePromptAssistantSettings
} from '../src/settings.js';
import { stubFetch } from './fakes.js';

describe('resolvePromptAssistantSettings', () => {
	it('falls back to spec defaults when the catalogue has no prompt_assistant.* keys yet', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': {
					settings: [{ key: 'other.thing', kind: 'bool', value: true }]
				},
				'GET /api/settings/features': { features: [] }
			})
		);
		const settings = await resolvePromptAssistantSettings(client);
		expect(settings.model).toBe(SETTINGS_DEFAULTS.model);
		expect(settings.codex_model).toBe('gpt-6-astra');
		expect(settings.provider).toBe('anthropic');
		expect(settings.effort).toBe('medium');
		expect(settings.instructions).toBe(BUILTIN_INSTRUCTIONS);
		expect(settings.read_prompt).toBe(true);
		expect(settings.read_memory).toBe(false);
		expect(settings.may_apply).toBe('suggest');
		expect(settings.max_turns).toBe(8);
		expect(settings.daily_budget_usd).toBe(1.0);
		expect(settings.keep_days).toBe(30);
		// No feature row at all defaults to enabled, matching every other worker toggle in this repo.
		expect(settings.enabled).toBe(true);
	});

	it('overlays stored values and the feature toggle over the defaults', async () => {
		const client = new InspectClient(
			'http://inspect.local',
			stubFetch({
				'GET /api/settings': {
					settings: [
						{ key: 'assistant.provider', kind: 'enum', value: 'codex' },
						{
							key: 'prompt_assistant.model',
							kind: 'string',
							value: 'claude-opus-5'
						},
						{
							key: 'prompt_assistant.codex_model',
							kind: 'string',
							value: 'gpt-5.6-luna'
						},
						{ key: 'prompt_assistant.effort', kind: 'enum', value: 'high' },
						{ key: 'prompt_assistant.may_apply', kind: 'enum', value: 'apply' },
						{
							key: 'prompt_assistant.daily_budget_usd',
							kind: 'float',
							value: 5
						},
						{ key: 'prompt_assistant.read_memory', kind: 'bool', value: true }
					]
				},
				'GET /api/settings/features': {
					features: [
						{
							key: 'prompt_assistant',
							enabled: false,
							scope: 'global',
							applies: 'immediate'
						}
					]
				}
			})
		);
		const settings = await resolvePromptAssistantSettings(client);
		expect(settings.model).toBe('claude-opus-5');
		expect(settings.codex_model).toBe('gpt-5.6-luna');
		expect(settings.provider).toBe('codex');
		expect(settings.effort).toBe('high');
		expect(settings.may_apply).toBe('apply');
		expect(settings.daily_budget_usd).toBe(5);
		expect(settings.read_memory).toBe(true);
		expect(settings.enabled).toBe(false);
		// Untouched keys still fall back to defaults.
		expect(settings.keep_days).toBe(30);
	});

	it('never throws when mike is unreachable — settings and features both fall back', async () => {
		const client = new InspectClient('http://inspect.local', (async () => {
			throw new Error('ECONNREFUSED');
		}) as unknown as typeof fetch);
		const settings = await resolvePromptAssistantSettings(client);
		expect(settings.model).toBe(SETTINGS_DEFAULTS.model);
		expect(settings.enabled).toBe(true);
	});
});
