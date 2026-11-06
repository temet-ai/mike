<script lang="ts" module>
	import { cn, type WithElementRef } from '$lib/utils';
	import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';
	import { type VariantProps, tv } from 'tailwind-variants';

	export const buttonVariants = tv({
		base: "focus-visible:ring-[var(--ring)] focus-visible:ring-inset aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 rounded-[var(--r-tol)] border border-transparent bg-clip-padding font-[family-name:var(--font-machine)] text-xs font-semibold uppercase tracking-[var(--track-machine)] focus-visible:ring-2 aria-invalid:ring-2 [&_svg:not([class*='size-'])]:size-4 group/button inline-flex shrink-0 items-center justify-center whitespace-nowrap outline-none select-none touch-manipulation disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		variants: {
			variant: {
				default:
					'bg-[var(--status-steering-bg)] text-[var(--status-steering-fg)] hover:shadow-[var(--glow-magenta)]',
				outline:
					'bg-background hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground',
				ghost:
					'hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground',
				destructive:
					'bg-[var(--status-danger-bg)] text-[var(--status-danger-fg)] hover:shadow-[var(--glow-red)]'
			},
			size: {
				default:
					'h-8 gap-2 px-[calc(var(--edge-sm)+var(--space-2))] has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3',
				sm: "h-7 gap-1 px-3 text-xs has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2 [&_svg:not([class*='size-'])]:size-3.5",
				icon: 'size-8'
			}
		},
		defaultVariants: {
			variant: 'default',
			size: 'default'
		}
	});

	export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
	export type ButtonSize = VariantProps<typeof buttonVariants>['size'];

	export type ButtonProps = WithElementRef<HTMLButtonAttributes & HTMLAnchorAttributes> & {
		variant?: ButtonVariant;
		size?: ButtonSize;
	};
</script>

<script lang="ts">
	let {
		ref = $bindable(null),
		class: className,
		variant = 'default',
		size = 'default',
		href = undefined,
		type = 'button',
		disabled,
		children,
		...restProps
	}: ButtonProps = $props();

	const isShaped = $derived(variant !== 'ghost');
</script>

{#if href}
	<!-- eslint-disable svelte/no-navigation-without-resolve -- A generic primitive accepts caller-resolved routes and external URLs. -->
	<a
		href={disabled ? undefined : href}
		bind:this={ref}
		data-slot="button"
		data-variant={variant}
		data-size={size}
		data-motion="hover press"
		data-augmented-ui={isShaped ? 'tr-clip bl-clip border' : undefined}
		class={cn(buttonVariants({ variant, size }), className)}
		aria-disabled={disabled}
		role={disabled ? 'link' : undefined}
		tabindex={disabled ? -1 : undefined}
		{...restProps}
		data-button-root=""
	>
		{@render children?.()}
	</a>
	<!-- eslint-enable svelte/no-navigation-without-resolve -->
{:else}
	<button
		bind:this={ref}
		data-slot="button"
		data-variant={variant}
		data-size={size}
		data-motion="hover press"
		data-augmented-ui={isShaped ? 'tr-clip bl-clip border' : undefined}
		class={cn(buttonVariants({ variant, size }), className)}
		{type}
		{disabled}
		{...restProps}
		data-button-root=""
	>
		{@render children?.()}
	</button>
{/if}

<style>
	:global([data-button-root][data-augmented-ui]) {
		--edge: var(--edge-sm);
		--aug-tl: 0;
		--aug-tr: var(--edge);
		--aug-bl: var(--edge);
		--aug-br: 0;
		--aug-border-all: 1px;
		--aug-border-bg: var(--status-steering-border);
	}
	:global([data-button-root][data-augmented-ui][data-variant='outline']) {
		--aug-border-bg: var(--border-default);
	}
	:global([data-button-root][data-augmented-ui][data-variant='destructive']) {
		--aug-border-bg: var(--status-danger-border);
	}
	:global([data-button-root][data-augmented-ui]:hover) {
		--aug-border-bg: currentColor;
	}
	:global([data-button-root][data-augmented-ui]:focus-visible) {
		--aug-border-bg: var(--interactive-focus-ring);
	}
</style>
