<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent, waitFor } from 'storybook/test';
	import AnthropicIntegrationPanel from './AnthropicIntegrationPanel.svelte';
	import { getAnthropicIntegration } from '$lib/api/integrations';
	import { __resetApiCacheForTests } from '$lib/stores/cache/apiCache.svelte';

	interface CliLoginState {
		id: string;
		state: 'starting' | 'awaiting_code' | 'succeeded' | 'failed' | 'cancelled';
		url?: string;
		message?: string;
	}

	interface ServerState {
		mode: 'claude_code_login' | 'api_key';
		configured: boolean;
		masked_key: string | null;
		cliInstalled: boolean;
		cliLoggedIn: boolean;
		cliEmail: string | null;
		cliOrgName: string | null;
		cliSubscription: string | null;
		cliVersion: string | null;
		cliLogin: CliLoginState | null;
	}

	const CLI_LOGIN_URL = 'https://claude.com/oauth/authorize?code=story-fixture';

	function defaultServer(overrides: Partial<ServerState> = {}): ServerState {
		return {
			mode: 'claude_code_login',
			configured: false,
			masked_key: null,
			cliInstalled: true,
			cliLoggedIn: false,
			cliEmail: null,
			cliOrgName: null,
			cliSubscription: null,
			cliVersion: '2.1.280 (Claude Code)',
			cliLogin: null,
			...overrides
		};
	}

	function readBody(init: RequestInit | undefined): Record<string, unknown> {
		if (!init || typeof init.body !== 'string') return {};
		try {
			return JSON.parse(init.body) as Record<string, unknown>;
		} catch {
			return {};
		}
	}

	function cliStatusBody(server: ServerState) {
		return {
			installed: server.cliInstalled,
			binary: 'claude',
			version: server.cliInstalled ? server.cliVersion : null,
			logged_in: server.cliLoggedIn,
			auth_method: server.cliLoggedIn ? 'claude.ai' : null,
			email: server.cliEmail,
			org_id: server.cliLoggedIn ? 'org-story-fixture' : null,
			org_name: server.cliOrgName,
			subscription_type: server.cliSubscription,
			error: server.cliInstalled ? null : 'claude CLI not found on PATH — install it, or set MIKE_CLAUDE_CLI_BIN'
		};
	}

	function handleCliLoginRoutes(
		url: string,
		init: RequestInit | undefined,
		server: ServerState,
		json: (body: unknown, status?: number) => Response
	): Response | null {
		if (url.endsWith('/cli-login') && init?.method === 'POST') {
			if (server.cliLogin && !['succeeded', 'failed', 'cancelled'].includes(server.cliLogin.state)) {
				return json(
					{ ok: false, login: server.cliLogin, error: 'a sign-in is already in progress' },
					409
				);
			}
			server.cliLogin = { id: 'story-login-1', state: 'awaiting_code', url: CLI_LOGIN_URL };
			return json({ ok: true, login: server.cliLogin });
		}
		const idMatch = /\/cli-login\/([^/]+)(\/(code|cancel))?$/.exec(url);
		if (!idMatch) return null;
		const [, id, , action] = idMatch;
		if (!server.cliLogin || server.cliLogin.id !== id) return json({ error: 'not found' }, 404);
		if (action === 'code' && init?.method === 'POST') {
			const code = readBody(init).code;
			if (typeof code === 'string' && code.trim().length > 0) {
				server.cliLogin = { id, state: 'succeeded' };
				server.cliLoggedIn = true;
				server.cliEmail = 'dawid@example.com';
				server.cliOrgName = "dawid@example.com's Organization";
				server.cliSubscription = 'max';
				return json({}, 200);
			}
			return json({ error: 'invalid code' }, 422);
		}
		if (action === 'cancel' && init?.method === 'POST') {
			server.cliLogin = { id, state: 'cancelled' };
			return json({}, 200);
		}
		if (!action && (!init || init.method === undefined || init.method === 'GET')) {
			return json(server.cliLogin);
		}
		return null;
	}

	function buildFetchStub(server: ServerState): typeof globalThis.fetch {
		return (async (input: RequestInfo | URL, init?: RequestInit) => {
			const url =
				typeof input === 'string'
					? input
					: input instanceof URL
						? input.href
						: (input as Request).url;
			const json = (body: unknown, status = 200) =>
				new Response(JSON.stringify(body), {
					status,
					headers: { 'content-type': 'application/json' }
				});

			if (url.endsWith('/api/settings/assistant/anthropic/key') && init?.method === 'PUT') {
				const key = readBody(init).key;
				if (typeof key === 'string' && key.startsWith('sk-ant-')) {
					server.configured = true;
					server.masked_key = `sk-ant-...${key.slice(-4)}`;
					return json({ valid: true, label: server.masked_key, persisted: false });
				}
				return json(
					{ valid: false, error: 'not a recognised Anthropic key shape', persisted: false },
					422
				);
			}
			if (url.endsWith('/api/settings/assistant/anthropic/key') && init?.method === 'DELETE') {
				server.configured = false;
				server.masked_key = null;
				return json({ cleared: true });
			}
			if (url.endsWith('/api/settings/assistant/anthropic/test') && init?.method === 'POST') {
				const ok = server.mode === 'claude_code_login' || server.configured;
				return json({
					ok,
					message: ok
						? server.mode === 'claude_code_login'
							? 'Claude Code login works — verified a live call.'
							: 'API key works — verified a live call.'
						: 'No key configured to test.'
				});
			}
			if (url.endsWith('/api/settings/assistant.anthropic_auth_mode') && init?.method === 'PATCH') {
				const value = readBody(init).value;
				if (value === 'claude_code_login' || value === 'api_key') server.mode = value;
				return json({
					key: 'assistant.anthropic_auth_mode',
					group: 'assistant',
					kind: 'enum',
					scope: 'global',
					applies: 'immediate',
					default: 'claude_code_login',
					label: 'Anthropic auth mode',
					description: 'Which credentials the Claude Agent SDK-powered assistants use.',
					value: server.mode,
					enum_variants: ['claude_code_login', 'api_key']
				});
			}
			if (url.endsWith('/api/settings/assistant/anthropic/cli-logout') && init?.method === 'POST') {
				server.cliLoggedIn = false;
				server.cliEmail = null;
				server.cliOrgName = null;
				server.cliSubscription = null;
				return json({ ok: true });
			}
			if (url.includes('/api/settings/assistant/anthropic/cli-login')) {
				const handled = handleCliLoginRoutes(url, init, server, json);
				if (handled) return handled;
			}
			if (url.endsWith('/api/settings/assistant/anthropic/cli-status')) {
				return json(cliStatusBody(server));
			}
			if (url.endsWith('/api/settings/assistant/anthropic')) {
				return json({
					mode: server.mode,
					configured: server.configured,
					masked_key: server.masked_key
				});
			}
			return json({ error: `unexpected fetch: ${url}` }, 503);
		}) as typeof globalThis.fetch;
	}

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/AnthropicIntegrationPanel',
		component: AnthropicIntegrationPanel,
		tags: ['autodocs'],
		parameters: {
			layout: 'fullscreen',
			docs: {
				description: {
					component:
						'Anthropic connection-authorization card on /settings/integrations. Mirrors ' +
						'OpenRouterIntegrationPanel structurally (card shell, key-entry form, aria-live ' +
						'result region) but owns its mutations directly — mode switch, key save/clear, ' +
						'test-connection, and the Claude Code CLI sign-in/sign-out lifecycle all call the ' +
						'API and then ask the parent to refetch status via onStatusChanged, rather than ' +
						'threading each field up through props.'
				}
			}
		}
	});
