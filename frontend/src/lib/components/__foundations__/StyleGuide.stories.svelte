<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { Inbox, Play, Plus, RotateCw, SearchX, X } from '@lucide/svelte';
	import { Button } from '$lib/components/ui/button';
	import Field from '$lib/components/ui/field/field.svelte';
	import * as RadioGroup from '$lib/components/ui/radio-group';
	import { Slider } from '$lib/components/ui/slider';
	import { Switch } from '$lib/components/ui/switch';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import { Select } from '$lib/components/ui/select';
	import SearchField from '$lib/components/shared/SearchField.svelte';
	import * as Tabs from '$lib/components/ui/tabs';
	import Badge from '$lib/components/shared/Badge.svelte';
	import StatusPill from '$lib/components/shared/StatusPill.svelte';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';
	import HelpHint from '$lib/components/shared/HelpHint.svelte';
	import LoadErrorBanner from '$lib/components/shared/LoadErrorBanner.svelte';
	import EmptyStateOnboarding from '$lib/components/shared/EmptyStateOnboarding.svelte';
	import FilteredEmptyState from '$lib/components/shared/FilteredEmptyState.svelte';
	import InstrumentReadout from '$lib/components/shared/InstrumentReadout.svelte';
	import PageIntro from '$lib/components/shared/PageIntro.svelte';
	import SectionCard from '$lib/components/detail/SectionCard.svelte';
	import SettingsSection from '$lib/components/settings/SettingsSection.svelte';
	import SettingRow from '$lib/components/settings/SettingRow.svelte';
	import type { SettingMeta } from '$lib/api/settings';

	const { Story } = defineMeta({
		title: 'Foundations/Style Guide',
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'The FUNK style guide, section by section, drawn with the components a page ' +
						'imports. Each plate names the file to import. A pattern the guide shows and ' +
						'the app has no component for is marked as missing.'
				}
			}
		}
	});

	const SETTINGS: SettingMeta[] = [
		{
			key: 'gallery.capture',
			group: 'Gallery',
			kind: 'bool',
			scope: 'global',
			applies: 'next_message',
			default: true,
			value: true,
			label: 'Capture traffic',
			description: 'Sends traffic through mike, so you can inspect it.'
		},
		{
			key: 'gallery.effort',
			group: 'Gallery',
			kind: 'enum',
			scope: 'global',
			applies: 'on_restart',
			default: 'medium',
			value: 'medium',
			label: 'Effort',
			description: 'How much the model thinks before it answers.',
			enum_variants: ['low', 'medium', 'high']
		}
	];

	let protocol = $state('tail');
	let port = $state('8791');
	let level = $state(40);
	let scheme = $state('wss');
	let persist = $state(true);
	let tail = $state(true);
	let find = $state('');
</script>

