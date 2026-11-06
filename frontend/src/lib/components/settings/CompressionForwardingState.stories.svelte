<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import CompressionForwardingState from './CompressionForwardingState.svelte';
	import SettingsSection from './SettingsSection.svelte';
	import SettingRow from './SettingRow.svelte';
	import type { CompressionEffective, SettingMeta } from '$lib/api/settings';

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/CompressionForwardingState',
		component: CompressionForwardingState,
		tags: ['autodocs'],
		parameters: {
			docs: {
				description: {
					component:
						'The effective forwarding state of file read compression: a toned label and the ' +
						"server's own summary, rendered as phrasing content so it fits the section description. " +
						'An unknown state shows the summary alone.'
				}
			}
		}
	});

	const SESSION_ARM =
		"each request's manifest records its session's arm in the file_read_compression stage's resolved_parameters.arm";

	const SHADOW_ONLY: CompressionEffective = {
		state: 'pure_shadow',
		mode: 'shadow',
		forward_share: 0,
		applies: 'new_sessions',
		enabled: true,
		summary:
			'No session is sent shortened requests; the compressor only records what it would cut.',
		session_arm: SESSION_ARM
	};
	const SAMPLED: CompressionEffective = {
		state: 'sampled_forwarding',
		mode: 'shadow',
		forward_share: 20,
		applies: 'new_sessions',
		enabled: true,
		summary:
			'20% of new sessions (by session id) are sent shortened requests; the rest are the shadow control.',
		session_arm: SESSION_ARM
	};
	const FORCED: CompressionEffective = {
		state: 'forced_forwarding',
		mode: 'forward',
		forward_share: 20,
		applies: 'new_sessions',
		enabled: true,
		summary: 'Every new session is sent shortened requests; forward mode overrides the share.',
		session_arm: SESSION_ARM
	};
	const OFF: CompressionEffective = {
		state: 'off',
		mode: 'shadow',
		forward_share: 20,
		applies: 'new_sessions',
		enabled: false,
		summary:
			'File-read compression is off: no session is sent shortened requests and nothing is recorded. Mode and share take effect for new sessions once it is turned on.',
		session_arm: SESSION_ARM
	};

	function rows(effective: CompressionEffective): SettingMeta[] {
		return [
			{
				key: 'file_read_compression.mode',
				group: 'File read compression',
				kind: 'enum',
				scope: 'session',
				applies: 'new_sessions',
				default: 'shadow',
				value: effective.mode,
				label: 'Mode',
				description: 'Forward sends every new session shortened requests, whatever the share.',
				enum_variants: ['shadow', 'forward']
			},
			{
				key: 'file_read_compression.forward_share',
				group: 'File read compression',
				kind: 'int',
				scope: 'session',
				applies: 'new_sessions',
				default: 0,
				value: effective.forward_share,
				label: 'Share of sessions forwarded',
				description: 'In shadow mode, the percent of new sessions sent shortened requests.',
				min_i64: 0,
				max_i64: 100
			}
		];
	}
</script>

{#snippet frame(effective: CompressionEffective)}
	<div style="max-width:720px; padding:24px; background:var(--surface-bg);">
		<SettingsSection
			title="File read compression"
			id="file-read-compression"
			level="h3"
			badge="Applies to new sessions"
			badgeVariant="session"
		>
			{#snippet description()}
				<CompressionForwardingState {effective} />
			{/snippet}
			{#each rows(effective) as setting (setting.key)}
				<SettingRow {setting} onSave={() => {}} />
			{/each}
		</SettingsSection>
	</div>
{/snippet}

<Story
	name="ShadowOnly"
	play={async ({ canvasElement }) => {
		const badge = canvasElement.querySelector('.crf [data-slot="badge"]');
		await expect(badge?.textContent?.trim()).toBe('Shadow only');
		await expect(badge?.classList.contains('badge-muted')).toBe(true);
		await expect(canvasElement.querySelector('.crf-summary')?.textContent).toBe(
			SHADOW_ONLY.summary
		);
	}}
>
	{#snippet template()}
		{@render frame(SHADOW_ONLY)}
	{/snippet}
</Story>

<Story
	name="SampledForwarding"
	play={async ({ canvasElement }) => {
		const badge = canvasElement.querySelector('.crf [data-slot="badge"]');
		await expect(badge?.textContent?.trim()).toBe('Sampled forwarding: 20% of new sessions');
		await expect(badge?.classList.contains('badge-warning')).toBe(true);
		await expect(canvasElement.querySelector('.crf-summary')?.textContent).toBe(SAMPLED.summary);
		const desc = canvasElement.querySelector('p.cfg-sec-desc');
		await expect(
			desc?.querySelector('.crf'),
			'the state sits inside the description'
		).not.toBeNull();
	}}
>
	{#snippet template()}
		{@render frame(SAMPLED)}
	{/snippet}
</Story>

<Story
	name="ForwardingEverySession"
	play={async ({ canvasElement }) => {
		const badge = canvasElement.querySelector('.crf [data-slot="badge"]');
		await expect(badge?.textContent?.trim()).toBe('Forwarding every session');
		await expect(badge?.classList.contains('badge-warning')).toBe(true);
		await expect(canvasElement.querySelector('.crf-summary')?.textContent).toBe(FORCED.summary);
	}}
>
	{#snippet template()}
		{@render frame(FORCED)}
	{/snippet}
</Story>

<Story
	name="Off"
	play={async ({ canvasElement }) => {
		const badge = canvasElement.querySelector('.crf [data-slot="badge"]');
		await expect(badge?.textContent?.trim()).toBe('Off');
		await expect(badge?.classList.contains('badge-muted')).toBe(true);
		await expect(canvasElement.querySelector('.crf-summary')?.textContent).toBe(OFF.summary);
	}}
>
	{#snippet template()}
		{@render frame(OFF)}
	{/snippet}
</Story>
