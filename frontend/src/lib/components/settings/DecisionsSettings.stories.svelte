<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import type { FeatureDto, SettingMeta } from '$lib/api/settings';
	import DecisionsSettings from './DecisionsSettings.svelte';

	const onToggleFeature = fn();
	const onSaveSetting = fn();

	const FEATURES: FeatureDto[] = [
		{ key: 'decisions', enabled: true, scope: 'global', applies: 'immediate' }
	];

	function setting(
		key: string,
		kind: SettingMeta['kind'],
		value: SettingMeta['value'],
		label: string,
		description: string,
		extra: Partial<SettingMeta> = {}
	): SettingMeta {
		return {
			key,
			group: 'Decisions',
			kind,
			scope: 'global',
			applies: 'next_message',
			default: value,
			value,
			label,
			description,
			...extra
		};
	}

	const SETTINGS: SettingMeta[] = [
		setting(
			'decisions.model',
			'string',
			'typesafe/jev-1.14',
			'Decision model',
			'The OpenRouter model that answers every decision point.',
			{
				max_len: 256
			}
		),
		setting(
			'decisions.timeout_ms',
			'int',
			3000,
			'Timeout (ms)',
			'How long mike waits for Jev before acting as if the layer were off.',
			{
				min_i64: 200,
				max_i64: 30000
			}
		),
		setting(
			'decisions.rate_per_minute',
			'int',
			120,
			'Calls per minute',
			'The most decision calls mike makes in one minute.',
			{
				min_i64: 1,
				max_i64: 5000
			}
		),
		setting(
			'decisions.max_calls_per_hour',
			'int',
			3000,
			'Calls per hour',
			'The most decision calls mike makes in one hour.',
			{
				min_i64: 1,
				max_i64: 100000
			}
		),
		setting(
			'decisions.daily_spend_ceiling_usd',
			'float',
			5,
			'Daily spend ceiling (USD)',
			"Once today's decision spend reaches this many dollars, every decision point takes its off path until midnight UTC. 0 means no ceiling.",
			{ min_f64: 0, max_f64: 1000 }
		),
		setting(
			'decisions.turn_guard.hold_below',
			'float',
			0.3,
			'Hold below',
			'A turn-guard nudge scoring under this value is held.',
			{
				min_f64: 0,
				max_f64: 1
			}
		),
		setting(
			'decisions.turn_judge.mode',
			'enum',
			'on',
			'Turn judge mode',
			'Mode of one decision point.',
			{
				enum_variants: ['off', 'shadow', 'on']
			}
		)
	];

	const { Story } = defineMeta({
		title: 'Settings/DecisionsSettings',
		component: DecisionsSettings,
		tags: ['autodocs'],
		argTypes: { features: { control: false }, settings: { control: false } }
	});
</script>

<Story
	name="Default"
	tags={['interactive']}
	args={{ features: FEATURES, settings: SETTINGS, onToggleFeature, onSaveSetting }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByText('Jev decisions')).toBeVisible();
		await expect(canvas.getByText('Hold below')).toBeVisible();
		const hourly = canvas.getByText('Calls per hour');
		const ceiling = canvas.getByText('Daily spend ceiling (USD)');
		await expect(ceiling).toBeVisible();
		await expect(
			hourly.compareDocumentPosition(ceiling) & Node.DOCUMENT_POSITION_FOLLOWING
		).toBeTruthy();
		const ceilingInput = canvas.getByDisplayValue('5') as HTMLInputElement;
		await expect(ceilingInput.type).toBe('number');
		await expect(canvas.queryByText('Turn judge mode')).toBeNull();
		await userEvent.click(canvas.getByRole('switch'));
		await expect(onToggleFeature).toHaveBeenCalledWith('decisions', false);
	}}
>
	{#snippet template(args)}
		<div style="width: 44rem; background: var(--surface-bg); padding: var(--space-4);">
			<DecisionsSettings {...args} />
		</div>
	{/snippet}
</Story>
