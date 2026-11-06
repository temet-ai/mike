import { getContext, setContext } from 'svelte';

export interface RadioGroupContext {
	readonly name: string;
	readonly value: string;
	readonly disabled: boolean;
	select(value: string): void;
}

const KEY = Symbol('radio-group');

export function setRadioGroupContext(ctx: RadioGroupContext): void {
	setContext(KEY, ctx);
}

export function getRadioGroupContext(): RadioGroupContext {
	const ctx = getContext<RadioGroupContext | undefined>(KEY);
	if (!ctx) throw new Error('RadioGroup.Item must be rendered inside RadioGroup.Root');
	return ctx;
}
