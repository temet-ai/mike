<script lang="ts">
	import { iconForPath } from './fileExtensionIcon';

	interface Props {
		path: string;
		size?: number;
		class?: string;
	}

	let { path, size = 14, class: klass = '' }: Props = $props();

	const icon = $derived(iconForPath(path));
</script>

<svg
	class={klass}
	width={size}
	height={size}
	viewBox="0 0 {icon.w} {icon.h}"
	role="img"
	aria-hidden="true"
	focusable="false"
>
	<!-- eslint-disable-next-line svelte/no-at-html-tags -->
	{@html icon.body}
</svg>
