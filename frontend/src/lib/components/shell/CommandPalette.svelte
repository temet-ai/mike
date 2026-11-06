<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import {
		AlertTriangle,
		BookOpen,
		Braces,
		Compass,
		ExternalLink,
		FileText,
		Filter,
		FolderGit2,
		HardDrive,
		History,
		Home,
		LineChart,
		MessageSquare,
		Pin,
		Search,
		Settings,
		X,
		ArrowUp,
		ArrowDown,
		CornerDownLeft,
		Sparkles,
		Star,
		Users
	} from '@lucide/svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { shellState } from '$lib/stores/shellState.svelte';
	import { match } from '$lib/utils/fuzzyMatch';
	import { listSessions, ApiError } from '$lib/api';
	import type { SessionSummary } from '$lib/types';
	import { shortSessionId } from '$lib/format';
	import { SETTINGS_DESTINATIONS } from '$lib/settings/destinations';
	import { FEATURE_COPY, OTHER_PAGE_FEATURES } from '$lib/settings/featureCopy';
	import { REPO_BASE, PROJECT_DOCS } from '$lib/help/externalDocs';

	type SectionId = 'sessions' | 'recent' | 'pages' | 'actions' | 'help';

	const SESSION_SEARCH_MIN_QUERY = 2;
	const SESSION_SEARCH_DEBOUNCE_MS = 200;
	const SESSION_SEARCH_MAX_HITS = 50;

	interface BaseEntry {
		id: string;
		section: SectionId;
		title: string;
		description: string;
		icon: typeof Home;
		keywords?: string;
		search?: string;
		disabled?: boolean;
		disabledReason?: string;
	}

	type ActionFn = () => void | Promise<void>;

	type KnownPathname =
		| '/'
		| '/projects'
		| '/sessions'
		| '/agents'
		| '/prompts'
		| '/analysis'
		| '/tools#errors'
		| '/diagnostics/agent-dispatches'
		| '/diagnostics/memory'
		| '/prompts/volatile-lines'
		| '/help/glossary'
		| (typeof SETTINGS_DESTINATIONS)[number]['href']
		| `/sessions/${string}`;

	type Entry =
		| (BaseEntry & { kind: 'route'; href: KnownPathname })
		| (BaseEntry & { kind: 'action'; run: ActionFn })
		| (BaseEntry & { kind: 'external'; href: string });

	const RECENT_STORAGE_KEY = 'mike:cmdk-recent';
	const RECENT_MAX = 5;

	interface RecentEntry {
		id: string;
		title: string;
		description: string;
		href: KnownPathname;
		search?: string;
	}

	function readRecent(): RecentEntry[] {
		if (!browser) return [];
		try {
			const raw = window.localStorage.getItem(RECENT_STORAGE_KEY);
			if (!raw) return [];
			const parsed = JSON.parse(raw) as unknown;
			if (!Array.isArray(parsed)) return [];
			return parsed
				.filter(
					(r): r is RecentEntry =>
						r !== null &&
						typeof r === 'object' &&
						typeof (r as RecentEntry).id === 'string' &&
						typeof (r as RecentEntry).title === 'string' &&
						typeof (r as RecentEntry).description === 'string' &&
						typeof (r as RecentEntry).href === 'string'
				)
				.slice(0, RECENT_MAX);
		} catch {
			return [];
		}
	}

	function writeRecent(list: RecentEntry[]): void {
		if (!browser) return;
		try {
			window.localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify(list.slice(0, RECENT_MAX)));
		} catch {
			// Storage may be unavailable; keep the current in-memory state.
		}
	}

	let recentList = $state<RecentEntry[]>([]);

	function refreshRecentFromStorage(): void {
		recentList = readRecent();
	}

	function pushRecent(entry: RecentEntry): void {
		const dedup = recentList.filter((r) => r.id !== entry.id);
		const next = [entry, ...dedup].slice(0, RECENT_MAX);
		recentList = next;
		writeRecent(next);
	}

	const SETTINGS_ENTRIES: Entry[] = [
		...SETTINGS_DESTINATIONS.map<Entry>((d) => ({
			kind: 'route',
			id: `settings:${d.href}`,
			section: 'pages',
			title: d.href === '/settings' ? 'Settings' : `${d.label} settings`,
			description: d.href === '/settings' ? 'General settings and switches' : 'Settings tab',
			keywords: 'settings',
			icon: d.icon,
			href: d.href
		})),
		...[...Object.values(FEATURE_COPY), ...Object.values(OTHER_PAGE_FEATURES)].map<Entry>((f) => ({
			kind: 'route',
			id: `setting:${f.label}`,
			section: 'pages',
			title: f.label,
			description: f.description,
			keywords: `settings switch ${f.find ?? ''}`,
			icon: Settings,
			href: '/settings',
			search: `?find=${encodeURIComponent(f.label)}`
		}))
	];

	const PAGES: ReadonlyArray<{
		id: string;
		title: string;
		description: string;
		href: KnownPathname;
		icon: typeof Home;
	}> = [
		{
			id: 'page:home',
			title: 'Home',
			description: 'Captured requests across active sessions.',
			href: '/',
			icon: Home
		},
		{
			id: 'page:projects',
			title: 'Projects',
			description: 'Every codebase or workspace captured on the wire.',
			href: '/projects',
			icon: FolderGit2
		},
		{
			id: 'page:sessions',
			title: 'Sessions',
			description: 'Active and recent Claude Code sessions.',
			href: '/sessions',
			icon: MessageSquare
		},
		{
			id: 'page:agents',
			title: 'Agents',
			description: 'Spawned Claude agents in captured sessions.',
			href: '/agents',
			icon: Users
		},
		{
			id: 'page:prompts',
			title: 'Prompts',
			description: 'System-prompt compositions captured from recent requests.',
			href: '/prompts',
			icon: FileText
		},
		{
			id: 'page:analysis',
			title: 'Analysis',
			description: 'Weasel-pattern findings across captured request bodies.',
			href: '/analysis',
			icon: LineChart
		},
		{
			id: 'page:tool-errors',
			title: 'Tool errors',
			description: 'Errored tool calls — the errors-only view of the Tools feed.',
			href: '/tools#errors',
			icon: AlertTriangle
		},
		{
			id: 'page:agent-dispatches',
			title: 'Agent dispatches',
			description: 'Cross-session list of sub-agent spawns (Task tool calls).',
			href: '/diagnostics/agent-dispatches',
			icon: Users
		},
		{
			id: 'page:memory',
			title: 'Memory diagnostics',
			description: 'Live RSS, store byte budget, and SPA eviction pressure.',
			href: '/diagnostics/memory',
			icon: HardDrive
		},
		{
			id: 'page:volatile-lines',
			title: 'Volatile lines',
			description: 'Regex catalogue for session-derived volatile prompt lines.',
			href: '/prompts/volatile-lines',
			icon: Braces
		},
		{
			id: 'page:glossary',
			title: 'Glossary',
			description: 'Vocabulary used in this app.',
			href: '/help/glossary',
			icon: BookOpen
		}
	];

	const HELP: ReadonlyArray<{
		id: string;
		title: string;
		description: string;
		href: string;
		icon: typeof ExternalLink;
		external?: boolean;
	}> = [
		...PROJECT_DOCS.map((document) => ({
			id: `help:${document.href}`,
			title: document.label,
			description: document.description,
			href: document.href,
			icon: BookOpen
		})),
		{
			id: 'help:repo',
			title: 'Open GitHub repo',
			description: 'Source code, releases, contribution notes.',
			href: REPO_BASE,
			icon: ExternalLink,
			external: true
		},
		{
			id: 'help:glossary',
			title: 'Open glossary',
			description: 'In-app vocabulary reference.',
			href: '/help/glossary',
			icon: BookOpen
		}
	];

	const currentSessionId = $derived(page.params['sessionId'] ?? null);

	function buildActions(): Entry[] {
		const actions: Entry[] = [];

		actions.push({
			kind: 'action',
			id: 'action:help-drawer',
			section: 'actions',
			title: 'Open Help drawer',
			description: 'Documentation, repository, and capture diagnostics.',
			icon: Compass,
			run: () => {
				shellState.helpDrawerOpen = true;
			}
		});

		actions.push({
			kind: 'action',
			id: 'action:pin-session',
			section: 'actions',
			title: 'Pin current session',
			description: currentSessionId ? 'Pin the session you are viewing.' : 'Open a session first.',
			icon: Pin,
			disabled: currentSessionId === null,
			disabledReason: 'Available on session detail pages.',
			run: async () => {
				if (currentSessionId === null) return;
				try {
					const { patchSessionPin } = await import('$lib/api');
					await patchSessionPin(currentSessionId, true);
				} catch {
					// A failed pin request must not optimistically mark the session as pinned.
				}
			}
		});

		actions.push({
			kind: 'action',
			id: 'action:filter-workbench',
			section: 'actions',
			title: 'Open filter workbench',
			description: 'Author and manage filter rules on the dedicated workbench page.',
			icon: Filter,
			run: () => {
				void goto(resolve('/filter/workbench'));
			}
		});

		return actions;
	}

	const actions = $derived(buildActions());

	const allEntries = $derived<Entry[]>([
		...recentList.map<Entry>((r) => ({
			kind: 'route',
			id: `recent:${r.id}`,
			section: 'recent',
			title: r.title,
			description: r.description,
			icon: History,
			href: r.href,
			search: r.search
		})),
		...PAGES.map<Entry>((p) => ({
			kind: 'route',
			id: p.id,
			section: 'pages',
			title: p.title,
			description: p.description,
			icon: p.icon,
			href: p.href
		})),
		...SETTINGS_ENTRIES,
		...actions,
		...HELP.map<Entry>((h) =>
			h.external
				? {
						kind: 'external',
						id: h.id,
						section: 'help',
						title: h.title,
						description: h.description,
						icon: h.icon,
						href: h.href
					}
				: {
						kind: 'route',
						id: h.id,
						section: 'help',
						title: h.title,
						description: h.description,
						icon: h.icon,
						href: h.href as KnownPathname
					}
		)
	]);

	let query = $state('');

	const queryTrimmed = $derived(query.trim());

	let sessionHits = $state<SessionSummary[]>([]);
	let sessionSearchLoading = $state(false);
	let sessionSearchError = $state<string | null>(null);
	let sessionSearchTimer: ReturnType<typeof setTimeout> | null = null;
	let sessionSearchSeq = 0;

	function clearSessionSearchTimer(): void {
		if (sessionSearchTimer !== null) {
			clearTimeout(sessionSearchTimer);
			sessionSearchTimer = null;
		}
	}

	function resetSessionSearch(): void {
		clearSessionSearchTimer();
		sessionHits = [];
		sessionSearchLoading = false;
		sessionSearchError = null;
		sessionSearchSeq += 1;
	}

	async function runSessionSearch(rawQuery: string, seq: number): Promise<void> {
		try {
			const fetched = await listSessions({
				search: rawQuery,
				limit: SESSION_SEARCH_MAX_HITS
			});
			if (seq !== sessionSearchSeq) return;
			sessionHits = fetched.slice(0, SESSION_SEARCH_MAX_HITS);
			sessionSearchError = null;
		} catch (err) {
			if (seq !== sessionSearchSeq) return;
			sessionSearchError = err instanceof ApiError ? err.message : String(err);
			sessionHits = [];
		} finally {
			if (seq === sessionSearchSeq) {
				sessionSearchLoading = false;
			}
		}
	}

	$effect(() => {
		const q = queryTrimmed;
		if (q.length < SESSION_SEARCH_MIN_QUERY) {
			untrack(() => resetSessionSearch());
			return;
		}
		untrack(() => {
			clearSessionSearchTimer();
			sessionSearchTimer = setTimeout(() => {
				sessionSearchTimer = null;
				const seq = ++sessionSearchSeq;
				sessionSearchLoading = true;
				void runSessionSearch(q, seq);
			}, SESSION_SEARCH_DEBOUNCE_MS);
		});
	});

	const sessionHitEntries = $derived<Entry[]>(
		sessionHits.map((sess) => {
			const title = shortSessionId(sess.id);
			const descParts: string[] = [];
			if (sess.project) descParts.push(sess.project);
			descParts.push(`${sess.request_count} request${sess.request_count === 1 ? '' : 's'}`);
			return {
				kind: 'route',
				id: `session:${sess.id}`,
				section: 'sessions',
				title,
				description: descParts.join(' · '),
				icon: MessageSquare,
				href: `/sessions/${encodeURIComponent(sess.id)}` as KnownPathname
			};
		})
	);

	const visibleEntries = $derived.by(() => {
		const q = queryTrimmed;
		if (q.length === 0) return allEntries;
		const fuzzy = match(
			q,
			allEntries,
			(e) => `${e.title} ${e.description} ${e.keywords ?? ''}`
		).map((r) => r.item);
		return [...sessionHitEntries, ...fuzzy];
	});

	const groupedSections = $derived.by<Array<{ id: SectionId; label: string; entries: Entry[] }>>(
		() => {
			const order: Array<{ id: SectionId; label: string }> = [
				{ id: 'recent', label: 'Recent' },
				{ id: 'pages', label: 'Pages' },
				{ id: 'actions', label: 'Actions' },
				{ id: 'help', label: 'Help' }
			];

			if (queryTrimmed.length > 0) {
				const sections: Array<{ id: SectionId; label: string; entries: Entry[] }> = [];
				const sessionEntries = visibleEntries.filter((e) => e.section === 'sessions');
				const fuzzyEntries = visibleEntries.filter((e) => e.section !== 'sessions');
				if (sessionEntries.length > 0) {
					sections.push({
						id: 'sessions',
						label: `Sessions · ${sessionEntries.length}`,
						entries: sessionEntries
					});
				}
				if (fuzzyEntries.length > 0) {
					sections.push({
						id: 'pages',
						label: `Results · ${fuzzyEntries.length}`,
						entries: fuzzyEntries
					});
				}
				return sections;
			}

			return order
				.map((o) => ({
					id: o.id,
					label: o.label,
					entries: visibleEntries.filter((e) => e.section === o.id)
				}))
				.filter((s) => s.entries.length > 0);
		}
	);

	const flatVisibleEntries = $derived.by<Entry[]>(() => {
		const out: Entry[] = [];
		for (const section of groupedSections) {
			for (const entry of section.entries) {
				out.push(entry);
			}
		}
		return out;
	});

	let selectedIndex = $state(0);
	let inputEl = $state<HTMLInputElement | null>(null);
	let resultsEl = $state<HTMLDivElement | null>(null);
	let listboxId = 'cmdk-listbox';

	function rowId(entry: Entry): string {
		return `cmdk-opt-${entry.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`;
	}

	function ensureSelectionInRange(): void {
		const len = flatVisibleEntries.length;
		if (len === 0) {
			selectedIndex = 0;
			return;
		}
		if (selectedIndex >= len) selectedIndex = 0;
		if (selectedIndex < 0) selectedIndex = len - 1;
	}

	$effect(() => {
		void flatVisibleEntries.length;
		untrack(() => ensureSelectionInRange());
	});

	function selectByDelta(delta: number): void {
		const len = flatVisibleEntries.length;
		if (len === 0) return;
		const next = (selectedIndex + delta + len) % len;
		selectedIndex = next;
		queueMicrotask(() => {
			const target = flatVisibleEntries[selectedIndex];
			if (!target) return;
			const el = document.getElementById(rowId(target));
			el?.scrollIntoView({ block: 'nearest' });
		});
	}

	async function activateEntry(entry: Entry): Promise<void> {
		if (entry.disabled) return;

		if (entry.kind === 'route' && entry.section !== 'recent') {
			pushRecent({
				id: entry.id,
				title: entry.title,
				description: entry.description,
				href: entry.href,
				search: entry.search
			});
		}

		shellState.commandPaletteOpen = false;

		if (entry.kind === 'route') {
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- Resolve the route before appending its saved search parameters.
			await goto(`${resolve(entry.href)}${entry.search ?? ''}`);
		} else if (entry.kind === 'external') {
			if (typeof window !== 'undefined') {
				window.open(entry.href, '_blank', 'noopener,noreferrer');
			}
		} else if (entry.kind === 'action') {
			await entry.run();
		}
	}

	function handleKeydown(event: KeyboardEvent): void {
		if (
			!shellState.commandPaletteOpen ||
			event.isComposing ||
			(event.target !== inputEl && event.target !== resultsEl)
		)
			return;

		switch (event.key) {
			case 'ArrowDown': {
				event.preventDefault();
				selectByDelta(1);
				break;
			}
			case 'ArrowUp': {
				event.preventDefault();
				selectByDelta(-1);
				break;
			}
			case 'Home': {
				if (event.target === inputEl) break;
				event.preventDefault();
				selectedIndex = 0;
				break;
			}
			case 'End': {
				if (event.target === inputEl) break;
				event.preventDefault();
				selectedIndex = flatVisibleEntries.length - 1;
				break;
			}
			case 'Enter': {
				event.preventDefault();
				const entry = flatVisibleEntries[selectedIndex];
				if (entry) void activateEntry(entry);
				break;
			}
			case 'Tab':
				break;
			default:
				break;
		}
	}

	function handleOpenChange(open: boolean): void {
		shellState.commandPaletteOpen = open;
	}

	$effect(() => {
		if (!shellState.commandPaletteOpen) {
			untrack(() => resetSessionSearch());
			return;
		}
		untrack(() => {
			refreshRecentFromStorage();
			query = '';
			selectedIndex = 0;
			resetSessionSearch();
		});
		void tick().then(() => {
			inputEl?.focus();
		});
	});

	const activeOptionId = $derived.by(() => {
		const entry = flatVisibleEntries[selectedIndex];
		return entry ? rowId(entry) : undefined;
	});
