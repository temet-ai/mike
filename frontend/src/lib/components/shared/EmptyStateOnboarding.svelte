<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { getConnectionInfo, directClaudeCommand } from '$lib/api/connectionInfo';
	import { Inbox, BookOpen, HelpCircle } from '@lucide/svelte';
	import CopyIconButton from './CopyIconButton.svelte';
	import { Button } from '$lib/components/ui/button';

	interface Props {
		headline?: string;
		body?: string;
		command?: string | null;
		confirmation?: string;
		onOpenHelp?: () => void;
		actions?: Snippet;
	}

	const {
		headline = 'No requests captured yet',
		body = 'Run Claude Code on the computer running mike, then send a prompt.',
		command,
		confirmation = 'Captured requests will appear here in real time.',
		onOpenHelp,
		actions
	}: Props = $props();

	let runtimeCommand = $state<string | null>(null);
	let connectionError = $state<string | null>(null);
	let connectionLoading = $state(true);
	const shownCommand = $derived(command === undefined ? runtimeCommand : command);

	onMount(() => {
		if (command !== undefined) return;
		const controller = new AbortController();
		void getConnectionInfo(controller.signal)
			.then((info) => {
				if (controller.signal.aborted) return;
				if (!info.proxy_url)
					throw new Error('The running proxy has no available connection address.');
				runtimeCommand = directClaudeCommand(info.proxy_url);
			})
			.catch((error: unknown) => {
				if (!controller.signal.aborted) {
					connectionError = error instanceof Error ? error.message : String(error);
				}
			})
			.finally(() => {
				if (!controller.signal.aborted) connectionLoading = false;
			});
		return () => controller.abort();
	});
</script>

<div
	class="empty-state-onboarding"
	role="status"
	aria-live="polite"
	data-slot="empty-state-onboarding"
>
	<div class="icon" aria-hidden="true">
		<Inbox size={24} />
	</div>

	<h2 class="headline">{headline}</h2>

	<p class="body">{body}</p>

	{#if shownCommand}
		<div class="command-row-backing">
			<div class="command-row" data-augmented-ui="tr-clip bl-clip border">
				<code class="command font-mono">{shownCommand}</code>
				<CopyIconButton
					getText={() => shownCommand ?? ''}
					label="Copy command"
					copiedLabel="Copied"
					size={12}
				/>
			</div>
		</div>
	{/if}

	{#if command === undefined && connectionLoading}
		<p class="confirmation">Reading the running proxy address...</p>
	{:else if command === undefined && connectionError}
		<p class="confirmation">{connectionError} Open the installed guide to check your setup.</p>
	{/if}

	{#if confirmation}
		<p class="confirmation">{confirmation}</p>
	{/if}

	<div class="secondary-actions">
		{#if onOpenHelp}
			<Button variant="outline" onclick={onOpenHelp} data-slot="empty-state-help-trigger">
				<HelpCircle class="size-3.5" aria-hidden="true" />
				<span>What is this?</span>
			</Button>
		{/if}
		{#if actions}
			{@render actions()}
		{:else}
			<Button
				variant="outline"
				href={resolve('/help/guides/getting-started')}
				data-slot="empty-state-readme-link"
			>
				<BookOpen class="size-3.5" aria-hidden="true" />
				<span>Getting started</span>
			</Button>
		{/if}
	</div>
</div>

<style>
	.empty-state-onboarding {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-3);
		padding: var(--space-6) var(--space-5);
		width: 100%;
		max-width: 35rem;
		min-width: 0;
		overflow-wrap: anywhere;
		margin: 0 auto;
		text-align: center;
	}
	.icon {
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-3);
		opacity: 0.6;
		margin-bottom: var(--space-1);
	}
	.headline {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--weight-display);
		color: var(--text-strong);
		margin: 0;
	}
	.body {
		font-size: var(--text-md);
		color: var(--text-2);
		margin: 0;
		max-width: 48ch;
	}
	.command-row-backing {
		display: inline-flex;
		margin-top: var(--space-1);
		max-width: 100%;
		background: var(--surface-bg);
	}
	.command-row {
		--edge: var(--edge-sm);
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-br: 0;
		--aug-bl: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		padding: calc(var(--edge) + var(--space-1));
		background: var(--surface-2);
		max-width: 100%;
	}
	.command {
		font-size: var(--text-sm);
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.confirmation {
		font-size: var(--text-sm);
		color: var(--text-2);
		margin: 0;
		max-width: 48ch;
	}
	.secondary-actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		justify-content: center;
		margin-top: var(--space-2);
	}
</style>
