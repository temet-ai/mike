<!--
@component
Put causes in a named group: a new group by name, a group that exists, or none to take them out of
the one they are in. A group only changes how the page shows causes, not their order. It closes when
the change worked.
-->
<script lang="ts">
	import type { WrightGroup } from '$lib/api/wright';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Field } from '$lib/components/ui/field';
	import { Select, type SelectOption } from '$lib/components/ui/select';
	import type { GroupTarget } from '$lib/wright/store.svelte';

	interface Props {
		open: boolean;
		/** How many causes the change is for, named in the title. */
		count: number;
		groups: WrightGroup[];
		busy?: boolean;
		/** The server's reason when the last try failed. */
		error?: string | null;
		/** The group to put them in, or null to take them out; true when it worked. */
		onConfirm: (target: GroupTarget | null) => Promise<boolean>;
	}

	let {
		open = $bindable(false),
		count,
		groups,
		busy = false,
		error = null,
		onConfirm
	}: Props = $props();

	const NEW = 'new';
	const NONE = 'none';

	let choice = $state(NEW);
	let name = $state('');
	$effect(() => {
		if (open) {
			choice = groups[0] === undefined ? NEW : String(groups[0].id);
			name = '';
		}
	});

	const options = $derived<SelectOption[]>([
		{ value: NEW, label: 'A new group' },
		...groups.map((g) => ({ value: String(g.id), label: g.name })),
		{ value: NONE, label: 'No group' }
	]);

	const trimmed = $derived(name.trim());
	const taken = $derived(groups.some((g) => g.name === trimmed));
	const nameError = $derived(
		choice !== NEW || trimmed === ''
			? null
			: taken
				? 'A group with that name already exists. Pick it from the list.'
				: null
	);
	const ready = $derived(choice !== NEW || (trimmed !== '' && !taken));

	async function confirm(): Promise<void> {
		const target: GroupTarget | null =
			choice === NONE ? null : choice === NEW ? { name: trimmed } : { id: Number(choice) };
		if (await onConfirm(target)) open = false;
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header>
			<Dialog.Title>Group {count === 1 ? 'this cause' : `${count} causes`}</Dialog.Title>
			<Dialog.Description>
				A group shows its causes together on the By area view. It does not change their order.
			</Dialog.Description>
		</Dialog.Header>
		<Select label="Group" bind:value={choice} {options} />
		<!-- The name field keeps its place in every choice, so picking one does not move the buttons. -->
		<div class="group__name" data-hidden={choice === NEW ? undefined : ''}>
			<Field
				label="Name"
				bind:value={name}
				disabled={choice !== NEW}
				error={nameError}
				placeholder="Mail delivery"
				onkeydown={(event) => {
					if (event.key === 'Enter' && ready && !busy) {
						event.preventDefault();
						void confirm();
					}
				}}
			/>
		</div>
		<p class="group__error" role="alert">{error ?? ''}</p>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
			<Button disabled={busy || !ready} onclick={() => void confirm()}>
				{choice === NONE ? 'Remove from group' : 'Group'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.group__name[data-hidden] {
		visibility: hidden;
	}

	/* The line is reserved, so a refusal appearing does not move the buttons. */
	.group__error {
		margin: 0;
		min-block-size: 1.4em;
		font-size: var(--text-xs);
		color: var(--status-danger-fg);
		overflow-wrap: anywhere;
	}
</style>
