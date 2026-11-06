<script lang="ts">
	import { resolve } from '$app/paths';
	import {
		BookOpen,
		Bot,
		ExternalLink,
		Activity,
		MessageSquareWarning,
		Sparkles,
		GraduationCap
	} from '@lucide/svelte';
	import * as Sheet from '$lib/components/ui/sheet';
	import { shellState } from '$lib/stores/shellState.svelte';
	import { ApiError, getHealth } from '$lib/api';
	import type { HealthResponse, StatsResponse } from '$lib/types';
	import { GUIDES } from '$lib/help/guides';
	import { PROJECT_DOCS, ANTHROPIC_DOCS } from '$lib/help/externalDocs';

	interface Props {
		stats: StatsResponse | null;
	}

	const { stats }: Props = $props();

	const docs: Array<{
		label: string;
		href: string;
		description: string;
		internal?: boolean;
	}> = [
		{
			label: 'Help index',
			href: '/help',
			description: 'How-to guides, glossary, and installed documentation.',
			internal: true
		},
		{
			label: 'Glossary',
			href: '/help/glossary',
			description: 'Single source of truth for vocabulary in mike.',
			internal: true
		},
		...PROJECT_DOCS
	];

	const anthropic: Array<{ label: string; href: string; description: string }> = ANTHROPIC_DOCS;

	function guideHref(slug: string): string {
		switch (slug) {
			case 'getting-started':
				return resolve('/help/guides/getting-started');
			case 'inspecting-requests':
				return resolve('/help/guides/inspecting-requests');
			case 'filter-rules':
				return resolve('/help/guides/filter-rules');
			case 'anomalies':
				return resolve('/help/guides/anomalies');
			case 'retention':
				return resolve('/help/guides/retention');
			case 'steering-concepts':
				return resolve('/help/guides/steering-concepts');
			default:
				return resolve('/help/guides');
		}
	}

	function docHref(href: string): string {
		if (href === '/help') return resolve('/help');
		if (href === '/help/glossary') return resolve('/help/glossary');
		if (href.startsWith('/help/docs/'))
			return resolve('/help/docs/[slug]', { slug: href.split('/').at(-1) ?? '' });
		return href;
	}

	let health = $state<HealthResponse | null>(null);
	let healthError = $state<string | null>(null);
	let healthFailure = $state('health request failed');
	let healthRequest = 0;

	function setOpen(open: boolean): void {
		shellState.helpDrawerOpen = open;
	}

	async function loadHealth(): Promise<void> {
		const request = ++healthRequest;
		try {
			const response = await getHealth();
			if (request !== healthRequest) return;
			health = response;
			healthError = null;
		} catch (err) {
			if (request !== healthRequest) return;
			health = null;
			healthFailure =
				err instanceof ApiError
					? 'reachable · health request failed'
					: err instanceof TypeError
						? 'unreachable'
						: 'health request failed';
			healthError = err instanceof Error ? err.message : String(err);
		}
	}

	$effect(() => {
		if (shellState.helpDrawerOpen) {
			health = null;
			healthError = null;
			void loadHealth();
		}
		return () => {
			healthRequest++;
		};
	});

	function formatUptime(seconds: number): string {
		if (!Number.isFinite(seconds) || seconds < 0) return '—';
		const days = Math.floor(seconds / 86_400);
		const hours = Math.floor((seconds % 86_400) / 3_600);
		const minutes = Math.floor((seconds % 3_600) / 60);
		if (days > 0) return `${days}d ${hours}h`;
		if (hours > 0) return `${hours}h ${minutes}m`;
		return `${minutes}m`;
	}

	const uptimeLabel = $derived(stats ? formatUptime(stats.uptime_seconds) : '—');
</script>