</script>

<svelte:window onkeydown={handleKeydown} />

<Dialog.Root open={shellState.commandPaletteOpen} onOpenChange={handleOpenChange}>
	<Dialog.Content class="!p-0 !gap-0 cmdk-content" scrollBody={false} showCloseButton={false}>
		<Dialog.Title class="sr-only">Command palette</Dialog.Title>
		<Dialog.Description class="sr-only"
			>Search pages, sessions, actions and help. Use the up and down arrow keys to choose a result,
			then Enter to open it.</Dialog.Description
		>

		<div class="cmdk-search">
			<Search class="size-4 text-[var(--text-3)]" aria-hidden="true" />
			<input
				bind:this={inputEl}
				bind:value={query}
				type="text"
				aria-label="Search pages, sessions and commands"
				role="combobox"
				placeholder="Search pages, sessions, actions…"
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
				aria-expanded="true"
				aria-controls={listboxId}
				aria-activedescendant={activeOptionId}
				aria-autocomplete="list"
			/>
			<Dialog.Close class="cmdk-close" aria-label="Close command palette">
				<X class="size-4" aria-hidden="true" />
			</Dialog.Close>
		</div>

		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			bind:this={resultsEl}
			class="cmdk-body"
			role="region"
			tabindex="0"
			aria-label="Command results"
		>
			{#if sessionSearchError !== null}
				<div class="cmdk-search-status" role="alert" data-slot="cmdk-search-error">
					Sessions search failed: {sessionSearchError}
				</div>
			{:else if sessionSearchLoading}
				<div class="cmdk-search-status" data-slot="cmdk-search-loading">Searching sessions…</div>
			{/if}
			{#if flatVisibleEntries.length === 0}
				<div class="cmdk-empty" role="status">
					<Sparkles class="size-3.5 text-[var(--text-3)]" aria-hidden="true" />
					<span>No matches for “{queryTrimmed}”.</span>
					<span class="cmdk-empty-hint">Try a page name, session ID or project.</span>
				</div>
			{/if}
			<ul id={listboxId} role="listbox" aria-label="Command palette results" class="cmdk-list">
				{#each groupedSections as section (section.id)}
					<li class="cmdk-section" role="presentation">
						<div class="cmdk-section-label" id={`cmdk-section-${section.id}`} role="presentation">
							{section.label}
						</div>
						<ul
							class="cmdk-section-list"
							role="group"
							aria-labelledby={`cmdk-section-${section.id}`}
						>
							{#each section.entries as entry (entry.id)}
								{@const flatIdx = flatVisibleEntries.indexOf(entry)}
								{@const isSelected = flatIdx === selectedIndex}
								<li
									id={rowId(entry)}
									role="option"
									aria-selected={isSelected}
									aria-disabled={entry.disabled || undefined}
									class="cmdk-row"
									class:cmdk-row--selected={isSelected}
									class:cmdk-row--disabled={entry.disabled}
									onmouseenter={() => {
										selectedIndex = flatIdx;
									}}
									onclick={() => {
										if (!entry.disabled) void activateEntry(entry);
									}}
									onkeydown={(e) => {
										if (e.key === 'Enter' || e.key === ' ') {
											if (!entry.disabled) {
												e.preventDefault();
												void activateEntry(entry);
											}
										}
									}}
								>
									<entry.icon class="size-4 cmdk-row-icon" aria-hidden="true" />
									<span class="cmdk-row-text">
										<span class="cmdk-row-title">
											<span>{entry.title}</span>
											{#if entry.kind === 'external'}
												<ExternalLink class="size-3 cmdk-row-external" aria-hidden="true" />
											{/if}
										</span>
										<span class="cmdk-row-desc">
											{entry.disabled && entry.disabledReason
												? entry.disabledReason
												: entry.description}
										</span>
									</span>
									{#if queryTrimmed.length > 0 && entry.section !== 'recent'}
										<span class="cmdk-row-tag">{entry.section}</span>
									{:else if entry.section === 'recent'}
										<Star class="size-3 cmdk-row-recent" aria-hidden="true" />
									{/if}
								</li>
							{/each}
						</ul>
					</li>
				{/each}
			</ul>
		</div>

		<div class="cmdk-foot" aria-hidden="true">
			<span class="cmdk-foot-keys">
				<kbd class="cmdk-kbd"><ArrowUp class="size-3" /></kbd><kbd class="cmdk-kbd"
					><ArrowDown class="size-3" /></kbd
				>
				navigate
			</span>
			<span class="cmdk-foot-keys">
				<kbd class="cmdk-kbd"><CornerDownLeft class="size-3" /></kbd>
				select
			</span>
			<span class="cmdk-foot-keys">
				<kbd class="cmdk-kbd">esc</kbd>
				close
			</span>
		</div>
	</Dialog.Content>
</Dialog.Root>

<style>
	:global(.cmdk-content) {
		display: flex;
		flex-direction: column;
		max-width: min(40rem, calc(100vw - 2 * var(--space-4)));
		max-height: min(80dvh, 40rem);
		overflow: hidden;
		scrollbar-gutter: auto;
	}

	.cmdk-search {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-card);
	}
	.cmdk-search input {
		flex: 1 1 auto;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: var(--text-strong);
		font-size: var(--text-lg);
		line-height: 1.4;
		padding: 0;
	}
	.cmdk-search input::placeholder {
		color: var(--text-3);
	}

	.cmdk-body {
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		scrollbar-gutter: stable;
		padding: var(--space-2) 0;
	}

	.cmdk-empty {
		flex-wrap: wrap;
		overflow-wrap: anywhere;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-5);
		font-size: var(--text-md);
		color: var(--text-2);
	}

	.cmdk-search-status {
		padding: var(--space-2) var(--space-4);
		font-size: var(--text-sm);
		color: var(--text-3);
		font-style: italic;
	}
	.cmdk-search-status[role='alert'] {
		color: var(--status-danger-fg);
	}

	.cmdk-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.cmdk-section {
		padding: var(--space-1) 0;
	}
	.cmdk-section + .cmdk-section {
		border-top: 1px solid var(--border-subtle);
	}
	.cmdk-section-label {
		padding: var(--space-2) var(--space-4) var(--space-1);
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-3);
	}
	.cmdk-section-list {
		list-style: none;
		margin: 0;
		padding: 0 var(--space-2);
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.cmdk-row {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--r-tol);
		cursor: pointer;
		transition:
			background var(--dur-1) var(--ease-snap),
			color var(--dur-1) var(--ease-snap);
	}
	.cmdk-row--selected {
		background: var(--funk-cyan-dim);
		box-shadow: inset 2px 0 0 var(--funk-cyan);
	}
	.cmdk-row--disabled {
		cursor: not-allowed;
		opacity: 0.55;
	}
	.cmdk-row--disabled.cmdk-row--selected {
		background: transparent;
	}
	.cmdk-row :global(.cmdk-row-icon) {
		flex: 0 0 auto;
		color: var(--text-3);
	}
	.cmdk-row--selected :global(.cmdk-row-icon) {
		color: var(--interactive-active-fg);
	}

	.cmdk-row-text {
		overflow-wrap: anywhere;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		min-width: 0;
		flex: 1 1 auto;
	}
	.cmdk-row-title {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-strong);
		font-size: var(--text-md);
	}
	.cmdk-row :global(.cmdk-row-external) {
		color: var(--text-3);
	}
	.cmdk-row-desc {
		font-size: var(--text-sm);
		color: var(--text-2);
	}
	.cmdk-row-tag {
		flex: 0 0 auto;
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		text-transform: uppercase;
		letter-spacing: var(--track-machine);
		color: var(--text-3);
		padding: var(--space-1) var(--space-2);
		border: 1px solid var(--border-subtle);
		border-radius: var(--r-tol);
	}
	.cmdk-row :global(.cmdk-row-recent) {
		flex: 0 0 auto;
		color: var(--interactive-active-fg);
		fill: var(--interactive-active-fg);
	}

	.cmdk-foot {
		flex-wrap: wrap;
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-4);
		border-top: 1px solid var(--border-subtle);
		background: var(--surface-card);
		font-size: var(--text-xs);
		color: var(--text-3);
	}
	.cmdk-foot-keys {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
	.cmdk-kbd {
		display: inline-flex;
		align-items: center;
		font-family: var(--font-machine);
		font-size: var(--text-xs);
		padding: var(--hairline) var(--space-1);
		border: 1px solid var(--border-default);
		border-radius: var(--r-tol);
		background: var(--surface-2);
		color: var(--text-2);
	}
	:global(.cmdk-close) {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: var(--control-h);
		height: var(--control-h);
		color: var(--text-2);
		border: var(--hairline) solid var(--border-default);
		border-radius: var(--r-tol);
		background: var(--surface-2);
		cursor: pointer;
	}
	.cmdk-empty-hint {
		flex-basis: 100%;
		color: var(--text-3);
		font-size: var(--text-sm);
	}
	@media (pointer: coarse) {
		:global(.cmdk-close) {
			min-height: calc(var(--control-h-lg) + var(--space-1));
			min-width: calc(var(--control-h-lg) + var(--space-1));
		}
	}
</style>
