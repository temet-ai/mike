<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { getContext } from 'svelte';
	import { cn, type WithElementRef } from '$lib/utils';
	import { Bell, Folder, Globe, Server, Sparkles, Wrench, Zap } from '@lucide/svelte';
	import { formatCount } from '$lib/time';
	import type {
		ClaudeMdSection,
		ClaudeMdSubsection,
		GroupKind,
		ParsedSystemPrompt,
		ReminderBlock,
		ReminderGroup
	} from './parseSystemPrompt';
	import { estimateTokens, formatTokens } from './parseSystemPrompt';
	import SystemRemindersCard from './SystemRemindersCard.svelte';
	import ReminderSubgroup from './ReminderSubgroup.svelte';
	import ReminderRow from './ReminderRow.svelte';
	import SubRow from './SubRow.svelte';
	import SubRowDetail from './SubRowDetail.svelte';
	import CodeBlock from './CodeBlock.svelte';
	import UserMessageCard from './UserMessageCard.svelte';
	import Tag from './Tag.svelte';
	import { COLLAPSE_STORE_KEY } from '$lib/context-keys';
	import type { CollapseStore } from '$lib/stores/collapseState.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		parsed: ParsedSystemPrompt;
		userMessage: string;
		renderCardChrome?: boolean;
		renderUserMessage?: boolean;
		reqId?: number | string | null;
	}

	let {
		ref = $bindable(null),
		class: className,
		parsed,
		userMessage,
		renderCardChrome = true,
		renderUserMessage = true,
		reqId = null,
		...restProps
	}: Props = $props();

	const collapseStore = getContext<CollapseStore | undefined>(COLLAPSE_STORE_KEY);
	const usesStore = $derived(collapseStore !== undefined && reqId !== null);

	type ContextBucket = 'global' | 'project';

	interface ContextSubgroup {
		bucket: ContextBucket;
		label: 'Global' | 'Project';
		hint: string;
		sections: ClaudeMdSection[];
		charCount: number;
		estTokens: number;
	}

	const contextSubgroups = $derived.by((): ContextSubgroup[] => {
		if (!parsed.claudeMd) return [];
		const global = parsed.claudeMd.sections.filter((s) => s.bucket === 'global');
		const project = parsed.claudeMd.sections.filter(
			(s) => s.bucket === 'project' || s.bucket === 'memory' || s.bucket === 'harness'
		);
		const out: ContextSubgroup[] = [];
		if (global.length > 0) {
			const chars = global.reduce((a, s) => a + s.charCount, 0);
			out.push({
				bucket: 'global',
				label: 'Global',
				hint: `~/.claude/ · ${global.length} file${global.length === 1 ? '' : 's'}`,
				sections: global,
				charCount: chars,
				estTokens: estimateTokens(chars)
			});
		}
		if (project.length > 0) {
			const chars = project.reduce((a, s) => a + s.charCount, 0);
			out.push({
				bucket: 'project',
				label: 'Project',
				hint: `${project.length} source${project.length === 1 ? '' : 's'}`,
				sections: project,
				charCount: chars,
				estTokens: estimateTokens(chars)
			});
		}
		return out;
	});

	const SUBSECTION_ORDER: ClaudeMdSubsection[] = ['claudeMd', 'rules', 'memory', 'harness'];
	const SUBSECTION_LABEL: Record<ClaudeMdSubsection, string> = {
		claudeMd: 'CLAUDE.md',
		rules: 'Rules',
		memory: 'Memory',
		harness: 'Harness'
	};

	function groupSectionsBySubsection(
		sections: ClaudeMdSection[]
	): Array<{ subsection: ClaudeMdSubsection; sections: ClaudeMdSection[] }> {
		const groups: Array<{ subsection: ClaudeMdSubsection; sections: ClaudeMdSection[] }> = [];
		for (const sub of SUBSECTION_ORDER) {
			const inSub = sections.filter((s) => s.subsection === sub);
			if (inSub.length > 0) groups.push({ subsection: sub, sections: inSub });
		}
		return groups;
	}

	const openSubgroups = $state<Record<string, boolean>>({
		hooks: true,
		mcp: true,
		tools: true,
		skills: true,
		claudeMd: true,
		'context:global': true,
		'context:project': true,
		other: true
	});

	function subgroupStoreKey(key: string): string {
		return `msg:${reqId}:subgroup:${key}`;
	}

	function isSubgroupOpen(key: string): boolean {
		return usesStore
			? collapseStore!.isExpanded(subgroupStoreKey(key), true)
			: (openSubgroups[key] ?? true);
	}

	function toggleSubgroup(key: string): void {
		if (usesStore) collapseStore!.toggle(subgroupStoreKey(key));
		else openSubgroups[key] = !(openSubgroups[key] ?? true);
	}

	const openRows = $state<Record<string, boolean>>({});

	function rowStoreKey(key: string): string {
		return `msg:${reqId}:row:${key}`;
	}

	function toggleRow(key: string): void {
		if (usesStore) collapseStore!.toggle(rowStoreKey(key));
		else openRows[key] = !(openRows[key] ?? false);
	}

	function isRowOpen(key: string): boolean {
		return usesStore ? collapseStore!.isExpanded(rowStoreKey(key)) : (openRows[key] ?? false);
	}

	function blockRowKey(kind: GroupKind, blockIndex: number): string {
		return `block:${kind}:${blockIndex}`;
	}

	function sectionRowKey(bucket: ContextBucket, path: string): string {
		return `section:${bucket}:${path}`;
	}

	function blockTitle(block: ReminderBlock): string {
		return block.text
			.replace(/^<[^>]+>/, '')
			.replace(/^#+\s*/, '')
			.replace(/^contents of\s+/i, '')
			.trim();
	}

	function isClaudeMdGroup(kind: GroupKind): boolean {
		return kind === 'claudeMd';
	}
	function isFlatLeafGroup(kind: GroupKind): boolean {
		return kind === 'tools' || kind === 'skills';
	}

	const rawReminderGroups = $derived(
		parsed.groups.filter((g) => !isClaudeMdGroup(g.kind) && !isFlatLeafGroup(g.kind))
	);

	const flatLeafGroups = $derived(parsed.groups.filter((g) => isFlatLeafGroup(g.kind)));
</script>

{#snippet subgroupIcon(kind: GroupKind | 'context-global' | 'context-project')}
	{#if kind === 'hooks'}
		<Zap size={14} class="ico" aria-hidden="true" />
	{:else if kind === 'mcp'}
		<Server size={14} class="ico" aria-hidden="true" />
	{:else if kind === 'context-global'}
		<Globe size={14} class="ico" aria-hidden="true" />
	{:else if kind === 'context-project'}
		<Folder size={14} class="ico" aria-hidden="true" />
	{:else if kind === 'tools'}
		<Wrench size={14} class="ico" aria-hidden="true" />
	{:else if kind === 'skills'}
		<Sparkles size={14} class="ico" aria-hidden="true" />
	{:else}
		<Bell size={14} class="ico" aria-hidden="true" />
	{/if}
{/snippet}

{#snippet rawGroupHead(group: ReminderGroup)}
	{@render subgroupIcon(group.kind)}
	<Tag variant="group">{group.label}</Tag>
	<span class="row__title">{group.subtitle}</span>
	{#if group.hint}
		<span class="row__sub">· {group.hint}</span>
	{/if}
	<span class="row__stats">
		<span class="tok">{formatTokens(group.estTokens)}</span>
	</span>
{/snippet}

{#snippet contextHead(sub: ContextSubgroup)}
	{@render subgroupIcon(sub.bucket === 'global' ? 'context-global' : 'context-project')}
	<Tag variant="group">Context</Tag>
	<span class="row__title">{sub.label}</span>
	<span class="row__sub">· {sub.hint}</span>
	<span class="row__stats">
		<span class="tok">{formatTokens(sub.estTokens)}</span>
	</span>
{/snippet}

{#snippet reminderBody()}
	{#each rawReminderGroups as group (group.kind)}
		<ReminderSubgroup
			open={isSubgroupOpen(group.kind)}
			ariaLabel={`${group.label} ${group.subtitle}`}
			onToggle={() => toggleSubgroup(group.kind)}
		>
			{#snippet head()}
				{@render rawGroupHead(group)}
			{/snippet}
			{#each group.blocks as block (block.index)}
				{@const rowKey = blockRowKey(group.kind, block.index)}
				<SubRow
					expandable
					open={isRowOpen(rowKey)}
					ariaLabel={`${group.label} block ${block.index + 1}`}
					onclick={() => toggleRow(rowKey)}
				>
					<span class="subrow__title">{blockTitle(block)}</span>
					<span class="row__stats" style="margin-left: auto;">
						<span class="tok">{formatTokens(block.estTokens)}</span>
					</span>
				</SubRow>
				{#if isRowOpen(rowKey)}
					<SubRowDetail label={`${group.label} block ${block.index + 1}`}>
						<div class="detail__section">
							<span class="detail__label">Raw reminder</span>
							<CodeBlock lang="reminder" code={block.text} />
						</div>
					</SubRowDetail>
				{/if}
			{/each}
		</ReminderSubgroup>
	{/each}

	{#each contextSubgroups as sub (sub.bucket)}
		{@const subKey = `context:${sub.bucket}`}
		<ReminderSubgroup
			open={isSubgroupOpen(subKey)}
			ariaLabel={`Context ${sub.label}`}
			onToggle={() => toggleSubgroup(subKey)}
		>
			{#snippet head()}
				{@render contextHead(sub)}
			{/snippet}
			{#each groupSectionsBySubsection(sub.sections) as subsectionGroup (subsectionGroup.subsection)}
				<div class="subsection">
					<span class="subsection__label">{SUBSECTION_LABEL[subsectionGroup.subsection]}</span>
				</div>
				{#each subsectionGroup.sections as section (section.path)}
					{@const rowKey = sectionRowKey(sub.bucket, section.path)}
					<SubRow
						expandable
						open={isRowOpen(rowKey)}
						ariaLabel={`${sub.label} ${SUBSECTION_LABEL[subsectionGroup.subsection]} ${section.displayPath}`}
						onclick={() => toggleRow(rowKey)}
					>
						<span class="subrow__path">{section.displayPath}</span>
						<span class="row__stats" style="margin-left: auto;">
							<span class="tok">{formatTokens(section.estTokens)}</span>
						</span>
					</SubRow>
					{#if isRowOpen(rowKey)}
						<SubRowDetail label={`${sub.label} ${section.displayPath}`}>
							<div class="detail__section">
								<span class="detail__label">File contents</span>
								<CodeBlock
									lang={section.subsection === 'harness' ? 'harness' : 'markdown'}
									code={section.body}
								/>
							</div>
						</SubRowDetail>
					{/if}
				{/each}
			{/each}
		</ReminderSubgroup>
	{/each}

	{#each flatLeafGroups as group (group.kind)}
		<ReminderRow>
			{@render rawGroupHead(group)}
		</ReminderRow>
	{/each}
{/snippet}

<div
	bind:this={ref}
	data-slot="message-view-user-turn"
	class={cn('flex flex-col gap-2.5', className)}
	{...restProps}
>
	{#if parsed.groups.length > 0}
		{#if renderCardChrome}
			<SystemRemindersCard
				groupCount={parsed.groups.length}
				totalChars={parsed.totalChars}
				totalTokens={parsed.totalTokens}
			>
				{@render reminderBody()}
			</SystemRemindersCard>
		{:else}
			<div class="tsc-reminders-flat" style:--reminders-head-h="0px">
				{@render reminderBody()}
			</div>
		{/if}
	{/if}

	{#if renderUserMessage}
		<UserMessageCard text={userMessage} />
	{/if}
</div>

<style>
	:global(.subgroup__head .row__title),
	:global(.reminders__body .row .row__title) {
		color: var(--text-strong);
		font-weight: 500;
	}
	:global(.subgroup__head .row__sub),
	:global(.reminders__body .row .row__sub) {
		color: var(--text-2);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
	}
	:global(.subgroup__head .row__stats),
	:global(.reminders__body .row .row__stats),
	:global(.subrow .row__stats) {
		margin-left: auto;
		color: var(--text-2);
		font-size: var(--text-label);
		font-variant-numeric: tabular-nums;
		display: flex;
		gap: 10px;
		align-items: center;
	}
	:global(.subrow .row__stats .tok),
	:global(.subgroup__head .row__stats .tok) {
		color: var(--text-3);
	}
	:global(.subrow .row__stats .sep),
	:global(.subgroup__head .row__stats .sep) {
		color: var(--text-faint);
	}
	:global(.subrow__title) {
		color: var(--text-strong);
		font-weight: 500;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		word-break: break-word;
		min-width: 0;
	}
	:global(.detail__section) {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	:global(.detail__label) {
		color: var(--text-3);
		font-size: var(--text-micro);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: var(--tracking-kpi);
	}
</style>