<Sheet.Root open={shellState.helpDrawerOpen} onOpenChange={setOpen}>
	<Sheet.Content aria-describedby={undefined} class="!max-w-[440px]">
		<div class="drawer-head">
			<Sheet.Title>Help & references</Sheet.Title>
			<p class="drawer-sub">Installed guides, upstream context, and capture diagnostics.</p>
		</div>

		<div class="drawer-body">
			<section class="group" aria-labelledby="help-guides-heading">
				<header class="group-head">
					<GraduationCap class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<h3 id="help-guides-heading" class="group-label">How-to guides</h3>
				</header>
				<ul class="link-list">
					{#each GUIDES as guide (guide.slug)}
						<li>
							<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- guideHref() resolves each known literal internally; the rule can't see through the function wrapper -->
							<a class="link-row" href={guideHref(guide.slug)}>
								<span class="link-title">
									<span>{guide.title}</span>
								</span>
								<span class="link-desc">{guide.description}</span>
							</a>
						</li>
					{/each}
				</ul>
			</section>

			<section class="group" aria-labelledby="help-steering-heading">
				<header class="group-head">
					<Bot class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<h3 id="help-steering-heading" class="group-label">Steering</h3>
				</header>
				<ul class="link-list">
					<li>
						<a class="link-row" href={resolve('/help/guides/steering-concepts')}>
							<span class="link-title">
								<span>How steering works</span>
							</span>
							<span class="link-desc">Enrolment, authority levels, and mode selection.</span>
						</a>
					</li>
					<li>
						<a class="link-row" href={resolve('/steering')}>
							<span class="link-title">
								<span>Session Steering section</span>
							</span>
							<span class="link-desc">Overview of the steering interface.</span>
						</a>
					</li>
					<li>
						<a class="link-row" href={resolve('/steering?tab=fleet')}>
							<span class="link-title">
								<span>Fleet</span>
							</span>
							<span class="link-desc">Manage registered steerers.</span>
						</a>
					</li>
					<li>
						<a class="link-row" href={resolve('/steering?tab=monitor')}>
							<span class="link-title">
								<span>Monitor</span>
							</span>
							<span class="link-desc">Live view of steering events.</span>
						</a>
					</li>
				</ul>
			</section>

			<section class="group" aria-labelledby="help-docs-heading">
				<header class="group-head">
					<BookOpen class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<h3 id="help-docs-heading" class="group-label">Documentation</h3>
				</header>
				<ul class="link-list">
					{#each docs as link (link.href)}
						<li>
							<!-- eslint-disable svelte/no-navigation-without-resolve -- docs mixes internal routes (resolved via docHref) with external doc URLs from PROJECT_DOCS -->
							<a
								class="link-row"
								href={link.internal ? docHref(link.href) : link.href}
								target={link.internal ? undefined : '_blank'}
								rel={link.internal ? undefined : 'noopener noreferrer'}
							>
								<span class="link-title">
									<span>{link.label}</span>
									{#if !link.internal}
										<ExternalLink class="size-3" aria-hidden="true" />
									{/if}
								</span>
								<span class="link-desc">{link.description}</span>
							</a>
							<!-- eslint-enable svelte/no-navigation-without-resolve -->
						</li>
					{/each}
				</ul>
			</section>

			<section class="group" aria-labelledby="help-anthropic-heading">
				<header class="group-head">
					<Sparkles class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<h3 id="help-anthropic-heading" class="group-label">Anthropic context</h3>
				</header>
				<ul class="link-list">
					{#each anthropic as link (link.href)}
						<li>
							<!-- eslint-disable svelte/no-navigation-without-resolve -- anthropic entries are always external Anthropic/Claude Code doc URLs -->
							<a class="link-row" href={link.href} target="_blank" rel="noopener noreferrer">
								<span class="link-title">
									<span>{link.label}</span>
									<ExternalLink class="size-3" aria-hidden="true" />
								</span>
								<span class="link-desc">{link.description}</span>
							</a>
							<!-- eslint-enable svelte/no-navigation-without-resolve -->
						</li>
					{/each}
				</ul>
			</section>

			<section class="group" aria-labelledby="help-diagnostics-heading">
				<header class="group-head">
					<Activity class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<h3 id="help-diagnostics-heading" class="group-label">Diagnostics</h3>
				</header>
				<dl class="status-list">
					<div class="status-row">
						<dt>Inspect API</dt>
						<dd>
							{#if !health && healthError === null}
								<span class="muted">checking</span>
							{:else if health}
								<span class:ok={health.status === 'ok'}
									>reachable · {health.status === 'ok' ? 'healthy' : 'degraded'}</span
								>
							{:else}
								<span class="err">
									{healthFailure}{healthError ? ` (${healthError})` : ''}
								</span>
							{/if}
						</dd>
					</div>
					<div class="status-row">
						<dt>Captured requests</dt>
						<dd>
							<span class="font-mono">
								{stats ? stats.request_count.toLocaleString() : '—'}
							</span>
						</dd>
					</div>
					<div class="status-row">
						<dt>Active streams</dt>
						<dd>
							<span class="font-mono">
								{stats ? stats.active_streams.toLocaleString() : '—'}
							</span>
						</dd>
					</div>
					<div class="status-row">
						<dt>Uptime</dt>
						<dd><span class="font-mono">{uptimeLabel}</span></dd>
					</div>
				</dl>
				<details>
					<summary class="link-title">Capture-gap diagnostics</summary>
					{#if health?.capture_gap}
						{@const gap = health.capture_gap}
						<p class="link-desc">
							{gap.reading_age_secs === null
								? 'Unknown: no capture-activity reading is available.'
								: gap.alarm
									? 'Capture gap detected: recent sidecar traffic without recent work traffic.'
									: 'No capture gap detected in the current reading.'}
						</p>
						<p class="link-desc">
							The detector expects work traffic only when sidecar traffic is recent. It flags work
							silence longer than {gap.silence_threshold_secs} seconds with sidecar traffic within {gap.sidecar_recent_secs}
							seconds, using a {gap.lookback_secs}-second lookback. Idle time alone does not
							establish a capture failure. New sessions without work traffic are measured from first
							seen.
						</p>
						<p class="link-desc">
							Reading age: {gap.reading_age_secs === null
								? 'unknown'
								: `${gap.reading_age_secs} seconds`}. Flagged sessions: {gap.sessions.length}.
						</p>
						<ul class="link-list">
							{#each gap.sessions as session (session.session_id)}
								<li class="link-desc break-words">
									Session: {session.session_id}. Work silence: {session.work_silent_secs} seconds. Last
									work: {session.last_work_at ?? 'none observed'}. Last sidecar: {session.last_sidecar_at}.
								</li>
							{/each}
						</ul>
					{:else}
						<p class="link-desc">
							Capture-gap state: unknown. {healthError
								? 'The health request failed.'
								: health
									? 'This server did not provide capture-gap diagnostics.'
									: 'Checking the health endpoint.'}
						</p>
					{/if}
				</details>
				<ul class="link-list">
					<li>
						<!-- eslint-disable svelte/no-navigation-without-resolve -- Same-origin backend endpoint, not a SvelteKit page. -->
						<a class="link-row" href="/api/requests" target="_blank" rel="noopener noreferrer">
							<span class="link-title">
								<span>Open this instance’s inspect API</span>
								<ExternalLink class="size-3" aria-hidden="true" />
							</span>
							<span class="link-desc">
								Raw request list from the same origin as this browser UI.
							</span>
						</a>
						<!-- eslint-enable svelte/no-navigation-without-resolve -->
					</li>
				</ul>
			</section>

			<section class="group" aria-labelledby="help-issue-heading">
				<header class="group-head">
					<MessageSquareWarning class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<h3 id="help-issue-heading" class="group-label">Report an issue</h3>
				</header>
				<ul class="link-list">
					<li>
						<a
							class="link-row"
							href={resolve('/help/docs/[slug]', { slug: 'recovery-and-maintenance' })}
						>
							<span class="link-title">
								<span>Prepare a safe problem report</span>
							</span>
							<span class="link-desc">
								Troubleshooting and what to send your release contact.
							</span>
						</a>
					</li>
				</ul>
			</section>
		</div>
	</Sheet.Content>
</Sheet.Root>

<style>
	.drawer-head {
		padding: 16px 20px 12px;
		border-bottom: 1px solid var(--border-subtle);
	}
	.drawer-sub {
		margin: 4px 0 0;
		font-size: 0.75rem;
		color: var(--text-2);
	}
	.drawer-body {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		padding: 16px 20px 24px;
		display: flex;
		flex-direction: column;
		gap: 18px;
	}
	.group {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.group-head {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.group-label {
		margin: 0;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		font-size: 0.6875rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-3);
	}
	.link-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.link-row {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 8px 10px;
		border: 1px solid transparent;
		border-radius: 4px;
		text-decoration: none;
		transition:
			background 0.14s,
			border-color 0.14s,
			color 0.14s;
	}
	.link-row:hover {
		background: var(--surface-2);
		border-color: var(--border-default);
	}
	.link-row:focus-visible {
		outline: none;
		border-color: var(--interactive-active-border);
		box-shadow: 0 0 0 2px var(--ring);
	}
	.link-title {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--text-strong);
		font-size: 0.8125rem;
	}
	.link-row:hover .link-title {
		color: var(--interactive-active-fg);
	}
	.link-desc {
		font-size: 0.6875rem;
		color: var(--text-3);
	}
	.status-list {
		margin: 0;
		padding: 8px 10px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		background: var(--surface-2);
	}
	.status-row {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 12px;
	}
	.status-row dt {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-3);
	}
	.status-row dd {
		margin: 0;
		font-size: 0.75rem;
		color: var(--text-1);
	}
	.status-row .ok {
		color: var(--status-success-fg);
	}
	.status-row .err {
		color: var(--status-danger-fg);
		word-break: break-all;
	}
	.status-row .muted {
		color: var(--text-faint);
	}
</style>
