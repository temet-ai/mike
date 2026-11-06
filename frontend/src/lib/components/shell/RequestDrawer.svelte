<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import * as Dialog from '$lib/components/ui/dialog';
	import { SESSION_DETAIL_STORE_KEY } from '$lib/context-keys';
	import type { SessionDetailStore } from '$lib/stores/cache/sessionDetailStore.svelte';
	import {
		closeDrawer,
		isDrawerKind,
		openDrawer,
		shellState,
		type DrawerKind
	} from '$lib/stores/shellState.svelte';
	import RequestBodyCard from '$lib/components/detail/RequestBodyCard.svelte';
	import ResponseBodyCard from '$lib/components/detail/ResponseBodyCard.svelte';
	import DiagnosticsCard from '$lib/components/detail/DiagnosticsCard.svelte';
	import ToolCallCard from '$lib/components/tool-calls/ToolCallCard.svelte';

	const sessionDetailStore = getContext<SessionDetailStore | undefined>(SESSION_DETAIL_STORE_KEY);

	const open = $derived(shellState.drawerKind !== null && shellState.drawerBandId !== null);
	const kind = $derived<DrawerKind | null>(shellState.drawerKind);
	const bandId = $derived<number | null>(shellState.drawerBandId);

	const requestBodyInitialTab = $derived.by(
		(): 'original' | 'sent' | 'transformations' | 'diff' => {
			const raw = page.url.searchParams.get('bodyTab');
			return raw === 'sent' || raw === 'transformations' || raw === 'diff' ? raw : 'original';
		}
	);

	$effect(() => {
		const id = bandId;
		const store = sessionDetailStore;
		if (id === null || store === undefined) return;
		void store.loadDetail(id);
	});

	const detail = $derived(
		bandId === null || sessionDetailStore === undefined
			? null
			: (sessionDetailStore.details.get(bandId) ?? null)
	);

	const detailLoadFailed = $derived(
		bandId !== null &&
			sessionDetailStore !== undefined &&
			sessionDetailStore.detailErrors.has(bandId) &&
			detail === null
	);

	function retryDetail(): void {
		if (bandId === null || sessionDetailStore === undefined) return;
		void sessionDetailStore.loadDetail(bandId);
	}

	$effect(() => {
		const raw = page.url.searchParams.get('drawer');
		const ridRaw = page.params.requestId;
		const ridParsed = ridRaw ? Number(ridRaw) : Number.NaN;
		untrack(() => {
			if (!isDrawerKind(raw)) {
				if (shellState.drawerKind !== null) closeDrawer();
				return;
			}
			if (!Number.isFinite(ridParsed)) {
				if (shellState.drawerKind !== null) closeDrawer();
				return;
			}
			if (shellState.drawerKind !== raw || shellState.drawerBandId !== ridParsed) {
				openDrawer(ridParsed, raw);
			}
		});
	});

	$effect(() => {
		const sid = page.params.sessionId;
		untrack(() => {
			if (!sid && shellState.drawerKind !== null) closeDrawer();
		});
	});

	$effect(() => {
		const k = shellState.drawerKind;
		const id = shellState.drawerBandId;
		untrack(() => {
			syncUrlFromState(k, id);
		});
	});

	function syncUrlFromState(nextKind: DrawerKind | null, nextBandId: number | null): void {
		const sid = page.params.sessionId;
		if (!sid) return;
		const url = new URL(page.url);
		if (nextKind === null || nextBandId === null) {
			if (!url.searchParams.has('drawer')) return;
			url.searchParams.delete('drawer');
			{
				/* eslint-disable-next-line svelte/no-navigation-without-resolve -- rewriting the current page's own already-resolved pathname + a modified query string, not navigating to a named route; not expressible as a typed SvelteKit route */
			}
			void goto(url.pathname + url.search, {
				replaceState: true,
				keepFocus: true,
				noScroll: true
			});
			return;
		}
		const ridRaw = page.params.requestId;
		if (!ridRaw) return;
		const ridParsed = Number(ridRaw);
		const targetPath =
			Number.isFinite(ridParsed) && ridParsed === nextBandId
				? url.pathname
				: `/sessions/${encodeURIComponent(sid)}/requests/${nextBandId}`;
		url.searchParams.set('drawer', nextKind);
		const target = targetPath + url.search;
		if (target === page.url.pathname + page.url.search) return;
		{
			/* eslint-disable-next-line svelte/no-navigation-without-resolve -- target mixes the current page's own pathname (or a computed sessions/requests path) with a modified query string; not a single named route */
		}
		void goto(target, {
			replaceState: true,
			keepFocus: true,
			noScroll: true
		});
	}

	function setOpen(next: boolean): void {
		if (next) return;
		closeDrawer();
	}

	const drawerTitle = $derived.by(() => {
		switch (kind) {
			case 'request':
				return 'Request body';
			case 'response':
				return 'Response body';
			case 'events':
				return 'SSE events';
			case 'findings':
				return 'Findings';
			default:
				return '';
		}
	});

	const eventsCount = $derived.by(() => {
		const d = detail;
		if (!d) return 0;
		let n = 0;
		if ((d.accumulated_text ?? '').length > 0) n += 1;
		if ((d.accumulated_thinking ?? '').length > 0) n += 1;
		n += d.tool_calls?.length ?? 0;
		return n;
	});

	function parseToolInput(inputJson: string): unknown {
		try {
			return JSON.parse(inputJson);
		} catch {
			return inputJson;
		}
	}