</script>

<script lang="ts">
	import type { AnthropicIntegrationStatus } from '$lib/api/integrations';

	let status = $state<AnthropicIntegrationStatus | null>(null);
	let statusError = $state<string | null>(null);

	async function reload(): Promise<void> {
		try {
			status = await getAnthropicIntegration();
			statusError = null;
		} catch (err) {
			statusError = err instanceof Error ? err.message : String(err);
		}
	}

	// Mirrors the real /settings/integrations page's onMount fetch: runs once the story mounts
	// (after the loader below has already stubbed globalThis.fetch), rather than being awaited
	// from the loader itself — a loader-mutated $state isn't guaranteed to reach the component
	// that ends up rendering the Story's template.
	$effect(() => {
		void reload();
	});
</script>

<Story
	name="Default"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			globalThis.fetch = buildFetchStub(defaultServer());
			status = null;
			statusError = null;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Authorization mode')) {
				throw new Error('status not yet loaded');
			}
		});
		await expect(canvasElement.textContent).toContain('Anthropic');
		await expect(canvasElement.textContent).toContain(
			'Credentials for the Claude Agent SDK-powered assistants'
		);
		await expect(canvasElement.textContent).toContain('Claude Code login on this machine');

		const apiKeyRadio = canvasElement.querySelector('input[value="api_key"]');
		if (!apiKeyRadio) throw new Error('api_key radio not found');
		await userEvent.click(apiKeyRadio);
		await waitFor(() => {
			const checked = canvasElement.querySelector('input[value="api_key"]') as HTMLInputElement;
			if (!checked.checked) throw new Error('mode not yet switched to api_key');
		});
		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Anthropic API key')) {
				throw new Error('key form not yet rendered');
			}
		});

		const keyInput = canvas.getByPlaceholderText('sk-ant-...') as HTMLInputElement;
		await userEvent.type(keyInput, 'sk-ant-abcd1234');
		const applyButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Apply key'
		);
		if (!applyButton) throw new Error('Apply key button not found');
		await userEvent.click(applyButton);

		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Key verified')) {
				throw new Error('key result not yet rendered');
			}
		});
		await waitFor(() => {
			if (!canvasElement.textContent?.includes('...1234')) {
				throw new Error('masked key not yet rendered');
			}
		});

		const testButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Test connection'
		);
		if (!testButton) throw new Error('Test connection button not found');
		await userEvent.click(testButton);
		await waitFor(() => {
			if (!canvasElement.textContent?.includes('API key works')) {
				throw new Error('test result not yet rendered');
			}
		});

		const clearButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Clear'
		);
		if (!clearButton) throw new Error('Clear button not found');
		await userEvent.click(clearButton);
		await waitFor(() => {
			if (canvasElement.textContent?.includes('...1234')) {
				throw new Error('masked key still showing after clear');
			}
		});
	}}
