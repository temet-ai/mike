// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from 'vitest';
import { createLocalStorageMock } from '$lib/test-support/localStorage-mock';

const { storage, localStorageMock } = createLocalStorageMock();

Object.defineProperty(globalThis, 'localStorage', {
	value: localStorageMock,
	writable: true
});

import { themeStore } from './theme.svelte';
import { THEME_STORAGE_KEY } from '$lib/themes';

beforeEach(() => {
	storage.clear();
	localStorageMock.getItem.mockClear();
	localStorageMock.setItem.mockClear();
	document.documentElement.removeAttribute('data-theme');
	themeStore.set('forge');
	storage.clear();
	localStorageMock.setItem.mockClear();
});

describe('themeStore.set', () => {
	it('writes the [data-theme] attribute on <html>', () => {
		themeStore.set('glacier');
		expect(document.documentElement.dataset.theme).toBe('glacier');

		themeStore.set('phosphor');
		expect(document.documentElement.dataset.theme).toBe('phosphor');
	});

	it('persists the chosen theme to localStorage', () => {
		themeStore.set('void');
		expect(localStorageMock.setItem).toHaveBeenCalledWith(THEME_STORAGE_KEY, 'void');
		expect(storage.get(THEME_STORAGE_KEY)).toBe('void');
	});

	it('updates the reactive current field', () => {
		themeStore.set('crimson');
		expect(themeStore.current).toBe('crimson');
	});

	it('ignores unknown ids (DOM + storage + state unchanged)', () => {
		themeStore.set('glacier');
		// @ts-expect-error — deliberately passing an invalid id to assert the guard.
		themeStore.set('not-a-theme');
		expect(themeStore.current).toBe('glacier');
		expect(document.documentElement.dataset.theme).toBe('glacier');
		expect(storage.get(THEME_STORAGE_KEY)).toBe('glacier');
	});
});
