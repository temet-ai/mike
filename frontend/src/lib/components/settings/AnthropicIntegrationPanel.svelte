<script lang="ts">
	import {
		getAnthropicIntegration,
		setAnthropicKey,
		clearAnthropicKey,
		testAnthropicConnection,
		getAnthropicCliStatus,
		startAnthropicCliLogin,
		getAnthropicCliLogin,
		submitAnthropicCliLoginCode,
		cancelAnthropicCliLogin,
		logoutAnthropicCli
	} from '$lib/api/integrations';
	import type {
		AnthropicIntegrationStatus,
		AnthropicCliStatus,
		AnthropicCliLoginStatus
	} from '$lib/api/integrations';
	import { patchSetting } from '$lib/api/settings';
	import { ApiError } from '$lib/api';
	import { anthropicMarkSvg } from '$lib/icons/vendorMarks';
	import { onDestroy } from 'svelte';

	interface Props {
		status: AnthropicIntegrationStatus | null;
		statusError: string | null;
		statusUnavailable: boolean;
		onRetryStatus?: () => void;
		onStatusChanged?: () => void;
	}

	const { status, statusError, statusUnavailable, onRetryStatus, onStatusChanged }: Props =
		$props();

	interface ModeOption {
		value: AnthropicIntegrationStatus['mode'];
		label: string;
	}

	const MODE_OPTIONS: ModeOption[] = [
		{ value: 'claude_code_login', label: 'Claude Code login on this machine' },
		{ value: 'api_key', label: 'API key' }
	];

	let modePending = $state(false);
	let modeError = $state<string | null>(null);

	let keyInput = $state('');
	let applying = $state(false);
	let keyResult = $state<{ ok: boolean; message: string } | null>(null);

	let clearing = $state(false);
	let clearError = $state<string | null>(null);

	let cliStatus = $state<AnthropicCliStatus | null>(null);
	let cliStatusError = $state<string | null>(null);
	let cliStatusLoading = $state(false);
	let cliLoadedForMode = false;

	let cliLogin = $state<AnthropicCliLoginStatus | null>(null);
	let cliLoginStarting = $state(false);
	let cliLoginError = $state<string | null>(null);
	let cliLoginFallback = $state<string | null>(null);
	let cliCode = $state('');
	let cliSubmittingCode = $state(false);
	let cliPollHandle: ReturnType<typeof setTimeout> | null = null;

	let cliSigningOut = $state(false);
	let cliSignOutConfirming = $state(false);
	let cliSignOutError = $state<string | null>(null);

	async function loadCliStatus(): Promise<void> {
		cliStatusLoading = true;
		cliStatusError = null;
		try {
			cliStatus = await getAnthropicCliStatus();
		} catch (err) {
			cliStatusError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		} finally {
			cliStatusLoading = false;
		}
	}

	function stopCliLoginPolling(): void {
		if (cliPollHandle !== null) {
			clearTimeout(cliPollHandle);
			cliPollHandle = null;
		}
	}

	function scheduleCliLoginPoll(id: string): void {
		stopCliLoginPolling();
		cliPollHandle = setTimeout(() => void pollCliLogin(id), 1500);
	}

	async function pollCliLogin(id: string): Promise<void> {
		try {
			const login = await getAnthropicCliLogin(id);
			cliLogin = login;
			if (login === null) return;
			if (login.state === 'succeeded' || login.state === 'cancelled') {
				await loadCliStatus();
				return;
			}
			if (login.state === 'failed') {
				cliLoginError = login.message ?? 'sign-in failed.';
				return;
			}
			scheduleCliLoginPoll(id);
		} catch (err) {
			cliLoginError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		}
	}

	async function startCliLogin(): Promise<void> {
		if (cliLoginStarting) return;
		cliLoginStarting = true;
		cliLoginError = null;
		cliLoginFallback = null;
		cliCode = '';
		try {
			const result = await startAnthropicCliLogin();
			if (result.ok && result.login) {
				cliLogin = result.login;
				scheduleCliLoginPoll(result.login.id);
			} else {
				cliLogin = result.login ?? null;
				cliLoginError = result.error ?? 'could not start sign-in.';
				cliLoginFallback = result.fallback_command ?? null;
			}
		} catch (err) {
			cliLoginError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		} finally {
			cliLoginStarting = false;
		}
	}

	async function submitCliCode(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		if (cliLogin === null || cliSubmittingCode || cliCode.trim().length === 0) return;
		cliSubmittingCode = true;
		try {
			const ok = await submitAnthropicCliLoginCode(cliLogin.id, cliCode.trim());
			if (ok) {
				cliCode = '';
				scheduleCliLoginPoll(cliLogin.id);
			} else {
				cliLoginError = 'could not send that code to the sign-in process.';
			}
		} finally {
			cliSubmittingCode = false;
		}
	}

	async function cancelCliLogin(): Promise<void> {
		if (cliLogin === null) return;
		stopCliLoginPolling();
		await cancelAnthropicCliLogin(cliLogin.id);
		cliLogin = null;
		cliCode = '';
	}

	function dismissCliLogin(): void {
		stopCliLoginPolling();
		cliLogin = null;
		cliLoginError = null;
		cliLoginFallback = null;
		cliCode = '';
	}

	async function confirmCliSignOut(): Promise<void> {
		if (cliSigningOut) return;
		cliSigningOut = true;
		cliSignOutError = null;
		try {
			const result = await logoutAnthropicCli();
			if (!result.ok) {
				cliSignOutError = result.error ?? 'sign-out did not succeed.';
			}
			await loadCliStatus();
		} catch (err) {
			cliSignOutError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		} finally {
			cliSigningOut = false;
			cliSignOutConfirming = false;
		}
	}

	$effect(() => {
		if (status?.mode === 'claude_code_login' && !cliLoadedForMode) {
			cliLoadedForMode = true;
			void loadCliStatus();
		}
	});

	onDestroy(() => {
		stopCliLoginPolling();
	});

	let testing = $state(false);
	let testResult = $state<{ ok: boolean; message: string } | null>(null);

	async function selectMode(mode: AnthropicIntegrationStatus['mode']): Promise<void> {
		if (status === null || status.mode === mode || modePending) return;
		modePending = true;
		modeError = null;
		try {
			await patchSetting('assistant.anthropic_auth_mode', mode);
			onStatusChanged?.();
		} catch (err) {
			modeError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		} finally {
			modePending = false;
		}
	}

	async function applyKey(event: SubmitEvent): Promise<void> {
		event.preventDefault();
		const key = keyInput.trim();
		if (key.length === 0 || applying) return;
		applying = true;
		try {
			const result = await setAnthropicKey(key);
			if (result.valid) {
				keyResult = {
					ok: true,
					message: result.label ? `Key verified — now using ${result.label}.` : 'Key verified.'
				};
				keyInput = '';
				onStatusChanged?.();
			} else {
				keyResult = {
					ok: false,
					message: result.error
						? `Not applied — ${result.error}`
						: 'Not applied — this key could not be verified.'
				};
			}
		} catch (err) {
			keyResult = {
				ok: false,
				message: `Not applied — ${err instanceof Error ? err.message : 'could not reach the server.'}`
			};
		} finally {
			applying = false;
		}
	}

	async function clearKey(): Promise<void> {
		if (clearing) return;
		clearing = true;
		clearError = null;
		try {
			await clearAnthropicKey();
			keyResult = null;
			onStatusChanged?.();
		} catch (err) {
			clearError =
				err instanceof ApiError ? err.message : err instanceof Error ? err.message : String(err);
		} finally {
			clearing = false;
		}
	}

	async function testConnection(): Promise<void> {
		if (testing) return;
		testing = true;
		testResult = null;
		try {
			const result = await testAnthropicConnection();
			testResult = { ok: result.ok, message: result.message };
		} catch (err) {
			testResult = {
				ok: false,
				message: `Could not test connection — ${err instanceof Error ? err.message : 'could not reach the server.'}`
			};
		} finally {
			testing = false;
		}
	}
