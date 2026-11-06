<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect, userEvent } from 'storybook/test';
	import SettingRow from './SettingRow.svelte';
	import type { SettingMeta, SettingValue } from '$lib/api/settings';

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/SettingRow',
		component: SettingRow,
		tags: ['autodocs']
	});

	function meta(o: Partial<SettingMeta>): SettingMeta {
		return {
			key: 'k',
			group: 'Group',
			kind: 'string',
			scope: 'session',
			applies: 'new_sessions',
			default: '',
			value: '',
			label: 'Label',
			description: 'Description.',
			...o
		} as SettingMeta;
	}

	const INITIAL: SettingMeta[] = [
		meta({
			key: 'analysis.enabled',
			kind: 'bool',
			default: true,
			value: true,
			label: 'Enable analysis',
			description: 'Run weasel-pattern analysis across captured responses.'
		}),
		meta({
			key: 'reminders.max_lessons',
			kind: 'int',
			default: 5,
			value: 5,
			min_i64: 1,
			max_i64: 20,
			label: 'Max lessons',
			description: 'Maximum project lessons injected per turn.'
		}),
		meta({
			key: 'log_level',
			kind: 'enum',
			default: 'info',
			value: 'info',
			enum_variants: ['error', 'warn', 'info', 'debug'],
			label: 'Log level',
			description: 'Proxy log verbosity.'
		}),
		meta({
			key: 'weasel.forbidden_words',
			kind: 'string_list',
			default: [],
			value: ['I have successfully', 'The implementation is complete', 'All tests pass'],
			max_len: 256,
			label: 'Forbidden words',
			description: 'Substrings flagged by the weasel scan. Every item renders in full.'
		}),
		// G10: the on-demand prompt assistant's settings apply next message/next conversation,
		// never "next cycle" (a standing worker's polling loop it doesn't have).
		meta({
			key: 'prompt_assistant.model',
			kind: 'string',
			scope: 'global',
			applies: 'next_message',
			default: 'claude-sonnet-5',
			value: 'claude-sonnet-5',
			label: 'Model',
			description: 'Each message goes to this Claude model through the Agent SDK.'
		}),
		meta({
			key: 'prompt_assistant.read_versions',
			kind: 'bool',
			scope: 'global',
			applies: 'next_conversation',
			default: true,
			value: true,
			label: 'What changed since a version',
			description: 'Lets it answer "what changed since X" questions.'
		})
	];
</script>

<script lang="ts">
	let settings = $state<SettingMeta[]>(INITIAL.map((s) => ({ ...s })));
	function save(key: string, value: SettingValue): void {
		settings = settings.map((s) => (s.key === key ? { ...s, value } : s));
	}
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const sw = canvas.getByRole('switch', { name: /Enable analysis/ });
		await expect(sw.getAttribute('aria-checked')).toBe('true');
		await userEvent.click(sw);
		await expect(sw.getAttribute('aria-checked')).toBe('false');
		const inputValues = [...canvasElement.querySelectorAll('input')].map(
			(i) => (i as HTMLInputElement).value
		);
		await expect(inputValues).toContain('The implementation is complete');
		await expect(inputValues).toContain('All tests pass');

		await expect(await canvas.findByText('Next message')).toBeInTheDocument();
		await expect(await canvas.findByText('Next conversation')).toBeInTheDocument();
		await expect(canvas.queryByText('next cycle')).toBeNull();
	}}
>
	{#snippet template()}
		<div
			style="max-width:640px; display:flex; flex-direction:column; border:1px solid var(--border-default); border-radius:8px; overflow:hidden; background:var(--surface-card);"
		>
			{#each settings as setting (setting.key)}
				<SettingRow {setting} onSave={(k, v) => save(k, v)} />
			{/each}
		</div>
	{/snippet}
</Story>
