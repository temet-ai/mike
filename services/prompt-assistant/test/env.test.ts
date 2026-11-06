import { describe, expect, it } from 'vitest';
import { buildSdkEnv } from '../src/agent/env.js';

describe('buildSdkEnv mutation-exemption header', () => {
	it('sets x-mike-client: prompt-assistant when capture is on', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin' },
			{ capture: true, captureBaseUrl: 'http://127.0.0.1:9988' }
		);
		expect(env.ANTHROPIC_BASE_URL).toBe('http://127.0.0.1:9988');
		expect(env.ANTHROPIC_CUSTOM_HEADERS).toBe('x-mike-client: prompt-assistant');
	});

	it('omits both the base url and the header when capture is off', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988' }
		);
		expect(env.ANTHROPIC_BASE_URL).toBeUndefined();
		expect(env.ANTHROPIC_CUSTOM_HEADERS).toBeUndefined();
	});
});

describe('buildSdkEnv anthropicApiKeyOverride', () => {
	it('overrides an ANTHROPIC_API_KEY already present in process env', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin', ANTHROPIC_API_KEY: 'operator-login-key' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988', anthropicApiKeyOverride: 'sk-override' }
		);
		expect(env.ANTHROPIC_API_KEY).toBe('sk-override');
	});

	it('sets ANTHROPIC_API_KEY from the override even when process env has none', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988', anthropicApiKeyOverride: 'sk-override' }
		);
		expect(env.ANTHROPIC_API_KEY).toBe('sk-override');
	});

	it('falls through to the existing allowlist pass-through when the override is undefined', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin', ANTHROPIC_API_KEY: 'operator-login-key' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988' }
		);
		expect(env.ANTHROPIC_API_KEY).toBe('operator-login-key');
	});

	it('falls through to the existing allowlist pass-through when the override is null', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin', ANTHROPIC_API_KEY: 'operator-login-key' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988', anthropicApiKeyOverride: null }
		);
		expect(env.ANTHROPIC_API_KEY).toBe('operator-login-key');
	});

	it('falls through to the existing allowlist pass-through when the override is an empty string', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin', ANTHROPIC_API_KEY: 'operator-login-key' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988', anthropicApiKeyOverride: '' }
		);
		expect(env.ANTHROPIC_API_KEY).toBe('operator-login-key');
	});

	it('leaves ANTHROPIC_API_KEY unset when neither process env nor the override supply one', () => {
		const env = buildSdkEnv(
			{ PATH: '/usr/bin' },
			{ capture: false, captureBaseUrl: 'http://127.0.0.1:9988' }
		);
		expect(env.ANTHROPIC_API_KEY).toBeUndefined();
	});
});