</script>

<section id="anthropic" class="ani-section" aria-labelledby="ani-title">
	<div class="ani-card" data-augmented-ui="tl-clip br-clip border">
		<div class="ani-card-header">
			<span class="ani-icon" aria-hidden="true">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- anthropicMarkSvg is a hand-authored constant, not user input. -->
				{@html anthropicMarkSvg(15)}
			</span>
			<h2 class="ani-title" id="ani-title">Anthropic</h2>
		</div>
		<p class="ani-desc">
			Credentials for the Claude Agent SDK-powered assistants — currently just the prompt assistant.
		</p>

		{#if statusUnavailable}
			<p class="ani-plain-msg">This view needs a newer version than the one currently running.</p>
		{:else if statusError !== null}
			<div class="ani-error" role="alert" data-augmented-ui="tl-clip br-clip border">
				<p class="ani-error-msg">Could not load Anthropic status: {statusError}</p>
				{#if onRetryStatus}
					<span class="ani-retry-backing"
						><button
							type="button"
							class="ani-retry"
							data-augmented-ui="tr-clip bl-clip border"
							onclick={() => onRetryStatus?.()}>Retry</button
						></span
					>
				{/if}
			</div>
		{:else if status === null}
			<p class="ani-plain-msg" aria-busy="true">Loading Anthropic status...</p>
		{:else}
			<div class="ani-mode" aria-labelledby="ani-mode-title">
				<span class="ani-mode-title" id="ani-mode-title">Authorization mode</span>
				<div class="ani-mode-list" role="radiogroup" aria-label="Anthropic authorization mode">
					{#each MODE_OPTIONS as opt (opt.value)}
						{@const selected = status.mode === opt.value}
						<label class="ani-mode-opt" class:ani-mode-opt--selected={selected}>
							<input
								class="ani-mode-check"
								type="radio"
								name="ani-auth-mode"
								value={opt.value}
								checked={selected}
								disabled={modePending}
								onchange={() => void selectMode(opt.value)}
							/>
							<span class="ani-mode-label">{opt.label}</span>
						</label>
					{/each}
				</div>
				<p
					class="ani-mode-result"
					class:ani-mode-result--fail={modeError !== null}
					aria-live="polite"
				>
					{modePending ? 'Saving...' : (modeError ?? '')}
				</p>
			</div>

			{#if status.mode === 'api_key'}
				<form class="ani-keyform" onsubmit={applyKey} aria-busy={applying}>
					<label class="ani-keyform-label" for="ani-key-input">Anthropic API key</label>
					<div class="ani-keyform-row">
						<input
							id="ani-key-input"
							class="ani-keyform-input"
							type="password"
							placeholder="sk-ant-..."
							autocomplete="off"
							spellcheck="false"
							disabled={applying}
							aria-describedby="ani-key-result"
							bind:value={keyInput}
						/>
						<button
							type="submit"
							class="ani-keyform-submit"
							data-augmented-ui="tr-clip bl-clip border"
							disabled={applying || keyInput.trim().length === 0}
						>
							Apply key
						</button>
					</div>
					<p
						id="ani-key-result"
						class="ani-keyform-result"
						class:ani-keyform-result--ok={keyResult?.ok === true}
						class:ani-keyform-result--fail={keyResult?.ok === false}
						aria-live="polite"
					>
						{keyResult?.message ?? ''}
					</p>
				</form>

				{#if status.configured}
					<div class="ani-configured">
						<span class="ani-configured-item"
							>Key <code class="ani-key">{status.masked_key ?? 'configured'}</code></span
						>
						<span class="ani-retry-backing"
							><button
								type="button"
								class="ani-clear"
								data-augmented-ui="tr-clip bl-clip border"
								disabled={clearing}
								onclick={() => void clearKey()}>{clearing ? 'Clearing...' : 'Clear'}</button
							></span
						>
					</div>
					{#if clearError !== null}
						<p class="ani-clear-error" role="alert">Could not clear key: {clearError}</p>
					{/if}
				{/if}
			{:else if status.mode === 'claude_code_login'}
				<div class="ani-cli">
					{#if cliStatusLoading && cliStatus === null}
						<p class="ani-plain-msg" aria-busy="true">Checking sign-in status...</p>
					{:else if cliStatusError !== null && cliStatus === null}
						<div class="ani-error" role="alert" data-augmented-ui="tl-clip br-clip border">
							<p class="ani-error-msg">Could not load sign-in status: {cliStatusError}</p>
							<span class="ani-retry-backing"
								><button
									type="button"
									class="ani-retry"
									data-augmented-ui="tr-clip bl-clip border"
									onclick={() => void loadCliStatus()}>Retry</button
								></span
							>
						</div>
					{:else if cliStatus !== null}
						{#if !cliStatus.installed}
							<div class="ani-error" role="alert" data-augmented-ui="tl-clip br-clip border">
								<p class="ani-error-msg">
									{cliStatus.error ?? 'claude CLI not found on this machine.'}
								</p>
							</div>
						{:else}
							<div class="ani-cli-status">
								{#if cliStatus.logged_in}
									<p class="ani-cli-status-line">
										Signed in as <strong>{cliStatus.email ?? 'unknown account'}</strong>
										{#if cliStatus.org_name}<span class="ani-cli-dim">· {cliStatus.org_name}</span
											>{/if}
										{#if cliStatus.subscription_type}<span class="ani-badge"
												>{cliStatus.subscription_type}</span
											>{/if}
									</p>
								{:else}
									<p class="ani-cli-status-line ani-cli-status-line--out">
										Not signed in on this machine.
									</p>
								{/if}
								<p class="ani-cli-binary">
									{cliStatus.binary}{#if cliStatus.version}
										· {cliStatus.version}{/if}
								</p>
							</div>
						{/if}

						<div class="ani-cli-actions">
							<span class="ani-retry-backing"
								><button
									type="button"
									class="ani-retry"
									data-augmented-ui="tr-clip bl-clip border"
									disabled={cliStatusLoading}
									onclick={() => void loadCliStatus()}
									>{cliStatusLoading ? 'Checking...' : 'Check again'}</button
								></span
							>
							{#if cliStatus.installed && !cliStatus.logged_in && cliLogin === null}
								<span class="ani-retry-backing"
									><button
										type="button"
										class="ani-test-btn"
										data-augmented-ui="tr-clip bl-clip border"
										disabled={cliLoginStarting}
										onclick={() => void startCliLogin()}
										>{cliLoginStarting ? 'Starting...' : 'Sign in'}</button
									></span
								>
							{/if}
							{#if cliStatus.installed && cliStatus.logged_in}
								{#if cliSignOutConfirming}
									<span class="ani-cli-confirm"
										>Sign out of this machine's claude CLI?
										<button
											type="button"
											class="ani-clear"
											data-augmented-ui="tr-clip bl-clip border"
											disabled={cliSigningOut}
											onclick={() => void confirmCliSignOut()}
											>{cliSigningOut ? 'Signing out...' : 'Confirm sign out'}</button
										>
										<button
											type="button"
											class="ani-retry"
											data-augmented-ui="tr-clip bl-clip border"
											disabled={cliSigningOut}
											onclick={() => (cliSignOutConfirming = false)}>Never mind</button
										>
									</span>
								{:else}
									<span class="ani-retry-backing"
										><button
											type="button"
											class="ani-clear"
											data-augmented-ui="tr-clip bl-clip border"
											onclick={() => (cliSignOutConfirming = true)}>Sign out</button
										></span
									>
								{/if}
							{/if}
						</div>
						{#if cliSignOutError !== null}
							<p class="ani-clear-error" role="alert">Could not sign out: {cliSignOutError}</p>
						{/if}
					{/if}

					{#if cliLogin !== null}
						<div class="ani-cli-login" data-augmented-ui="tl-clip br-clip border">
							{#if cliLogin.state === 'starting'}
								<p class="ani-plain-msg" aria-busy="true">Starting sign-in...</p>
							{:else if cliLogin.state === 'awaiting_code'}
								<p class="ani-cli-login-msg">
									Open the sign-in page, finish it in the browser, then paste the code it gives you
									back here.
								</p>
								<a
									class="ani-cli-open-link"
									data-augmented-ui="tr-clip bl-clip border"
									href={cliLogin.url}
									target="_blank"
									rel="noopener noreferrer">Open sign-in page</a
								>
								<form class="ani-cli-codeform" onsubmit={submitCliCode}>
									<input
										class="ani-keyform-input"
										type="text"
										placeholder="Paste the code here"
										autocomplete="off"
										spellcheck="false"
										disabled={cliSubmittingCode}
										bind:value={cliCode}
									/>
									<button
										type="submit"
										class="ani-keyform-submit"
										data-augmented-ui="tr-clip bl-clip border"
										disabled={cliSubmittingCode || cliCode.trim().length === 0}
									>
										{cliSubmittingCode ? 'Submitting...' : 'Submit code'}
									</button>
								</form>
								<span class="ani-retry-backing"
									><button
										type="button"
										class="ani-retry"
										data-augmented-ui="tr-clip bl-clip border"
										onclick={() => void cancelCliLogin()}>Cancel sign-in</button
									></span
								>
							{:else if cliLogin.state === 'succeeded'}
								<p class="ani-mode-result ani-mode-result--ok">Signed in.</p>
							{:else if cliLogin.state === 'cancelled'}
								<p class="ani-plain-msg">Sign-in cancelled.</p>
							{/if}
						</div>
					{/if}

					{#if cliLoginError !== null}
						<div class="ani-error" role="alert" data-augmented-ui="tl-clip br-clip border">
							<p class="ani-error-msg">{cliLoginError}</p>
							{#if cliLoginFallback}
								<p class="ani-cli-fallback">
									Run it yourself instead: <code class="ani-key">{cliLoginFallback}</code>, then
									check again.
								</p>
							{/if}
							<span class="ani-retry-backing"
								><button
									type="button"
									class="ani-retry"
									data-augmented-ui="tr-clip bl-clip border"
									onclick={dismissCliLogin}>Dismiss</button
								></span
							>
						</div>
					{/if}
				</div>
			{/if}

			<div class="ani-test">
				<span class="ani-retry-backing"
					><button
						type="button"
						class="ani-test-btn"
						data-augmented-ui="tr-clip bl-clip border"
						disabled={testing}
						onclick={() => void testConnection()}
						>{testing ? 'Testing...' : 'Test connection'}</button
					></span
				>
				<p
					class="ani-test-result"
					class:ani-test-result--ok={testResult?.ok === true}
					class:ani-test-result--fail={testResult?.ok === false}
					aria-live="polite"
				>
					{testResult?.message ?? ''}
				</p>
			</div>
		{/if}
	</div>
</section>

<style>
	.ani-section {
		display: flex;
		flex-direction: column;
	}
	.ani-card {
		--edge-tl: 8px;
		--edge-br: 10px;
		--aug-tl: var(--edge-tl);
		--aug-br: var(--edge-br);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-info-fg) 0%,
			var(--status-info-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		background: var(--surface-card);
		overflow: visible;
	}

	.ani-card-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 16px 16px 0;
	}
	.ani-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--vendor-anthropic);
		flex-shrink: 0;
	}
	.ani-title {
		font-family: var(--font-display);
		font-size: 13px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
		margin: 0;
	}

	.ani-desc {
		margin: 6px 0 0;
		padding: 0 16px;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}

	.ani-plain-msg {
		margin: 0;
		padding: 14px 16px;
		font-size: 12px;
		color: var(--text-3);
		line-height: 1.55;
	}

	.ani-error {
		--edge: 10px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: linear-gradient(
			135deg,
			var(--status-danger-fg) 0%,
			var(--status-danger-border) 35%,
			var(--border-default) 70%
		);
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin: 14px 16px 0;
		padding: calc(var(--edge) + 6px) calc(var(--edge) + 10px);
		background: var(--status-danger-bg);
	}
	.ani-error-msg {
		margin: 0;
		font-size: 13px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ani-retry-backing {
		align-self: flex-start;
		display: inline-flex;
		background: transparent;
	}
	.ani-retry {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 6px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.ani-retry:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}

	.ani-mode {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 14px 16px;
		border-top: 1px solid var(--border-subtle);
	}
	.ani-mode-title {
		font-family: var(--font-display);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.ani-mode-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border-default);
		border-radius: 6px;
	}
	.ani-mode-opt {
		position: relative;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border: none;
		border-bottom: 1px solid var(--border-subtle);
		border-left: 2px solid transparent;
		background: var(--surface-card);
		cursor: pointer;
		transition:
			background 100ms ease-out,
			border-left-color 100ms ease-out;
	}
	.ani-mode-opt:last-child {
		border-bottom: none;
	}
	.ani-mode-opt:hover {
		background: var(--surface-2);
	}
	.ani-mode-opt:has(input:focus-visible) {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: -2px;
	}
	.ani-mode-opt--selected {
		background: var(--surface-2);
		border-left-color: var(--interactive-active-border);
	}
	.ani-mode-check {
		flex-shrink: 0;
		accent-color: var(--interactive-active-fg);
	}
	.ani-mode-label {
		font-size: 12px;
		color: var(--text-1);
	}
	.ani-mode-result {
		margin: 0;
		min-height: 15px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-3);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ani-mode-result--fail {
		color: var(--status-danger-fg);
	}
	.ani-mode-result--ok {
		color: var(--status-success-fg);
	}

	.ani-cli {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 14px 16px;
		border-top: 1px solid var(--border-subtle);
	}
	.ani-cli-status {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.ani-cli-status-line {
		margin: 0;
		font-size: 13px;
		color: var(--text-1);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ani-cli-status-line--out {
		color: var(--text-3);
	}
	.ani-cli-dim {
		color: var(--text-3);
		font-weight: 400;
	}
	.ani-badge {
		display: inline-flex;
		align-items: center;
		margin-left: 6px;
		padding: 1px 7px;
		border-radius: 4px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		border: 1px solid var(--status-info-border);
	}
	.ani-cli-binary {
		margin: 0;
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-3);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ani-cli-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.ani-cli-confirm {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		font-size: 12px;
		color: var(--text-2);
	}
	.ani-cli-login {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		padding: calc(var(--edge) + 4px) calc(var(--edge) + 8px);
		background: var(--surface-2);
	}
	.ani-cli-login-msg {
		margin: 0;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.55;
	}
	.ani-cli-open-link {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--interactive-active-border);
		display: inline-flex;
		align-items: center;
		padding: 5px 14px;
		background: var(--surface-card);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		font-weight: 600;
		text-decoration: none;
		transition: color 100ms ease-out;
	}
	.ani-cli-open-link:hover {
		color: var(--text-strong);
	}
	.ani-cli-open-link:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ani-cli-codeform {
		display: flex;
		gap: 8px;
		width: 100%;
	}
	.ani-cli-fallback {
		margin: 0;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.ani-keyform {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 14px 16px;
		border-top: 1px solid var(--border-subtle);
	}
	.ani-keyform-label {
		font-size: 11px;
		color: var(--text-3);
	}
	.ani-keyform-row {
		display: flex;
		gap: 8px;
	}
	.ani-keyform-input {
		flex: 1 1 auto;
		min-width: 0;
		padding: 5px 8px;
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-1);
		border-radius: 4px;
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		transition: border-color 100ms ease-out;
	}
	.ani-keyform-input:hover:not(:disabled) {
		border-color: var(--text-3);
	}
	.ani-keyform-input:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
		border-color: var(--interactive-focus-ring);
	}
	.ani-keyform-input:disabled {
		cursor: not-allowed;
		opacity: 0.6;
	}
	.ani-keyform-submit {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--interactive-active-border);
		flex-shrink: 0;
		padding: 5px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.ani-keyform-submit:hover:not(:disabled) {
		color: var(--text-strong);
	}
	.ani-keyform-submit:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ani-keyform-submit:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.ani-keyform-result {
		margin: 0;
		min-height: 15px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-3);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ani-keyform-result--ok {
		color: var(--status-success-fg);
	}
	.ani-keyform-result--fail {
		color: var(--status-danger-fg);
	}

	.ani-configured {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		padding: 0 16px 14px;
	}
	.ani-configured-item {
		font-size: 11px;
		color: var(--text-3);
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.ani-key {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		color: var(--text-2);
		background: var(--surface-2);
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		padding: 1px 6px;
		word-break: break-all;
	}
	.ani-clear {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 4px 12px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 11px;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.ani-clear:hover:not(:disabled) {
		color: var(--status-danger-fg);
		--aug-border-bg: var(--status-danger-border);
	}
	.ani-clear:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.ani-clear-error {
		margin: 0;
		padding: 0 16px 14px;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
	}

	.ani-test {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 6px;
		padding: 14px 16px 16px;
		border-top: 1px solid var(--border-subtle);
	}
	.ani-test-btn {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0;
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--interactive-active-border);
		padding: 5px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		transition: color 100ms ease-out;
	}
	.ani-test-btn:hover:not(:disabled) {
		color: var(--text-strong);
	}
	.ani-test-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.ani-test-btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}
	.ani-test-result {
		margin: 0;
		min-height: 15px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-3);
		overflow-wrap: break-word;
		word-break: break-word;
	}
	.ani-test-result--ok {
		color: var(--status-success-fg);
	}
	.ani-test-result--fail {
		color: var(--status-danger-fg);
	}
</style>
