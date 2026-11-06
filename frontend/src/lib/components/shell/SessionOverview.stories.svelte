<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { resolve } from '$app/paths';
	import SessionOverview from './SessionOverview.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const { Story } = defineMeta({
		title: 'Components/Shell/SessionOverview',
		component: SessionOverview,
		tags: ['autodocs'],
		parameters: { layout: 'fullscreen' }
	});

	interface MockSession {
		id: string;
		title: string | null;
		project: string | null;
		first_seen: string;
		last_seen: string;
		request_count: number;
		total_requests: number;
		models: string[];
		last_status: string;
		pinned: boolean;
		avg_latency_ms: number | null;
		latest_request_id: number | null;
	}

	function defaultSession(overrides: Partial<MockSession> = {}): MockSession {
		return {
			id: 'hash-a1b2c3d4e5f6a7b8',
			title: 'Fix auth null pointer exception',
			project: '/Users/dev/myproject',
			first_seen: '2026-05-04T10:00:00Z',
			last_seen: '2026-05-04T10:05:42Z',
			request_count: 7,
			total_requests: 7,
			models: ['claude-opus-4-6'],
			last_status: 'Complete',
			pinned: false,
			avg_latency_ms: 4200,
			latest_request_id: 107,
			...overrides
		};
	}

	function shortId(id: string): string {
		if (id.startsWith('hash-')) return id.slice(5, 13);
		return id.slice(0, 8);
	}

	function fmtDuration(ms: number): string {
		if (ms < 1000) return `${ms}ms`;
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
		const m = Math.floor(ms / 60_000);
		const s = Math.round((ms % 60_000) / 1000);
		return s > 0 ? `${m}m ${s}s` : `${m}m`;
	}

	function fmtClock(iso: string): string {
		const d = new Date(iso);
		const hh = String(d.getUTCHours()).padStart(2, '0');
		const mm = String(d.getUTCMinutes()).padStart(2, '0');
		return `${hh}:${mm}`;
	}

	function spanMs(session: MockSession): number {
		return Math.max(
			0,
			new Date(session.last_seen).getTime() - new Date(session.first_seen).getTime()
		);
	}

	interface MockSysBlock {
		index: number;
		text: string;
		charCount: number;
		expanded: boolean;
	}

	const SYS_BLOCKS: MockSysBlock[] = [
		{
			index: 0,
			charCount: 412,
			expanded: true,
			text: `You are Claude Code, Anthropic's official CLI for Claude.

You are an expert software engineer working in a TypeScript codebase.
You have deep knowledge of Node.js, Express, JWT authentication, and security best practices.
When you encounter an error, always examine the full stack trace before proposing a fix.
Prefer targeted, minimal changes. Do not refactor unless asked.`
		},
		{
			index: 1,
			charCount: 188,
			expanded: false,
			text: `Repository context: this project uses Jest for tests, ESLint with strict mode, and Prettier for formatting. All changes must pass \`npm test\` before being considered done.`
		},
		{
			index: 2,
			charCount: 97,
			expanded: false,
			text: `Current task: fix the JWT middleware null pointer exception described in the user prompt.`
		}
	];

	const CHANGED_FILES = [
		'/Users/dev/myproject/src/auth/jwt.ts',
		'/Users/dev/myproject/src/auth/token_store.ts',
		'/Users/dev/myproject/src/auth/session.ts',
		'/Users/dev/myproject/src/middleware/error_handler.ts',
		'/Users/dev/myproject/tests/auth/jwt.test.ts',
		'/Users/dev/myproject/tests/auth/token_store.test.ts',
		'/Users/dev/myproject/package.json'
	];

	const AGENT_COLORS: Record<string, string> = {
		'rust-developer': '#c87941',
		'frontend-developer': '#4a9f6f',
		'ui-designer': '#9b72cf',
		'db-dev': '#5b8dd9',
		watchdog: '#c48080'
	};

	function agentColor(type: string): string | null {
		return AGENT_COLORS[type] ?? null;
	}

	interface MockAgentGroup {
		subagent_type: string;
		dispatches: { description: string; parent_request_id: number }[];
		model_mock: string | null;
	}

	const AGENT_GROUPS: MockAgentGroup[] = [
		{
			subagent_type: 'rust-developer',
			model_mock: 'claude-sonnet-4-6',
			dispatches: [
				{
					parent_request_id: 42,
					description:
						'Implement JWT token refresh endpoint in the auth crate. Add unit tests for token expiry edge cases. The new endpoint should accept a refresh token and return a new access token with a fresh expiry. Handle the case where the refresh token is expired by returning 401.'
				},
				{
					parent_request_id: 67,
					description: 'Add integration tests for the full refresh flow hitting a test HTTP server.'
				}
			]
		},
		{
			subagent_type: 'frontend-developer',
			model_mock: 'claude-opus-4-6',
			dispatches: [
				{
					parent_request_id: 55,
					description:
						'Update the auth service in the SPA to call the new /auth/refresh endpoint when a 401 is received. Wire in exponential backoff with max 3 retries. Show a session-expired modal if all retries fail.'
				}
			]
		},
		{
			subagent_type: 'general-purpose',
			model_mock: null,
			dispatches: [
				{
					parent_request_id: 71,
					description: 'Review the final implementation and write a summary of the changes made.'
				}
			]
		}
	];

	const PIN_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2L15.09 8.26L22 9.27L17 14.14L18.18 21.02L12 17.77L5.82 21.02L7 14.14L2 9.27L8.91 8.26L12 2Z"/></svg>`;
	const PIN_ICON_FILLED = `<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2L15.09 8.26L22 9.27L17 14.14L18.18 21.02L12 17.77L5.82 21.02L7 14.14L2 9.27L8.91 8.26L12 2Z"/></svg>`;
	const COPY_ICON = `<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
	const WRENCH_ICON = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>`;
	const FILES_ICON = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>`;
	const AGENT_ICON = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="8" r="5"/><path d="M20 21a8 8 0 1 0-16 0"/></svg>`;
	const SYS_ICON = `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`;
	const CHART_ICON = `<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>`;
	const ARROW_DOWN_ICON = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 5v14"/><path d="m19 12-7 7-7-7"/></svg>`;
	const CHECK_ICON = `<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>`;

	void PIN_ICON;
	void PIN_ICON_FILLED;
	void COPY_ICON;
	void WRENCH_ICON;
	void FILES_ICON;
	void AGENT_ICON;
	void SYS_ICON;
	void CHART_ICON;
	void ARROW_DOWN_ICON;
	void CHECK_ICON;
