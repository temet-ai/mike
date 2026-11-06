import { type VariantProps, tv } from 'tailwind-variants';

export const badgeVariants = tv({
	base: '',
	variants: {
		variant: {
			'pill-complete': 'status-pill status-pill-complete',
			'pill-pending': 'status-pill status-pill-infly status-pill-pending',
			'pill-streaming': 'status-pill status-pill-infly status-pill-streaming',
			'pill-error': 'status-pill status-pill-error',
			'pill-unknown': 'status-pill status-pill-unknown',
			'chip-active': 'badge-nx badge-active',
			'chip-muted': 'badge-nx badge-muted',
			'chip-warning': 'badge-nx badge-warning',
			'chip-error': 'badge-nx badge-error',
			'chip-analyzed': 'badge-nx badge-analyzed',
			'tag-default': 'tag',
			'tag-group': 'tag tag--group',
			'tag-block': 'tag tag--block',
			'tag-green': 'tag tag--green',
			'tag-blue': 'tag tag--blue',
			'tag-amber': 'tag tag--amber',
			identity: 'class-chip',
			'identity-active': 'class-chip class-chip-active'
		}
	},
	defaultVariants: { variant: 'tag-default' }
});

export type BadgeVariant = NonNullable<VariantProps<typeof badgeVariants>['variant']>;

const CLIPPED_VARIANTS: ReadonlySet<BadgeVariant> = new Set([
	'pill-complete',
	'pill-pending',
	'pill-streaming',
	'pill-error',
	'pill-unknown',
	'chip-active',
	'chip-muted',
	'chip-warning',
	'chip-error',
	'chip-analyzed'
]);

export function needsClip(variant: BadgeVariant): boolean {
	return CLIPPED_VARIANTS.has(variant);
}
