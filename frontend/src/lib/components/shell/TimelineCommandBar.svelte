<script lang="ts">
	import {
		Search,
		X,
		List,
		MessageSquare,
		Wrench,
		AlertTriangle,
		GitBranch,
		CornerDownLeft,
		ChevronsUpDown,
		ChevronsDownUp,
		ChevronDown,
		ChevronUp,
		Check,
		UserRound
	} from '@lucide/svelte';
	import type { RoleFilter } from './timelineTurnView';

	interface ToolOption {
		value: string;
		label: string;
	}

	interface Props {
		query: string;
		role: RoleFilter;
		tool: string;
		toolOptions: ToolOption[];
		errorTurnCount: number;
		dispatchTurnCount: number;
		requestCount: number;
		turnCount: number;
		isLive: boolean;
		onJump: (n: number) => string | null;
		onExpandAll: () => void;
		onCollapseAll: () => void;
		operatorOnly: boolean;
		onToggleOperatorOnly: () => void;
		onHeightChange?: (px: number) => void;
	}

	let {
		query = $bindable(''),
		role = $bindable('all'),
		tool = $bindable('all'),
		toolOptions,
		errorTurnCount,
		dispatchTurnCount,
		requestCount,
		turnCount,
		isLive,
		onJump,
		onExpandAll,
		onCollapseAll,
		onHeightChange,
		operatorOnly,
		onToggleOperatorOnly
	}: Props = $props();

	let barHeight = $state(0);
	$effect(() => {
		onHeightChange?.(barHeight);
	});

	let sentinelEl: HTMLDivElement | undefined = $state();
	let docked = $state(false);
	$effect(() => {
		if (!sentinelEl || typeof IntersectionObserver === 'undefined') return;
		const observer = new IntersectionObserver(
			(entries) => {
				const entry = entries[0];
				if (entry) docked = !entry.isIntersecting;
			},
			{ threshold: 0 }
		);
		observer.observe(sentinelEl);
		return () => observer.disconnect();
	});

	const ROLES: Array<{ key: RoleFilter; label: string; icon: typeof List; title: string }> = [
		{ key: 'all', label: 'All', icon: List, title: 'Show everything' },
		{
			key: 'prompts',
			label: 'Prompts',
			icon: MessageSquare,
			title: 'Collapse to prompts + replies'
		},
		{ key: 'tools', label: 'Tools', icon: Wrench, title: 'Expand all tool cards' },
		{ key: 'errors', label: 'Errors', icon: AlertTriangle, title: 'Only turns that errored' },
		{ key: 'dispatch', label: 'Sub-agents', icon: GitBranch, title: 'Only turns with a sub-agent' }
	];

	function roleCount(key: RoleFilter): number | null {
		if (key === 'errors') return errorTurnCount;
		if (key === 'dispatch') return dispatchTurnCount;
		return null;
	}

	let toolMenuOpen = $state(false);
	const toolMenuOptions = $derived<ToolOption[]>([
		{ value: 'all', label: 'All tools' },
		...toolOptions
	]);

	function setRole(key: RoleFilter): void {
		role = key;
		tool = 'all';
		toolMenuOpen = false;
	}
	function toggleToolMenu(): void {
		toolMenuOpen = !toolMenuOpen;
	}
	function closeToolMenu(): void {
		toolMenuOpen = false;
	}
	function pickTool(value: string): void {
		tool = value;
		role = 'tools';
		toolMenuOpen = false;
	}

	let jumpText = $state('');
	let jumpMsg = $state('');
	let jumpMsgTimer: ReturnType<typeof setTimeout> | null = null;

	function doJump(): void {
		const n = parseInt(jumpText.replace('#', ''), 10);
		if (!Number.isFinite(n) || n <= 0) {
			jumpMsg = 'enter a #';
			return;
		}
		const err = onJump(n);
		if (err) {
			jumpMsg = err;
			if (jumpMsgTimer !== null) clearTimeout(jumpMsgTimer);
			jumpMsgTimer = setTimeout(() => {
				jumpMsg = '';
			}, 2200);
			return;
		}
		jumpMsg = '';
	}

	function onJumpKey(e: KeyboardEvent): void {
		if (e.key === 'Enter') doJump();
	}
</script>

