import { type VariantProps, tv } from 'tailwind-variants';

export const tagVariants = tv({
	base: 'tag',
	variants: {
		variant: {
			default: '',
			group: 'tag--group',
			block: 'tag--block',
			hook: 'tag--green',
			boot: 'tag--blue',
			'event-info': 'tag--blue',
			'event-warn': 'tag--amber',
			catchall: ''
		}
	},
	defaultVariants: { variant: 'default' }
});

export type TagVariant = VariantProps<typeof tagVariants>['variant'];
