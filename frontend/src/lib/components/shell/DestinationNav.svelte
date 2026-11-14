<script lang="ts">
	import { page } from '$app/state';
	import { resolveStatic } from '$lib/resolveHref';
	import {
		House,
		Layers,
		Filter,
		Radar,
		ShieldCheck,
		LineChart,
		BookText,
		Users,
		Wrench,
		Activity,
		Cpu,
		Boxes,
		ArrowDownWideNarrow,
		ScanText,
		ScanSearch,
		Plug,
		Settings,
		HelpCircle,
		ChevronsLeft,
		Network,
		FolderGit2,
		BrainCircuit,
		MessagesSquare,
		Bot,
		Scale,
		ListChecks,
		Sparkles,
		BellRing
	} from '@lucide/svelte';
	import { BookOpen, FilePen, ListTodo } from '@lucide/svelte';
	import type { Component } from 'svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import { toggleSessionsCollapsed } from '$lib/stores/shellState.svelte';

	let { showCollapse = true, onNavigate }: { showCollapse?: boolean; onNavigate?: () => void } =
		$props();

	type IconComponent = Component<{
		size?: number | string;
		class?: string;
		'aria-hidden'?: 'true' | boolean;
	}>;

	type NavHref =
		| '/'
		| '/projects'
		| '/sessions'
		| '/filter/workbench'
		| '/steering'
		| '/steerers'
		| '/analysis'
		| '/sessions/improvement'
		| '/sessions/improvement/guard'
		| '/sessions/labelling'
		| '/sessions/labelling/accuracy'
		| '/prompts'
		| '/agents'
		| '/fleet'
		| '/comms'
		| '/threads'
		| '/tools'
		| '/code-findings'
		| '/compression-review'
		| '/diagnostics/agent-dispatches'
		| '/memory'
		| '/skills'
		| '/proposals'
		| '/diagnostics/memory'
		| '/settings'
		| '/settings/steering'
		| '/settings/workers'
		| '/settings/assistants'
		| '/settings/embedding'
		| '/settings/reranker'
		| '/settings/flagging'
		| '/settings/integrations'
		| '/help';

	interface NavItem {
		label: string;
		href: NavHref;
		icon: IconComponent;
		hint: string;
		indent?: boolean;
	}

	interface NavGroup {
		caption: string;
		accent: 'overview' | 'levers' | 'insight' | 'system';
		items: NavItem[];
	}

	const SHAPE: Record<NavGroup['accent'], string | undefined> = {
		overview: 'tl-clip',
		levers: undefined,
		insight: undefined,
		system: 'bl-clip'
	};

	const NAV: NavGroup[] = [
		{
			caption: 'Overview',
			accent: 'overview',
			items: [
				{ label: 'Home', href: '/', icon: House, hint: 'Home — overview + active sessions' },
				{
					label: 'Projects',
					href: '/projects',
					icon: FolderGit2,
					hint: 'Every codebase or workspace captured on the wire'
				},
				{
					label: 'Sessions',
					href: '/sessions',
					icon: Layers,
					hint: 'All captured sessions, grouped by project'
				},
				{
					label: 'What the judge did',
					href: '/sessions/improvement',
					icon: Sparkles,
					hint: 'The lessons, nudges and directives the judge’s labels drove, what waits for you, and whether corrections are falling',
					indent: true
				},
				{
					label: 'Guard nudges',
					href: '/sessions/improvement/guard',
					icon: BellRing,
					hint: 'Every note the guard added or wake it sent, one panel per question, with your right or wrong on each',
					indent: true
				},
				{
					label: 'Judge',
					href: '/sessions/labelling',
					icon: Scale,
					hint: 'How the judge labels your turns and sessions, how accurate it is, what it costs',
					indent: true
				},
				{
					label: 'Judge accuracy',
					href: '/sessions/labelling/accuracy',
					icon: ListChecks,
					hint: 'Accuracy runs, run by run',
					indent: true
				},
				{
					label: 'Comms',
					href: '/comms',
					icon: MessagesSquare,
					hint: 'Every mailbox thread across every session and project'
				},
				{
					label: 'Threads',
					href: '/threads',
					icon: ListTodo,
					hint: 'The work each project has under way: state, acceptance criteria, history and sessions'
				}
			]
		},
		{
			caption: 'Levers',
			accent: 'levers',
			items: [
				{
					label: 'Filters',
					href: '/filter/workbench',
					icon: Filter,
					hint: 'Rewrite outbound system prompts before they reach Anthropic'
				},
				{
					label: 'Steering Monitor',
					href: '/steering',
					icon: Radar,
					hint: 'Live steering monitor — cross-session steerer activity'
				},
				{
					label: 'Steerers',
					href: '/steerers',
					icon: ShieldCheck,
					hint: 'Manage steerers, enrolments, and configuration'
				}
			]
		},
		{
			caption: 'Insight',
			accent: 'insight',
			items: [
				{
					label: 'Flagged responses',
					href: '/analysis',
					icon: LineChart,
					hint: 'Responses flagged for unbacked claims, blame-shifting, and avoided work'
				},
				{
					label: 'Prompts',
					href: '/prompts',
					icon: BookText,
					hint: 'Captured system-prompt compositions'
				},
				{
					label: 'Agents',
					href: '/agents',
					icon: Users,
					hint: 'Claude agents observed in captured sessions'
				},
				{
					label: 'Fleet',
					href: '/fleet',
					icon: Network,
					hint: 'Session fleet canvas — captured sessions and the messages linking them'
				},
				{
					label: 'Compression review',
					href: '/compression-review',
					icon: Wrench,
					hint: 'Flagged requests, responses and tool results for compression investigation'
				},
				{
					label: 'Tools',
					href: '/tools',
					icon: Wrench,
					hint: 'Cross-session tool-call feed — every tool use captured, filterable by name and project, errors marked inline'
				},
				{
					label: 'Code findings',
					href: '/code-findings',
					icon: ScanSearch,
					hint: 'Which code rules fire, whether sessions are told, and whether they fix what they are told'
				},
				{
					label: 'Agent dispatches',
					href: '/diagnostics/agent-dispatches',
					icon: Users,
					hint: 'Cross-session list of sub-agent spawns (Task tool calls)',
					indent: true
				}
			]
		},
		{
			caption: 'System',
			accent: 'system',
			items: [
				{
					label: 'Memory',
					href: '/memory',
					icon: BrainCircuit,
					hint: 'What memory consolidation has done to your lessons — promotions, archives, and restores'
				},
				{
					label: 'Skills',
					href: '/skills',
					icon: BookOpen,
					hint: 'The skills mike writes into your projects from their lessons, and your say over them'
				},
				{
					label: 'Proposals',
					href: '/proposals',
					icon: FilePen,
					hint: 'Lessons that belong in a rule or an agent definition, for you to accept or decline'
				},
				{
					label: 'Memory diagnostics',
					href: '/diagnostics/memory',
					icon: Activity,
					hint: 'Memory diagnostics — RSS, store byte budget, eviction pressure'
				},
				{ label: 'Settings', href: '/settings', icon: Settings, hint: 'System feature toggles' },
				{
					label: 'Steering',
					href: '/settings/steering',
					icon: Radar,
					hint: 'Steerer fleet configuration — model, thresholds, sub-judges',
					indent: true
				},
				{
					label: 'Workers',
					href: '/settings/workers',
					icon: Cpu,
					hint: 'Background workers — lesson worker, harvester, retention sweep, DB resync',
					indent: true
				},
				{
					label: 'Assistants',
					href: '/settings/assistants',
					icon: Bot,
					hint: 'Prompt assistant — model, what it can read, budget',
					indent: true
				},
				{
					label: 'Embedding',
					href: '/settings/embedding',
					icon: Boxes,
					hint: 'Embedding model + endpoint for memory recall',
					indent: true
				},
				{
					label: 'Reranker',
					href: '/settings/reranker',
					icon: ArrowDownWideNarrow,
					hint: 'Reranker model, endpoint, search ranking and the overnight pass that tunes it',
					indent: true
				},
				{
					label: 'Flagging',
					href: '/settings/flagging',
					icon: ScanText,
					hint: 'Flags unbacked claims, blame-shifting, and avoided work in model responses; can inject a live correction',
					indent: true
				},
				{
					label: 'Integrations',
					href: '/settings/integrations',
					icon: Plug,
					hint: 'External services this app talks to — OpenRouter account, balance, and per-model traffic captured on the wire',
					indent: true
				},
				{ label: 'Help', href: '/help', icon: HelpCircle, hint: 'Docs, glossary, and guides' }
			]
		}
	];

	const currentPath = $derived(page.url.pathname);

	const ALL_HREFS = NAV.flatMap((g) => g.items.map((i) => i.href));

	const REVIEW = /^\/sessions\/labelling\/accuracy\/[^/]+\/review(\/|$)/;

	function isActive(href: NavHref): boolean {
		if (REVIEW.test(currentPath)) return href === '/sessions/labelling/accuracy';
		if (href === '/') {
			return currentPath === '/';
		}
		if (currentPath === href) {
			return true;
		}
		if (!currentPath.startsWith(href + '/')) {
			return false;
		}
		return !ALL_HREFS.some(
			(other) =>
				other !== href &&
				other.length > href.length &&
				(currentPath === other || currentPath.startsWith(other + '/'))
		);
	}

	function isAncestor(href: NavHref): boolean {
		return href !== '/' && !isActive(href) && currentPath.startsWith(href + '/');
	}
