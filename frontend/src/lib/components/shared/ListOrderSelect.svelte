<script lang="ts">
	import { Select, type SelectOption } from '$lib/components/ui/select';
	import type { ListOrder } from '$lib/suggestedOrder';

	interface Props {
		order: ListOrder;
		onChange: (order: ListOrder) => void;
		disabled?: boolean;
	}

	const { order, onChange, disabled = false }: Props = $props();

	const OPTIONS: SelectOption[] = [
		{ value: 'default', label: 'Usual order' },
		{ value: 'suggested', label: "Jev's suggestion" }
	];
</script>

<div class="los" data-slot="list-order">
	<Select
		label="Order"
		value={order}
		options={OPTIONS}
		{disabled}
		onchange={(event) =>
			onChange(
				(event.currentTarget as HTMLSelectElement).value === 'suggested' ? 'suggested' : 'default'
			)}
	/>
</div>

<style>
	.los {
		width: 14rem;
	}
</style>
