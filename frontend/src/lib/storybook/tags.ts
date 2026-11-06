export const TAG_INTERACTIVE = 'interactive' as const;

export const TAG_VISUAL = 'visual' as const;

export const TAG_DATA_DRIVEN = 'data-driven' as const;

export const TAG_DESIGN_EXPLORATION = 'design-exploration' as const;

export type ProjectTag =
	| typeof TAG_INTERACTIVE
	| typeof TAG_VISUAL
	| typeof TAG_DATA_DRIVEN
	| typeof TAG_DESIGN_EXPLORATION;