<div bind:this={sentinelEl} class="cb-sentinel" aria-hidden="true"></div>
<header class="cb" class:docked data-slot="timeline-command-bar" bind:clientHeight={barHeight}>
	<div class="cb__meta">
		<span class="panel-hd">Requests</span>
		<span class="cb__chip" class:cb__chip--live={isLive}>
			{isLive ? 'live' : 'idle'}
		</span>
		<span class="cb__dot">·</span>
		<span>{requestCount} request{requestCount === 1 ? '' : 's'}</span>
		<span class="cb__dot">·</span>
		<span>{turnCount} turn{turnCount === 1 ? '' : 's'}</span>
	</div>

	<div class="cb__search">
		<Search size={13} class="cb__search-icon" aria-hidden="true" />
		<input
			type="text"
			bind:value={query}
			placeholder="Search prompts, replies, tools, paths, or a #req id"
			spellcheck="false"
			aria-label="Search prompts, replies, tools, paths, #req"
			class="cb__search-input"
		/>
		{#if query.length > 0}
			<button
				type="button"
				aria-label="Clear search"
				class="cb__search-clear"
				onclick={() => (query = '')}
			>
				<X size={12} aria-hidden="true" />
			</button>
		{/if}
	</div>

	<div class="cb__seg" role="group" aria-label="Filter turns by role">
		{#each ROLES as r (r.key)}
			{@const count = roleCount(r.key)}
			{@const isTools = r.key === 'tools'}
			{@const active = isTools ? role === 'tools' || tool !== 'all' : role === r.key}
			<div class="cb__seg-item">
				<button
					type="button"
					aria-pressed={active}
					aria-haspopup={isTools ? 'menu' : undefined}
					aria-expanded={isTools ? toolMenuOpen : undefined}
					title={r.title}
					class="cb__seg-btn"
					class:cb__seg-btn--active={active}
					onclick={() => (isTools ? toggleToolMenu() : setRole(r.key))}
				>
					<r.icon size={12} aria-hidden="true" />
					<span class="cb__seg-label">{isTools && tool !== 'all' ? `Tools: ${tool}` : r.label}</span
					>
					{#if count !== null && count > 0}
						<span class="cb__seg-count">{count}</span>
					{/if}
					{#if isTools}
						{#if toolMenuOpen}
							<ChevronUp size={11} aria-hidden="true" />
						{:else}
							<ChevronDown size={11} aria-hidden="true" />
						{/if}
					{/if}
				</button>
				{#if isTools && toolMenuOpen}
					<button
						type="button"
						class="cb__tool-menu-backdrop"
						aria-label="Close tool menu"
						tabindex="-1"
						onclick={closeToolMenu}
					></button>
					<div role="menu" class="cb__tool-menu" data-elevation="raised">
						{#each toolMenuOptions as o (o.value)}
							<button
								type="button"
								role="menuitemradio"
								aria-checked={tool === o.value}
								class="cb__tool-menu-item"
								class:cb__tool-menu-item--active={tool === o.value}
								onclick={() => pickTool(o.value)}
							>
								<span class="cb__tool-menu-item-label">{o.label}</span>
								{#if tool === o.value}
									<Check size={11} aria-hidden="true" />
								{/if}
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{/each}
	</div>

	<button
		type="button"
		aria-pressed={operatorOnly}
		title={operatorOnly
			? 'Showing operator turns only — click to reveal harness-kind turns (activity-label, hook-feedback, teammate-message)'
			: 'Showing every prompt kind — click to hide harness-kind turns'}
		class="cb__toggle"
		class:cb__toggle--active={operatorOnly}
		onclick={onToggleOperatorOnly}
	>
		<UserRound size={12} aria-hidden="true" />
		<span class="cb__seg-label">Operator only</span>
	</button>

	<div class="cb__jump">
		<span class="cb__jump-hash">#</span>
		<input
			bind:value={jumpText}
			oninput={() => (jumpMsg = '')}
			onkeydown={onJumpKey}
			placeholder="req"
			inputmode="numeric"
			class="cb__jump-input"
		/>
		<button
			type="button"
			title="Jump to request"
			aria-label="Jump to request"
			class="cb__jump-submit"
			onclick={doJump}
		>
			<CornerDownLeft size={12} aria-hidden="true" />
		</button>
		{#if jumpMsg}
			<span class="cb__jump-msg" role="alert">{jumpMsg}</span>
		{/if}
	</div>

	<div class="cb__actions">
		<button
			type="button"
			title="Expand all"
			aria-label="Expand all turns"
			class="cb__iconbtn"
			onclick={onExpandAll}
		>
			<ChevronsUpDown size={13} aria-hidden="true" />
		</button>
		<button
			type="button"
			title="Collapse all"
			aria-label="Collapse all turns"
			class="cb__iconbtn"
			onclick={onCollapseAll}
		>
			<ChevronsDownUp size={13} aria-hidden="true" />
		</button>
	</div>
</header>

<style>
	.cb {
		position: sticky;
		top: 44px;
		z-index: 40;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2, 8px);
		padding: var(--space-2, 8px) var(--space-4, 16px);
		background: var(--surface-bg);
		border-bottom: 1px solid var(--border-default);
		--cmdbar-control-h: 28px;
		transform: translateY(0);
		box-shadow:
			0 0 0 transparent,
			0 0 0 transparent;
		transition:
			background-color var(--dur-1) var(--ease-snap),
			box-shadow var(--dur-1) var(--ease-snap);
	}

	.cb__meta {
		display: flex;
		flex-shrink: 0;
		align-items: center;
		gap: var(--space-2, 8px);
		font-family: var(--font-mono);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
		color: var(--text-3);
	}

	.cb__dot {
		color: var(--text-faint);
	}

	.cb__chip {
		display: inline-flex;
		align-items: center;
		height: 18px;
		padding-inline: 6px;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		color: var(--text-3);
		font-size: var(--text-caption);
		line-height: 1;
		transition:
			color var(--dur-1) var(--ease-snap),
			background-color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.cb__chip--live {
		color: var(--status-success-fg);
		border-color: var(--status-success-border);
		background: var(--status-success-bg);
	}

	.cb__search {
		position: relative;
		display: flex;
		min-width: 160px;
		flex: 1 1 300px;
		align-items: center;
		margin-left: auto;
	}
	.cb__search :global(.cb__search-icon) {
		position: absolute;
		left: 8px;
		color: var(--text-faint);
		pointer-events: none;
	}
	.cb__search-input {
		height: var(--cmdbar-control-h);
		width: 100%;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		padding-inline: 28px;
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-1);
		outline: none;
	}
	.cb__search-input:focus-visible {
		border-color: var(--interactive-focus-ring);
		box-shadow: 0 0 0 2px var(--interactive-focus-ring);
	}
	.cb__search-clear {
		position: absolute;
		right: 8px;
		display: flex;
		color: var(--text-faint);
	}
	.cb__search-clear:hover {
		color: var(--text-strong);
	}

	.cb__seg {
		display: flex;
		flex-wrap: wrap;
		max-width: 100%;
		align-items: center;
		gap: 2px;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		padding: 2px;
	}
	.cb__seg-item {
		position: relative;
		display: inline-flex;
	}
	.cb__seg-btn {
		display: inline-flex;
		align-items: center;
		height: var(--cmdbar-control-h);
		gap: var(--space-1, 4px);
		white-space: nowrap;
		border-radius: var(--r-tol);
		border: 1px solid transparent;
		padding-inline: var(--space-2, 8px);
		background: transparent;
		color: var(--text-2);
		transition:
			color var(--dur-1) var(--ease-snap),
			background-color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.cb__seg-btn:not(.cb__seg-btn--active):hover {
		color: var(--text-1);
	}
	.cb__seg-btn--active {
		color: var(--data-teal-fg);
		background: var(--data-teal-bg);
		border-color: var(--data-teal-border);
	}
	.cb__seg-label {
		font-family: var(--font-display);
		font-size: var(--text-label);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--track-machine, 0.08em);
	}
	.cb__seg-count {
		font-family: var(--font-mono);
		font-size: var(--text-micro);
		font-weight: 600;
		color: var(--text-faint);
	}

	.cb__tool-menu-backdrop {
		position: fixed;
		inset: 0;
		z-index: 40;
		cursor: default;
		background: transparent;
		border: none;
		padding: 0;
		margin: 0;
	}
	.cb__tool-menu {
		position: absolute;
		left: 0;
		top: calc(100% + 6px);
		z-index: 50;
		display: flex;
		max-height: 320px;
		min-width: 190px;
		flex-direction: column;
		gap: 1px;
		overflow-y: auto;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-2);
		padding: var(--space-1, 4px);
	}
	.cb__tool-menu-item {
		display: flex;
		align-items: center;
		gap: var(--space-2, 8px);
		border-radius: var(--r-tol);
		padding: 6px 8px;
		text-align: left;
		font-size: var(--text-label);
		color: var(--text-2);
	}
	.cb__tool-menu-item:hover {
		background: var(--surface-card);
	}
	.cb__tool-menu-item--active {
		color: var(--data-teal-fg);
	}
	.cb__tool-menu-item-label {
		flex: 1;
		white-space: nowrap;
	}

	.cb__toggle {
		display: inline-flex;
		flex-shrink: 0;
		align-items: center;
		height: var(--cmdbar-control-h);
		gap: var(--space-1, 4px);
		white-space: nowrap;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		padding-inline: var(--space-2, 8px);
		background: var(--surface-bg);
		color: var(--text-2);
		transition:
			color var(--dur-1) var(--ease-snap),
			background-color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.cb__toggle:not(.cb__toggle--active):hover {
		color: var(--text-1);
	}
	.cb__toggle--active {
		color: var(--data-teal-fg);
		background: var(--data-teal-bg);
		border-color: var(--data-teal-border);
	}

	.cb__jump {
		position: relative;
		display: flex;
		height: var(--cmdbar-control-h);
		align-items: center;
		gap: var(--space-1, 4px);
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		padding-left: 8px;
		padding-right: 2px;
	}
	.cb__jump-hash {
		font-family: var(--font-mono);
		font-size: var(--text-caption);
		color: var(--text-faint);
	}
	.cb__jump-input {
		height: 100%;
		width: 42px;
		background: transparent;
		font-family: var(--font-mono);
		font-size: var(--text-label);
		color: var(--text-1);
		outline: none;
	}
	.cb__jump-submit {
		display: flex;
		height: 22px;
		width: 22px;
		align-items: center;
		justify-content: center;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-subtle);
		background: var(--surface-2);
		color: var(--text-2);
		transition:
			color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.cb__jump-submit:hover {
		border-color: var(--border-strong);
		color: var(--text-1);
	}
	.cb__jump-msg {
		position: absolute;
		top: 100%;
		right: 0;
		z-index: 10;
		margin-top: var(--space-1, 4px);
		white-space: nowrap;
		border-radius: var(--r-tol);
		border: 1px solid var(--status-danger-border);
		background: var(--surface-card);
		padding: 2px 6px;
		font-size: var(--text-micro);
		color: var(--status-danger-fg);
	}

	.cb__actions {
		display: flex;
		flex-shrink: 0;
		gap: 2px;
	}
	.cb__iconbtn {
		display: flex;
		height: var(--cmdbar-control-h);
		width: var(--cmdbar-control-h);
		align-items: center;
		justify-content: center;
		border-radius: var(--r-tol);
		border: 1px solid var(--border-default);
		background: var(--surface-bg);
		color: var(--text-3);
		transition:
			color var(--dur-1) var(--ease-snap),
			border-color var(--dur-1) var(--ease-snap);
	}
	.cb__iconbtn:hover {
		border-color: var(--border-strong);
		color: var(--text-1);
	}

	.cb.docked {
		background: var(--surface-card);
		box-shadow:
			0 0 2px var(--data-teal-border),
			0 0 12px var(--data-teal-border);
	}

	.cb::before {
		content: '';
		position: absolute;
		inset-inline: 0;
		bottom: -1px;
		height: 2px;
		background:
			linear-gradient(90deg, var(--data-teal-fg), transparent) left / 10px 2px no-repeat,
			linear-gradient(270deg, var(--data-teal-fg), transparent) right / 10px 2px no-repeat;
		opacity: 0;
		transform: scaleY(0);
		transform-origin: bottom;
		pointer-events: none;
		transition: opacity var(--dur-1) var(--ease-snap);
	}
	.cb.docked::before {
		opacity: 1;
		transform: scaleY(1);
	}

	.cb::after {
		content: '';
		position: absolute;
		inset-inline: 0;
		bottom: -1px;
		height: 1px;
		background: linear-gradient(
			90deg,
			var(--data-teal-fg),
			var(--data-teal-border) 60%,
			var(--border-default)
		);
		transform-origin: left;
		opacity: 0;
		pointer-events: none;
		transition: opacity var(--dur-1) var(--ease-snap);
	}
	.cb.docked::after {
		opacity: 1;
	}

	@keyframes cb-settle {
		from {
			transform: translateY(-3px);
		}
		to {
			transform: translateY(0);
		}
	}
	@keyframes cb-glow {
		0% {
			box-shadow:
				0 0 0 transparent,
				0 0 0 transparent;
		}
		70% {
			box-shadow:
				0 0 2px var(--data-teal-fg),
				0 0 12px var(--data-teal-fg);
		}
		100% {
			box-shadow:
				0 0 2px var(--data-teal-border),
				0 0 12px var(--data-teal-border);
		}
	}
	.cb.docked {
		animation:
			cb-settle var(--dur-1) var(--ease-snap) both,
			cb-glow var(--dur-2) var(--ease-io) calc(var(--dur-1) + var(--dur-2) * 0.5) both;
	}

	@keyframes cb-ticks-in {
		from {
			opacity: 0;
			transform: scaleY(0);
		}
		to {
			opacity: 1;
			transform: scaleY(1);
		}
	}
	.cb.docked::before {
		animation: cb-ticks-in var(--dur-1) var(--ease-snap) both;
	}

	@keyframes cb-sweep-in {
		from {
			opacity: 0;
			transform: scaleX(0);
		}
		to {
			opacity: 1;
			transform: scaleX(1);
		}
	}
	.cb.docked::after {
		animation: cb-sweep-in var(--dur-2) var(--ease-snap) var(--dur-1) both;
	}

	.cb-sentinel {
		height: 1px;
	}
</style>