</script>

<script lang="ts">
	let pinned = $state(false);

	let copiedId = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | null = null;

	async function copyToClipboard(text: string): Promise<void> {
		try {
			await navigator.clipboard.writeText(text);
			copiedId = true;
			if (copyTimer) clearTimeout(copyTimer);
			copyTimer = setTimeout(() => {
				copiedId = false;
				copyTimer = null;
			}, 1500);
		} catch {}
	}

	let sysBlockOpen = $state<Record<number, boolean>>({ 0: true, 1: false, 2: false });

	function toggleSysBlock(index: number): void {
		sysBlockOpen[index] = !sysBlockOpen[index];
	}
</script>

<Story name="Complete">
	{#snippet template()}
		{@const s = defaultSession({ request_count: 24, total_requests: 24, avg_latency_ms: 3800 })}
		{@const sid = shortId(s.id)}
		{@const span = spanMs(s)}
		{@const totalDispatches = AGENT_GROUPS.reduce((n, g) => n + g.dispatches.length, 0)}
		<div class="story-frame">
			<div class="s-detail-head">
				<div class="s-path">
					<button
						type="button"
						class="s-pin"
						aria-label={pinned ? 'Unpin session' : 'Pin session'}
						title={pinned ? 'Unpin session' : 'Pin session'}
						onclick={() => {
							pinned = !pinned;
						}}
					>
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- PIN_ICON/PIN_ICON_FILLED are hardcoded app-authored SVG constants, no dynamic/captured content -->
						{@html pinned ? PIN_ICON_FILLED : PIN_ICON}
					</button>
					<span class="s-title" title={s.title ?? s.id}>{s.title ?? sid}</span>
					<a
						href={resolve('/analysis')}
						class="s-analysis-btn"
						aria-label="View diagnostic findings for this session"
					>
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- CHART_ICON is a hardcoded app-authored SVG constant, no dynamic/captured content -->
						{@html CHART_ICON} Analysis
					</a>
				</div>
				<div class="s-sub">
					<span class="s-id-group" aria-label="Session id: {s.id}">
						<span class="s-mono s-sid-short">{sid}</span>
						<button
							type="button"
							class="s-copy-btn"
							aria-label={copiedId ? 'Copied' : 'Copy full session id to clipboard'}
							aria-live="polite"
							title={s.id}
							onclick={() => copyToClipboard(s.id)}
							><!-- eslint-disable-line svelte/no-at-html-tags -- CHECK_ICON/COPY_ICON are hardcoded app-authored SVG constants, no dynamic/captured content -->{@html copiedId
								? CHECK_ICON
								: COPY_ICON}</button
						>
					</span>
					{#if s.project}
						<span class="s-dot" aria-hidden="true">·</span>
						<span class="s-mono s-project" title={s.project}>{s.project}</span>
					{/if}
					<span class="s-dot" aria-hidden="true">·</span>
					<span>{s.request_count} requests</span>
				</div>
			</div>

			<div class="s-metrics" role="list" aria-label="Session metrics">
				<div class="s-mc" role="listitem">
					<div class="s-mk">Requests</div>
					<div class="s-mv">{s.request_count}</div>
				</div>
				<div class="s-mc" role="listitem">
					<div class="s-mk">Last request</div>
					<div class="s-mv"><span class="s-pill s-pill-ok">✓ Complete</span></div>
				</div>
				<div class="s-mc" role="listitem">
					<div class="s-mk">Model</div>
					<div class="s-mv s-mv-model">{s.models[0]}</div>
				</div>
				<div class="s-mc" role="listitem">
					<div class="s-mk">Span</div>
					<div class="s-mv s-mv-span">
						<span>{fmtClock(s.first_seen)}</span><span class="s-arr">→</span><span
							>{fmtClock(s.last_seen)}</span
						>
						<span class="s-dim">({fmtDuration(span)})</span>
					</div>
				</div>
				<div class="s-mc" role="listitem">
					<div class="s-mk">Avg latency</div>
					<div class="s-mv">{fmtDuration(s.avg_latency_ms ?? 0)}</div>
				</div>
			</div>

			<div class="s-dashboard-grid">
				<div class="s-left">
					<div class="s-card">
						<div class="s-card-hd">
							<!-- eslint-disable-next-line svelte/no-at-html-tags -- FILES_ICON is a hardcoded app-authored SVG constant, no dynamic/captured content -->
							{@html FILES_ICON}
							Files changed <span class="s-badge">{CHANGED_FILES.length}</span>
						</div>
						{#each CHANGED_FILES as fp (fp)}
							<div class="s-file-row"><span class="s-file-path">{fp}</span></div>
						{/each}
					</div>
				</div>

				<div class="s-right">
					<div class="s-card s-card-no-pad">
						<div class="s-card-hd s-card-hd-pad">
							<!-- eslint-disable-next-line svelte/no-at-html-tags -- AGENT_ICON is a hardcoded app-authored SVG constant, no dynamic/captured content -->
							{@html AGENT_ICON}
							Agents active
							<span class="s-badge">{totalDispatches}</span>
							<span class="s-badge-note">· {AGENT_GROUPS.length} types</span>
						</div>
						{#each AGENT_GROUPS as group (group.subagent_type)}
							{@const color = agentColor(group.subagent_type)}
							<div class="s-agent-group">
								<div class="s-agent-type-row">
									{#if color}
										<span class="s-agent-dot" style="background:{color}" aria-hidden="true"></span>
									{:else}
										<span class="s-agent-dot s-agent-dot-neutral" aria-hidden="true"></span>
									{/if}
									<span class="s-agent-type" style={color ? `color:${color}` : ''}
										>{group.subagent_type}</span
									>
									<span class="s-agent-count"
										>{group.dispatches.length}
										{group.dispatches.length === 1 ? 'dispatch' : 'dispatches'}</span
									>
									{#if group.model_mock}
										<span class="s-agent-model">{group.model_mock}</span>
									{:else}
										<span
											class="s-agent-model s-agent-model-unknown"
											title="Model unavailable — requires per-request agent attribution"
											>model?</span
										>
									{/if}
									<span
										class="s-agent-turns-dep"
										title="View agent turns — requires per-request attribution (frontend-developer dependency)"
										>turns ↗</span
									>
								</div>
								<div class="s-dispatch-list">
									{#each group.dispatches as dispatch (dispatch.parent_request_id)}
										<div class="s-dispatch-item">
											<a
												href={resolve(
													`/sessions/${s.id}/requests/${dispatch.parent_request_id}` as `/sessions/${string}/requests/${string}`
												)}
												class="s-dispatch-link"
												aria-label="Go to dispatch request #{dispatch.parent_request_id}"
												title="Go to the request that issued this Task call"
												>#{dispatch.parent_request_id}</a
											>
											<div class="s-dispatch-desc">{dispatch.description}</div>
										</div>
									{/each}
								</div>
							</div>
						{/each}
					</div>
				</div>
			</div>

			<div class="s-full">
				<div class="s-card">
					<div class="s-card-hd">
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- WRENCH_ICON is a hardcoded app-authored SVG constant, no dynamic/captured content -->
						{@html WRENCH_ICON}
						Tool uses <span class="s-badge">18</span>
					</div>
					<div class="s-tool-row">
						<span class="s-tn s-tn-agent">Task</span><span class="s-ta"
							>Implement JWT token refresh endpoint in the auth crate...</span
						>
					</div>
					<div class="s-tool-row">
						<span class="s-tn s-tn-agent">Task</span><span class="s-ta"
							>Update the auth service in the SPA to call /auth/refresh...</span
						>
					</div>
					<div class="s-tool-row">
						<span class="s-tn s-tn-agent">Task</span><span class="s-ta"
							>Review the final implementation and write a summary...</span
						>
					</div>
					{#each CHANGED_FILES.slice(0, 4) as fp (fp)}
						<div class="s-tool-row">
							<span class="s-tn s-tn-file">Write</span><span class="s-tp">{fp}</span>
						</div>
					{/each}
					<div class="s-tool-row">
						<span class="s-tn s-tn-shell">Bash</span><span class="s-ta">npm test</span>
					</div>
				</div>
			</div>

			<div class="s-full s-systemprompt-section">
				<div class="s-card s-card-no-pad">
					<div class="s-card-hd s-card-hd-pad">
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- SYS_ICON is a hardcoded app-authored SVG constant, no dynamic/captured content -->
						{@html SYS_ICON}
						System prompt
						<span class="s-badge">{SYS_BLOCKS.length} blocks</span>
					</div>
					{#each SYS_BLOCKS as block (block.index)}
						{@const isOpen = sysBlockOpen[block.index] ?? false}
						<div class="s-sys-block">
							<button
								type="button"
								class="s-sys-block-hd"
								aria-expanded={isOpen}
								onclick={() => toggleSysBlock(block.index)}
							>
								<span class="s-sys-chevron {isOpen ? 's-sys-chevron-open' : ''}">▶</span>
								<span class="s-sys-block-idx">Block {block.index + 1}</span>
								<span class="s-sys-chars">{block.charCount.toLocaleString()} chars</span>
							</button>
							{#if isOpen}
								<div class="s-sys-body">
									<pre class="s-sys-pre">{block.text}</pre>
								</div>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/snippet}
</Story>
