<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import SteererFleet from './SteererFleet.svelte';
	import type { SteererDto, SteererEnvelope } from '$lib/types';
	import { stubFetchJsonRouter, typed } from '$lib/test-support/storyFetchStubs';

	function buildSteerer(
		overrides: Partial<SteererDto> & Pick<SteererDto, 'id' | 'name'>
	): SteererDto {
		return {
			uuid: `uuid-${overrides.id}`,
			display_name: overrides.name,
			description: '',
			agent_id: 1,
			authority: 'redirect',
			mode: 'augment',
			model: 'google/gemini-3.5-flash-lite',
			priority: 0,
			enabled: true,
			auto_enrol: false,
			trigger_point: 'response',
			invocation: 'every_turn',
			pattern: null,
			config: { confidence_threshold: 0.7, debounce_turns: 3 },
			created_at: '2026-06-01T00:00:00Z',
			updated_at: '2026-06-01T00:00:00Z',
			active_enrolment_count: 0,
			...overrides
		};
	}

	const STEERERS: SteererDto[] = [
		buildSteerer({
			id: 6,
			name: 'verifier',
			display_name: 'Verifier',
			description: 'Nudges when a turn claims completion without evidence.',
			authority: 'command',
			mode: 'drive',
			auto_enrol: true,
			active_enrolment_count: 1327
		}),
		buildSteerer({
			id: 17,
			name: 'agent-lane-guard',
			display_name: 'Agent-lane guard',
			description: 'Nudges when code is edited by the wrong actor.',
			authority: 'command',
			mode: 'drive',
			auto_enrol: true,
			active_enrolment_count: 5
		}),
		buildSteerer({
			id: 1,
			name: 'refocus',
			display_name: 'Refocus',
			description: 'Nudges a drifting session back to its stated goal.',
			auto_enrol: false,
			active_enrolment_count: 1
		}),
		buildSteerer({
			id: 3,
			name: 'memory-keeper',
			display_name: 'Memory keeper',
			description: 'Reminds the session to record durable lessons.',
			auto_enrol: false,
			active_enrolment_count: 0
		})
	];

	const { Story } = defineMeta({
		title: 'Live/SteererFleet',
		component: SteererFleet,
		tags: ['autodocs']
	});
</script>

<Story
	name="Roster"
	loaders={[
		async () => {
			stubFetchJsonRouter((method, url, body) => {
				if (method === 'PATCH' && url.includes('/api/steerers/')) {
					const id = Number(url.split('/').pop());
					const target = STEERERS.find((s) => s.id === id);
					const base = target ?? STEERERS[0];
					if (!base) return typed<{ error: string }>({ error: 'no such steerer' });
					return typed<SteererEnvelope>({
						steerer: { ...base, ...(body as Partial<SteererDto>) },
						warnings: []
					});
				}
				if (url.includes('/api/steerers')) return typed<SteererDto[]>(STEERERS);
				return typed<SteererDto[]>([]);
			});
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		await expect(await canvas.findByText(/1327 active/)).toBeInTheDocument();
		await expect(await canvas.findByText(/5 active/)).toBeInTheDocument();

		await expect(await canvas.findByText(/1 active/)).toBeInTheDocument();
		await expect(await canvas.findByText('· manual')).toBeInTheDocument();

		await expect(await canvas.findByText('0 · manual only')).toBeInTheDocument();

		const verifierToggle = await canvas.findByRole('button', {
			name: /Disable Verifier/
		});
		await userEvent.click(verifierToggle);
		await expect(
			await canvas.findByRole('button', { name: /Enable Verifier/ })
		).toBeInTheDocument();
	}}
>
	{#snippet template()}
		<div class="h-[700px] w-[1100px] overflow-hidden bg-[var(--surface-bg)]">
			<SteererFleet />
		</div>
	{/snippet}
</Story>