</script>

<nav class="dn col" class:dn--drawer={!showCollapse} aria-label="Destinations">
	<div class="col-body dn-body">
		{#each NAV as group (group.caption)}
			<div class="dn-group" data-accent={group.accent} data-augmented-ui={SHAPE[group.accent]}>
				<div class="dn-caption">{group.caption}</div>
				<ul class="dn-list" role="list">
					{#each group.items as item (item.href)}
						{@const active = isActive(item.href)}
						{@const ancestor = isAncestor(item.href)}
						<li>
							<Tooltip text={item.hint} placement="right">
								{#snippet child({ props })}
									<!-- eslint-disable svelte/no-navigation-without-resolve -- every NavHref is a parameterless route; resolveStatic() resolves it, because resolve() cannot take a union this wide -->
									<a
										{...props}
										href={resolveStatic(item.href)}
										class="dn-item"
										class:dn-item--active={active}
										class:dn-item--ancestor={ancestor}
										class:dn-item--indent={item.indent}
										data-augmented-ui="br-clip border"
										aria-current={active ? 'page' : undefined}
										onclick={onNavigate}
									>
										<span class="dn-item-icon" aria-hidden="true">
											<item.icon size={15} aria-hidden="true" />
										</span>
										<span class="dn-item-label">{item.label}</span>
									</a>
									<!-- eslint-enable svelte/no-navigation-without-resolve -->
								{/snippet}
							</Tooltip>
						</li>
					{/each}
				</ul>
			</div>
		{/each}
	</div>

	{#if showCollapse}
		<div class="col-foot">
			<Tooltip text="Collapse the navigation column">
				{#snippet child({ props })}
					<button
						{...props}
						type="button"
						class="col-foot__btn"
						aria-label="Collapse navigation column"
						onclick={toggleSessionsCollapsed}
					>
						<ChevronsLeft size={14} aria-hidden="true" />
					</button>
				{/snippet}
			</Tooltip>
		</div>
	{/if}
</nav>

<style>
	.dn {
		background: var(--surface-sidebar);
	}
	.dn.dn--drawer {
		position: static;
		min-height: 0;
		max-height: none;
		border: 0;
	}
	.dn--drawer .dn-item {
		min-height: var(--control-h-lg);
	}

	.col-foot {
		border-top: 0;
		background: transparent;
	}

	.dn-body {
		display: flex;
		flex-direction: column;
		padding: var(--space-2, 8px) 0 var(--space-3, 12px);
		flex: 1 1 auto;
		min-height: 0;
		overflow-y: auto;
	}

	.dn-group {
		display: flex;
		flex-direction: column;
		--edge: var(--edge-md);
		--edge-pad: calc(var(--edge) + var(--space-1, 4px));
		--aug-tl: var(--edge);
		--aug-bl: var(--edge);
		padding: var(--edge-pad) 0 var(--space-3, 12px);
		background: linear-gradient(105deg, var(--acc-dim), transparent 18%);
	}
	.dn-group:last-child {
		flex: 1 1 auto;
	}
	.dn-group[data-accent='overview'] {
		--acc: var(--status-success-fg);
		--acc-dim: var(--status-success-bg);
		--acc-border: var(--status-success-border);
	}
	.dn-group[data-accent='levers'] {
		--acc: var(--status-danger-fg);
		--acc-dim: var(--status-danger-bg);
		--acc-border: var(--status-danger-border);
	}
	.dn-group[data-accent='insight'] {
		--acc: var(--data-teal-fg);
		--acc-dim: var(--data-teal-bg);
		--acc-border: var(--data-teal-border);
	}
	.dn-group[data-accent='system'] {
		--acc: var(--status-info-fg);
		--acc-dim: var(--status-info-bg);
		--acc-border: var(--status-info-border);
	}

	.dn-caption {
		padding: 0 var(--edge-pad) var(--space-2) var(--edge-pad);
		font-family: var(--font-machine);
		font-size: var(--text-xs, 11px);
		font-weight: var(--weight-medium, 500);
		letter-spacing: var(--track-machine, 0.08em);
		text-transform: uppercase;
		color: var(--acc, var(--text-3));
	}

	.dn-group {
		animation: dn-group-in var(--dur-2) var(--ease-io) both;
	}
	@keyframes dn-group-in {
		from {
			opacity: 0;
			transform: translateX(calc(-1 * var(--space-3)));
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.dn-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
	}

	.dn-item {
		--aug-br: var(--edge-sm);
		--aug-border-all: var(--hairline);
		--aug-border-left: 2px;
		--aug-border-bg: transparent;
		position: relative;
		display: flex;
		align-items: center;
		gap: var(--space-2, 8px);
		min-height: var(--control-h);
		padding: var(--space-2) var(--edge-pad);
		text-decoration: none;
		color: var(--text-2);
		transition:
			color var(--dur-1) ease,
			background var(--dur-1) ease;
	}
	.dn-item:hover {
		color: var(--text-strong);
	}
	.dn-item:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--ring);
	}

	.dn-item--active,
	.dn-item--active:hover {
		color: var(--funk-cyan);
		background:
			linear-gradient(0deg, var(--funk-cyan-dim), var(--funk-cyan-dim)), var(--surface-sidebar);
		text-shadow: var(--glow-cyan);
		--aug-br: var(--edge-sm);
		--aug-border-all: 1px;
		--aug-border-left: 2px;
		--aug-border-bg: var(--funk-cyan-border);
	}
	.dn-item--ancestor {
		color: var(--acc);
	}
	.dn-item--ancestor .dn-item-icon {
		color: var(--acc);
	}

	.dn-item--active .dn-item-icon {
		color: var(--funk-cyan);
	}
	.dn-item--indent {
		padding-left: var(--space-6);
	}

	.dn-item-icon {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: var(--space-4, 16px);
		color: var(--text-3);
		transition: color var(--dur-1) ease;
	}
	.dn-item:hover .dn-item-icon {
		color: var(--text-strong);
	}

	.dn-item-label {
		font-size: var(--text-sm, 12px);
		font-weight: 500;
		overflow-wrap: anywhere;
		min-width: 0;
	}
</style>