</script>

<Dialog.Root {open} onOpenChange={setOpen}>
	<Dialog.Content
		scrollBody={false}
		aria-describedby={undefined}
		class="drawer-content !max-w-3xl !p-0 !gap-0 !flex !flex-col !max-h-[85vh] !overflow-hidden"
		style="--edge: var(--edge-sm);"
	>
		<div class="drawer-head">
			<Dialog.Title>{drawerTitle}</Dialog.Title>
			{#if bandId !== null}
				<p class="drawer-sub">
					{#if detail?.request_number != null}
						<span class="font-mono tabular-nums" title="Global id: #{bandId}"
							>#{detail.request_number}<span class="text-[var(--text-3)]"
								>&nbsp;·&nbsp;{bandId}</span
							></span
						>
					{:else}
						<span class="font-mono">#{bandId}</span>
					{/if}
				</p>
			{/if}
		</div>

		<div class="drawer-body">
			{#if detailLoadFailed}
				<div class="drawer-error">
					<p class="hint">Failed to load request detail.</p>
					<span class="drawer-error__retry-backing">
						<button
							type="button"
							class="drawer-error__retry"
							data-motion="hover"
							data-augmented-ui="tr-clip bl-clip border"
							onclick={retryDetail}
						>
							Retry
						</button>
					</span>
				</div>
			{:else if detail === null}
				<p class="hint">Loading detail…</p>
			{:else if kind === 'request'}
				<RequestBodyCard {detail} fill initialTab={requestBodyInitialTab} />
			{:else if kind === 'response'}
				<ResponseBodyCard {detail} fill />
			{:else if kind === 'findings'}
				<DiagnosticsCard diagnostics={detail.diagnostics ?? []} requestId={bandId} />
			{:else if kind === 'events'}
				{#if eventsCount === 0}
					<p class="hint">No SSE events recorded for this request.</p>
				{:else}
					<div class="events">
						{#if (detail.accumulated_text ?? '').length > 0}
							<section>
								<h3 class="section-label">Accumulated text</h3>
								<div class="code">
									<div class="bar"><span class="lang">text</span></div>
									<pre>{detail.accumulated_text}</pre>
								</div>
							</section>
						{/if}

						{#if (detail.accumulated_thinking ?? '').length > 0}
							<section>
								<h3 class="section-label">Accumulated thinking</h3>
								<div class="code">
									<div class="bar"><span class="lang">thinking</span></div>
									<pre>{detail.accumulated_thinking}</pre>
								</div>
							</section>
						{/if}

						{#if detail.tool_calls && detail.tool_calls.length > 0}
							<section>
								<h3 class="section-label">
									Tool calls ({detail.tool_calls.length})
								</h3>
								<ul class="tool-calls">
									{#each detail.tool_calls as call, idx (idx)}
										<li>
											{#if !call.complete}
												<p class="hint">{call.name} (incomplete)</p>
											{/if}
											<ToolCallCard name={call.name} input={parseToolInput(call.input_json)} />
										</li>
									{/each}
								</ul>
							</section>
						{/if}
					</div>
				{/if}
			{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>

<style>
	.drawer-head {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 14px 52px 12px 18px;
		border-bottom: 1px solid var(--border-default);
	}
	.drawer-head :global([data-slot='dialog-title']) {
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--track-machine, 0.08em);
		color: var(--text-1);
	}
	.drawer-sub {
		font: 11px var(--font-mono);
		color: var(--text-2);
		margin: 0;
	}
	.drawer-body {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 14px 18px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.hint {
		font-size: 12px;
		color: var(--text-3);
		font-style: italic;
		margin: 0;
	}
	.drawer-error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
	}
	.drawer-error__retry-backing {
		align-self: flex-start;
		display: inline-flex;
		background: var(--surface-card);
	}
	.drawer-error__retry {
		--edge: 6px;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-tl: 0px;
		--aug-br: 0px;
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		padding: 6px 14px;
		background: var(--surface-2);
		color: var(--text-1);
		font-family: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	.drawer-error__retry:hover {
		color: var(--text-strong);
		--aug-border-bg: var(--interactive-active-border);
	}
	.drawer-error__retry:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}
	.events {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.tool-calls {
		display: flex;
		flex-direction: column;
		gap: 8px;
		list-style: none;
		padding: 0;
		margin: 0;
	}
</style>