>
	{#snippet template()}
		<div style="max-width:640px; padding:24px; background:var(--surface-bg);">
			<AnthropicIntegrationPanel
				{status}
				{statusError}
				statusUnavailable={false}
				onRetryStatus={() => void reload()}
				onStatusChanged={() => void reload()}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="ClaudeCodeSignedIn"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			globalThis.fetch = buildFetchStub(
				defaultServer({
					cliLoggedIn: true,
					cliEmail: 'dawid@example.com',
					cliOrgName: "dawid@example.com's Organization",
					cliSubscription: 'max'
				})
			);
			status = null;
			statusError = null;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Signed in as')) {
				throw new Error('signed-in status not yet loaded');
			}
		});
		await expect(canvasElement.textContent).toContain('dawid@example.com');
		await expect(canvasElement.textContent).toContain('max');
		const signOutButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Sign out'
		);
		if (!signOutButton) throw new Error('Sign out button not found');
	}}
>
	{#snippet template()}
		<div style="max-width:640px; padding:24px; background:var(--surface-bg);">
			<AnthropicIntegrationPanel
				{status}
				{statusError}
				statusUnavailable={false}
				onRetryStatus={() => void reload()}
				onStatusChanged={() => void reload()}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="ClaudeCodeSignedOut"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			globalThis.fetch = buildFetchStub(defaultServer());
			status = null;
			statusError = null;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Not signed in on this machine')) {
				throw new Error('signed-out status not yet loaded');
			}
		});
		const signInButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Sign in'
		);
		if (!signInButton) throw new Error('Sign in button not found');
	}}
>
	{#snippet template()}
		<div style="max-width:640px; padding:24px; background:var(--surface-bg);">
			<AnthropicIntegrationPanel
				{status}
				{statusError}
				statusUnavailable={false}
				onRetryStatus={() => void reload()}
				onStatusChanged={() => void reload()}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="ClaudeCodeSigningIn"
	loaders={[
		async () => {
			__resetApiCacheForTests();
			globalThis.fetch = buildFetchStub(defaultServer());
			status = null;
			statusError = null;
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Not signed in on this machine')) {
				throw new Error('signed-out status not yet loaded');
			}
		});

		const signInButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Sign in'
		);
		if (!signInButton) throw new Error('Sign in button not found');
		await userEvent.click(signInButton);

		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Open sign-in page')) {
				throw new Error('sign-in URL not yet shown');
			}
		});
		const openLink = canvasElement.querySelector('a') as HTMLAnchorElement | null;
		if (!openLink || openLink.href !== CLI_LOGIN_URL) {
			throw new Error('sign-in link does not carry the captured URL');
		}

		const codeInput = canvasElement.querySelector(
			'input[placeholder="Paste the code here"]'
		) as HTMLInputElement;
		if (!codeInput) throw new Error('code field not yet rendered');
		await userEvent.type(codeInput, '123456');
		const submitButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Submit code'
		);
		if (!submitButton) throw new Error('Submit code button not found');
		await userEvent.click(submitButton);

		await waitFor(
			() => {
				if (!canvasElement.textContent?.includes('Signed in as')) {
					throw new Error('sign-in did not complete');
				}
			},
			{ timeout: 5000 }
		);
	}}
>
	{#snippet template()}
		<div style="max-width:640px; padding:24px; background:var(--surface-bg);">
			<AnthropicIntegrationPanel
				{status}
				{statusError}
				statusUnavailable={false}
				onRetryStatus={() => void reload()}
				onStatusChanged={() => void reload()}
			/>
		</div>
	{/snippet}
</Story>
