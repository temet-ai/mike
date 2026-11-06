export function tooltipTriggerProps(
	props: Record<string, unknown>,
	onclick: (event: MouseEvent) => void
): Record<string, unknown> {
	const dismiss = props.onclick as ((event: MouseEvent) => void) | undefined;
	return {
		...props,
		onclick: (event: MouseEvent) => {
			dismiss?.(event);
			onclick(event);
		}
	};
}