{#snippet plate(name: string, file: string)}
	<div class="sg-plate-head">
		<span class="machine sg-name">{name}</span>
		<code class="sg-file">{file}</code>
	</div>
{/snippet}

{#snippet missing(name: string)}
	<div class="sg-missing" data-testid="missing-{name.toLowerCase()}">
		<span class="machine sg-name">{name}</span>
		<span
			>No component yet. Build it under <code>lib/components/ui/</code> before a page uses it.</span
		>
	</div>
{/snippet}

<Story name="Core">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Core</h2>
			{@render plate('Button', '$lib/components/ui/button')}
			<div class="sg-row">
				<Button>Open link</Button>
				<Button variant="outline">Flush buffer</Button>
				<Button variant="destructive">Purge</Button>
				<Button variant="ghost">Detach</Button>
				<Button variant="outline" disabled>Locked</Button>
				<Button size="sm" variant="outline">Small</Button>
			</div>
			{@render plate('IconButton', '$lib/components/ui/button, size="icon"')}
			<div class="sg-row">
				<Button size="icon" variant="outline" aria-label="Run"><Play /></Button>
				<Button size="icon" variant="outline" aria-label="Retry"><RotateCw /></Button>
				<Button size="icon" variant="outline" aria-label="Add"><Plus /></Button>
				<Button size="icon" variant="outline" aria-label="Close"><X /></Button>
			</div>
			{@render plate('Tabs', '$lib/components/ui/tabs')}
			<Tabs.Root value="requests">
				<Tabs.List aria-label="Gallery tabs">
					<Tabs.Trigger value="requests">Requests</Tabs.Trigger>
					<Tabs.Trigger value="log">Log</Tabs.Trigger>
					<Tabs.Trigger value="config">Config</Tabs.Trigger>
				</Tabs.List>
				<Tabs.Content value="requests" class="pt-3"
					>14:02:37.114 GET /v1/stream 200 412ms</Tabs.Content
				>
				<Tabs.Content value="log" class="pt-3">88 lines</Tabs.Content>
				<Tabs.Content value="config" class="pt-3">3 keys</Tabs.Content>
			</Tabs.Root>
			{@render plate('Frame', 'global class edge; add edge-4 for four corners')}
			<div class="sg-row">
				<div class="edge sg-frame"><span>edge</span></div>
				<div class="edge edge-4 sg-frame"><span>edge edge-4</span></div>
			</div>
			<p class="sg-note">Content sits in a child element, or the frame paints over it.</p>
		</div>
	{/snippet}
</Story>

<Story name="Forms">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Forms</h2>
			{@render plate('Input', '$lib/components/ui/field/field.svelte')}
			<div class="sg-narrow">
				<Field label="Port" bind:value={port} />
			</div>
			{@render plate('Radio', '$lib/components/ui/radio-group')}
			<div class="sg-narrow">
				<span id="sg-protocol" class="machine sg-label">Protocol</span>
				<RadioGroup.Root name="sg-protocol" bind:value={protocol} aria-labelledby="sg-protocol">
					<RadioGroup.Item value="tail" label="tail" />
					<RadioGroup.Item value="snapshot" label="snapshot" />
					<RadioGroup.Item value="replay" label="replay" disabled />
				</RadioGroup.Root>
			</div>
			{@render plate('Slider', '$lib/components/ui/slider')}
			<div class="sg-narrow">
				<Slider aria-label="Level" bind:value={level} />
			</div>
			{@render plate('Select', '$lib/components/ui/select')}
			<div class="sg-narrow">
				<Select
					label="Protocol"
					bind:value={scheme}
					options={[{ value: 'wss' }, { value: 'https' }, { value: 'raw' }]}
				/>
			</div>
			{@render plate('Checkbox', '$lib/components/ui/checkbox')}
			<div class="sg-stack">
				<Checkbox bind:checked={persist} label="persist session" />
				<Checkbox checked={false} label="raw frames" />
				<Checkbox checked={false} disabled label="locked" />
			</div>
			{@render plate('Switch', '$lib/components/ui/switch')}
			<div class="sg-row">
				<Switch bind:checked={tail} aria-label="Tail" />
				<span>tail — {tail ? 'on' : 'off'}</span>
				<Switch checked={false} disabled aria-label="Locked" />
			</div>
			{@render plate('Search field', '$lib/components/shared/SearchField.svelte')}
			<div class="sg-narrow">
				<SearchField bind:value={find} label="Find a setting" placeholder="Find a setting" />
			</div>
		</div>
	{/snippet}
</Story>

<Story name="Feedback">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Feedback</h2>
			{@render plate('Badge', '$lib/components/shared/Badge.svelte')}
			<div class="sg-row">
				<Badge variant="chip-active">Active</Badge>
				<Badge variant="chip-muted">Muted</Badge>
				<Badge variant="chip-warning">Degraded</Badge>
				<Badge variant="chip-error">Err 507</Badge>
				<Badge variant="chip-analyzed">Analyzed</Badge>
				<Badge variant="tag-default">tag</Badge>
				<Badge variant="tag-green">200 OK</Badge>
				<Badge variant="tag-blue">3s ago</Badge>
				<Badge variant="tag-amber">warn</Badge>
				<Badge variant="identity">main-agent</Badge>
			</div>
			{@render plate('StatusIndicator', '$lib/components/shared/StatusPill.svelte')}
			<div class="sg-row">
				<StatusPill status="Complete" />
				<StatusPill status="Streaming" />
				<StatusPill status="Pending" />
				<StatusPill status={{ Error: 'ERR 507' }} />
			</div>
			{@render plate('Tooltip', '$lib/components/shared/Tooltip.svelte')}
			<div class="sg-row">
				<Tooltip text="A change applies to the next message you send.">
					<Badge variant="tag-blue">Next message</Badge>
				</Tooltip>
				<span>steerer<HelpHint slug="steerer" /></span>
			</div>
			{@render plate('Load error', '$lib/components/shared/LoadErrorBanner.svelte')}
			<LoadErrorBanner
				title="Could not load this page."
				detail="GET /api/settings failed: 503"
				onRetry={() => {}}
			/>
			{@render plate('Empty list', '$lib/components/shared/EmptyStateOnboarding.svelte')}
			<EmptyStateOnboarding />
			{@render plate('Empty after filtering', '$lib/components/shared/FilteredEmptyState.svelte')}
			<FilteredEmptyState
				icon={SearchX}
				filtered
				emptyText="No sessions yet"
				filteredText="No session matches these filters"
				onClearFilters={() => {}}
			/>
			{@render missing('Toast')}
		</div>
	{/snippet}
</Story>

<Story name="Data and surfaces">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Data</h2>
			{@render plate('Readout', '$lib/components/shared/InstrumentReadout.svelte')}
			<div class="sg-row">
				<InstrumentReadout label="Tokens" value="184,890" />
				<InstrumentReadout label="Latency p95" value="412 ms" accentToken="--signal-info" />
				<InstrumentReadout label="Errors" value={7} accentToken="--signal-error" />
			</div>
			<h2 class="machine sg-title">Surfaces</h2>
			{@render plate('Panel', '$lib/components/detail/SectionCard.svelte')}
			<SectionCard title="Request headers" titleIcon={Inbox} description="5 keys">
				<pre class="sg-pre">content-type: application/json
x-session: 0x4A2F
accept: text/event-stream</pre>
			</SectionCard>
			{@render plate('Dialog, Sheet, Popover', '$lib/components/ui/dialog, ui/sheet, ui/popover')}
			<p class="sg-note">
				Each opens over the page, so each has its own story: UI/Dialog, UI/Sheet, UI/Popover.
			</p>
		</div>
	{/snippet}
</Story>

<Story name="Page layout">
	{#snippet template()}
		<div class="sg" data-design="funk">
			<h2 class="machine sg-title">Page layout</h2>
			{@render plate('Page title and lede', '$lib/components/shared/PageIntro.svelte')}
			<PageIntro level="h1" title="Assistants">
				Assistants you talk to from inside mike. They run only when you ask them something.
			</PageIntro>
			{@render plate('Settings page', '$lib/components/settings/SettingsPage.svelte')}
			<p class="sg-note">
				Holds the title, the lede, the settings navigation and the load error. Story:
				Settings/SettingsPage.
			</p>
			{@render plate('Settings group', '$lib/components/settings/SettingsSection.svelte')}
			{@render plate('One setting', '$lib/components/settings/SettingRow.svelte')}
			<SettingsSection title="Prompt assistant" id="sg-prompt-assistant">
				{#each SETTINGS as setting (setting.key)}
					<SettingRow {setting} />
				{/each}
			</SettingsSection>
		</div>
	{/snippet}
</Story>

<style>
	.sg {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		max-width: 920px;
		padding: var(--space-6);
		background: var(--surface-bg);
		color: var(--text-1);
		font-size: var(--text-md);
	}
	.sg-title {
		margin: var(--space-3) 0 0;
		font-size: var(--text-lg);
		color: var(--text-strong);
	}
	.sg-plate-head {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-3);
		margin-top: var(--space-4);
		padding-top: var(--space-3);
		border-top: var(--hairline) solid var(--border-subtle);
	}
	.sg-name {
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.sg-label {
		display: block;
		margin-bottom: var(--space-2);
		font-size: var(--text-xs);
		color: var(--text-2);
	}
	.sg-file {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--interactive-active-fg);
	}
	.sg-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-3);
	}
	.sg-narrow {
		max-width: 320px;
	}
	.sg-stack {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.sg-frame {
		--edge: var(--edge-md);
		--edge-line: var(--border-default);
		--edge-surface: var(--surface-card);
		min-width: 160px;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}
	.sg-missing {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: var(--space-3);
		margin-top: var(--space-4);
		padding: var(--space-3);
		border: var(--hairline) dashed var(--status-warning-border);
		color: var(--status-warning-fg);
		font-size: var(--text-sm);
	}
	.sg-note {
		margin: 0;
		color: var(--text-2);
		font-size: var(--text-sm);
	}
	.sg-pre {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}
</style>
