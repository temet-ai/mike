<script lang="ts">
	import type { HTMLAttributes } from 'svelte/elements';
	import { resolve } from '$app/paths';
	import { cn, type WithElementRef } from '$lib/utils';
	import type { RequestDetail } from '$lib/types';
	import SectionCard from './SectionCard.svelte';
	import HelpHint from '../shared/HelpHint.svelte';
	import Tooltip from '../shared/Tooltip.svelte';

	interface Props extends WithElementRef<HTMLAttributes<HTMLDivElement>> {
		detail: RequestDetail;
	}

	let { ref = $bindable(null), class: className, detail, ...restProps }: Props = $props();

	type LabelHelp =
		{ kind: 'hint'; slug: string } | { kind: 'tip'; text: string; slug?: string } | null;

	type Row = {
		label: string;
		value: string;
		href: `sessions/${string}` | null;
		help: LabelHelp;
	};

	const promptSha256 = $derived(detail.system_blocks?.prompt_sha256 ?? null);

	const rows = $derived<Row[]>([
		{
			label: 'session_id',
			value: detail.session_id,
			href:
				detail.session_id.length > 0 ? `sessions/${encodeURIComponent(detail.session_id)}` : null,
			help: { kind: 'hint', slug: 'session' }
		},
		{
			label: 'upstream_request_id',
			value: detail.upstream_request_id ?? '—',
			href: null,
			help: {
				kind: 'tip',
				text: 'Anthropic request-id header (req_xxx) — quote in support tickets',
				slug: 'upstream-request-id'
			}
		},
		{
			label: 'upstream_message_id',
			value: detail.upstream_message_id ?? '—',
			href: null,
			help: {
				kind: 'tip',
				text: 'Anthropic message_start.id (msg_xxx) — streaming-only',
				slug: 'upstream-message-id'
			}
		},
		{
			label: 'project',
			value: detail.project ?? '—',
			href: null,
			help: { kind: 'hint', slug: 'project' }
		},
		{
			label: 'prompt_sha256',
			value: promptSha256 ?? '—',
			href: null,
			help: { kind: 'hint', slug: 'prompt-sha256' }
		}
	]);
</script>

<div
	bind:this={ref}
	data-slot="identifiers-section"
	class={cn('contents', className)}
	{...restProps}
>
	<SectionCard
		title="Identifiers"
		ariaLabel="Request identifiers"
		collapsible
		sectionKey="detail:identifiers"
	>
		<dl
			data-slot="identifiers-grid"
			class="grid grid-cols-1 gap-x-6 gap-y-3 text-xs sm:grid-cols-2 lg:grid-cols-3"
		>
			{#each rows as row (row.label)}
				<div data-slot="identifiers-row">
					<dt class="flex items-center gap-1 uppercase tracking-wider text-[var(--text-3)]">
						{#if row.help && row.help.kind === 'tip'}
							<Tooltip text={row.help.text} slug={row.help.slug}>
								<span>{row.label}</span>
							</Tooltip>
						{:else}
							<span>{row.label}</span>
							{#if row.help && row.help.kind === 'hint'}
								<HelpHint slug={row.help.slug} />
							{/if}
						{/if}
					</dt>
					<dd class="mt-0.5 break-all font-mono text-[var(--text-strong)]">
						{#if row.href !== null}
							<a
								href={resolve(row.href)}
								class="hover:underline hover:text-[var(--status-info-fg)] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--interactive-focus-ring)]"
								data-slot="identifiers-link"
							>
								{row.value}
							</a>
						{:else}
							{row.value}
						{/if}
					</dd>
				</div>
			{/each}
		</dl>
	</SectionCard>
</div>
