<script lang="ts">
	import { onMount } from 'svelte';
	import GuideLayout from '$lib/components/help/GuideLayout.svelte';
	import { Button } from '$lib/components/ui/button';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import {
		getConnectionInfo,
		directClaudeCommand,
		type ConnectionInfo
	} from '$lib/api/connectionInfo';
	import { resolve } from '$app/paths';

	const toc = [
		{ slug: 'prerequisites', label: 'Before you start' },
		{ slug: 'start-the-proxy', label: 'Start mike' },
		{ slug: 'point-claude-code-at-it', label: 'Connect a client' },
		{ slug: 'verify-capture', label: 'Verify capture' },
		{ slug: 'next-steps', label: 'Next steps' }
	];
	let connection = $state<ConnectionInfo | null>(null);
	let connectionError = $state<string | null>(null);
	let loading = $state(true);
	let request: AbortController | null = null;
	const command = $derived(
		connection?.proxy_url ? directClaudeCommand(connection.proxy_url) : null
	);

	async function refreshConnection(): Promise<void> {
		request?.abort();
		const active = new AbortController();
		request = active;
		loading = true;
		connectionError = null;
		try {
			const next = await getConnectionInfo(active.signal);
			if (!active.signal.aborted) connection = next;
		} catch (error) {
			if (!active.signal.aborted) {
				connection = null;
				connectionError = error instanceof Error ? error.message : String(error);
			}
		} finally {
			if (!active.signal.aborted) loading = false;
		}
	}

	onMount(() => {
		void refreshConnection();
		return () => request?.abort();
	});
</script>

<GuideLayout
	title="Get Claude Code talking to mike"
	summary="Start the installed application, connect a client, and verify a real captured request. Addresses below come from this running instance."
	audience="first-time setup"
	duration="3 min"
	{toc}
>
	<h2 id="prerequisites">Before you start</h2>
	<p>
		Use the installed <code>mike</code> and <code>mike-mcp</code> binaries from a qualified archive. You
		do not need Rust, Node, or a source checkout to use this app or its bundled help. Claude Code must
		be installed and authenticated. A version check does not create a capture.
	</p>
	<p>
		Read <a href={resolve('/help/docs/[slug]', { slug: 'installed-setup' })}>Installed setup</a>
		for configuration, file locations, and the choice between durable PostgreSQL history and ephemeral
		trial mode.
	</p>

	<h2 id="start-the-proxy">Start mike</h2>
	<p>
		Run <code>mike</code> in a terminal for Setup, Start, Stop and Config. For a foreground server,
		run <code>mike serve</code>; Ctrl-C stops it. Use the same <code>--config</code> file on all commands
		if you selected a custom installation. The installed binary serves this browser UI and its same-origin
		API itself.
	</p>
	<p>
		From your project directory, <code>mike claude</code> starts the selected stack if necessary and launches
		Claude Code wired to it. Review folder auto-trust first: the wrapper can record the working directory
		as trusted when that setting is enabled.
	</p>
	<pre><code>mike claude</code></pre>

	<h2 id="point-claude-code-at-it">Connect a client</h2>
	<p>
		These addresses are for clients on the computer running mike. They are not remote-access URLs.
	</p>
	{#if loading}
		<p role="status">Reading this instance's connection addresses...</p>
	{:else if connectionError}
		<p role="alert">{connectionError} No default address has been substituted.</p>
		<Button variant="outline" onclick={() => void refreshConnection()}
			>Retry connection information</Button
		>
	{:else if connection}
		<p>Proxy: <code>{connection.proxy_url ?? 'Unavailable'}</code></p>
		<p>Inspect / MCP: <code>{connection.inspect_url ?? 'Unavailable'}</code></p>
		<p>Bundled browser: <code>{connection.browser_url ?? 'Disabled or unavailable'}</code></p>
		{#if command}
			<p>To connect Claude Code directly to the running proxy without the wrapper:</p>
			<pre><code>{command}</code></pre>
			<CopyIconButton getText={() => command ?? ''} label="Copy direct connection command" />
		{/if}
		{#if connection.inspect_url}
			<p>
				For the included <code>mike-mcp</code> stdio binary, set this value in your MCP client's environment:
			</p>
			<pre><code>MIKE_INSPECT_URL={connection.inspect_url}</code></pre>
		{/if}
	{/if}
	<p>
		A standalone Anthropic-compatible client also needs its normal authentication and a valid UUID
		in the <code>x-claude-code-session-id</code> request header for capture. Requests with missing
		or invalid session headers forward without capture. See
		<a href={resolve('/help/docs/[slug]', { slug: 'connections-and-privacy' })}
			>Connections and privacy</a
		>
		for the four usage options, local-access limits, optional providers, and request/response mutation.
	</p>

	<h2 id="verify-capture">Verify capture</h2>
	<ol>
		<li>Send a real prompt in Claude Code, such as <code>hello</code>.</li>
		<li>Open <a href={resolve('/sessions')}>Sessions</a> and select the new session.</li>
		<li>Open a request and confirm that both the prompt and response are present.</li>
		<li>
			For durable history, restart the same installation and confirm that the session remains.
		</li>
	</ol>
	<p>
		If nothing appears, check Inspect API in the help drawer or
		<!-- eslint-disable svelte/no-navigation-without-resolve -- Same-origin backend endpoint, not a SvelteKit page. -->
		<a href="/api/health" target="_blank" rel="noopener noreferrer"
			>this instance's health endpoint</a
		>. Then follow
		<a href={resolve('/help/docs/[slug]', { slug: 'recovery-and-maintenance' })}
			>Recovery and maintenance</a
		>.
	</p>

	<h2 id="next-steps">Next steps</h2>
	<ul>
		<li>
			<a href={resolve('/help/guides/inspecting-requests')}>Inspect a captured request</a>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->.
		</li>
		<li><a href={resolve('/settings')}>Review active features and provider settings</a>.</li>
		<li>
			<a href={resolve('/help/guides/retention')}>Choose retention and pin important history</a>.
		</li>
	</ul>
</GuideLayout>
