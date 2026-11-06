import { beforeEach, describe, expect, it } from 'vitest';
import { createLocalStorageMock } from '$lib/test-support/localStorage-mock';

const { storage, localStorageMock } = createLocalStorageMock();

Object.defineProperty(globalThis, 'localStorage', {
	value: localStorageMock,
	writable: true
});

import { createCollapseStore } from './collapseState.svelte';

const STORAGE_KEY = 'proxy-inspector:collapsed';

beforeEach(() => {
	storage.clear();
	localStorageMock.getItem.mockClear();
	localStorageMock.setItem.mockClear();
	sessionStorage.clear();
});

describe('collapseState', () => {
	it('returns false for unknown keys on a fresh store', () => {
		const store = createCollapseStore();
		expect(store.isCollapsed('foo')).toBe(false);
		expect(store.isCollapsed('bar')).toBe(false);
	});

	it('setDefault sets initial state and repeated calls are no-ops', () => {
		const store = createCollapseStore();
		store.setDefault('foo', true);
		expect(store.isCollapsed('foo')).toBe(true);

		store.setDefault('foo', false);
		expect(store.isCollapsed('foo')).toBe(true);
	});

	it('toggle flips collapsed state and back', () => {
		const store = createCollapseStore();
		expect(store.isCollapsed('section-a')).toBe(false);

		store.toggle('section-a');
		expect(store.isCollapsed('section-a')).toBe(true);

		store.toggle('section-a');
		expect(store.isCollapsed('section-a')).toBe(false);
	});

	it('persists collapsed state to localStorage after toggle', () => {
		const store = createCollapseStore();
		store.toggle('detail:headers');

		const stored = storage.get(STORAGE_KEY);
		expect(stored).toBeDefined();
		const parsed = JSON.parse(stored!);
		expect(parsed).toContain('detail:headers');

		store.toggle('detail:headers');
		const stored2 = storage.get(STORAGE_KEY);
		expect(stored2).toBeDefined();
		const parsed2 = JSON.parse(stored2!);
		expect(parsed2).not.toContain('detail:headers');
	});

	it('hydrates from localStorage on creation', () => {
		storage.set(STORAGE_KEY, JSON.stringify(['saved-section', 'another']));

		const store = createCollapseStore();
		expect(store.isCollapsed('saved-section')).toBe(true);
		expect(store.isCollapsed('another')).toBe(true);
		expect(store.isCollapsed('not-saved')).toBe(false);
	});

	it('handles corrupt localStorage gracefully', () => {
		storage.set(STORAGE_KEY, 'not-valid-json{{{');

		const store = createCollapseStore();
		expect(store.isCollapsed('anything')).toBe(false);
	});

	it('setDefault does not override existing localStorage entry', () => {
		storage.set(STORAGE_KEY, JSON.stringify(['detail:tools']));

		const store = createCollapseStore();
		expect(store.isCollapsed('detail:tools')).toBe(true);

		store.setDefault('detail:tools', false);
		expect(store.isCollapsed('detail:tools')).toBe(true);
	});

	it('setDefault(false) locks the key so a later setDefault(true) is a no-op', () => {
		const store = createCollapseStore();

		expect(store.isCollapsed('lockme')).toBe(false);

		store.setDefault('lockme', false);
		expect(store.isCollapsed('lockme')).toBe(false);

		store.setDefault('lockme', true);
		expect(store.isCollapsed('lockme')).toBe(false);
	});

	it('setAll(keys, true) collapses every key and writes localStorage exactly once', () => {
		const store = createCollapseStore();
		localStorageMock.setItem.mockClear();

		store.setAll(['a', 'b', 'c'], true);

		expect(store.isCollapsed('a')).toBe(true);
		expect(store.isCollapsed('b')).toBe(true);
		expect(store.isCollapsed('c')).toBe(true);
		expect(localStorageMock.setItem).toHaveBeenCalledTimes(1);
		const parsed = JSON.parse(storage.get(STORAGE_KEY) ?? '[]');
		expect(new Set(parsed)).toEqual(new Set(['a', 'b', 'c']));
	});

	it('setAll(keys, false) un-collapses every key', () => {
		storage.set(STORAGE_KEY, JSON.stringify(['a', 'b', 'c', 'unrelated']));
		const store = createCollapseStore();
		localStorageMock.setItem.mockClear();

		store.setAll(['a', 'b', 'c'], false);

		expect(store.isCollapsed('a')).toBe(false);
		expect(store.isCollapsed('b')).toBe(false);
		expect(store.isCollapsed('c')).toBe(false);
		expect(store.isCollapsed('unrelated')).toBe(true);
		expect(localStorageMock.setItem).toHaveBeenCalledTimes(1);
	});

	it('setAll([], value) is a no-op', () => {
		const store = createCollapseStore();
		store.toggle('foo');
		localStorageMock.setItem.mockClear();

		store.setAll([], true);
		store.setAll([], false);

		expect(store.isCollapsed('foo')).toBe(true);
		expect(localStorageMock.setItem).not.toHaveBeenCalled();
	});

	it('setAll skips the write when no key actually changes state', () => {
		storage.set(STORAGE_KEY, JSON.stringify(['a', 'b']));
		const store = createCollapseStore();
		localStorageMock.setItem.mockClear();

		store.setAll(['a', 'b'], true);

		expect(localStorageMock.setItem).not.toHaveBeenCalled();
		expect(store.isCollapsed('a')).toBe(true);
		expect(store.isCollapsed('b')).toBe(true);
	});

	it('isExpanded resolves correctly on a fresh store for both defaults', () => {
		const store = createCollapseStore();
		expect(store.isExpanded('foo')).toBe(false);
		expect(store.isExpanded('foo', false)).toBe(false);
		expect(store.isExpanded('foo', true)).toBe(true);
	});

	it('isExpanded reflects toggle for both defaults', () => {
		const store = createCollapseStore();

		store.toggle('df');
		expect(store.isExpanded('df', false)).toBe(true);
		store.toggle('df');
		expect(store.isExpanded('df', false)).toBe(false);

		store.toggle('dt');
		expect(store.isExpanded('dt', true)).toBe(false);
		store.toggle('dt');
		expect(store.isExpanded('dt', true)).toBe(true);
	});

	it('isExpanded is read-only and stores nothing for untoggled keys', () => {
		const store = createCollapseStore();
		localStorageMock.setItem.mockClear();

		store.isExpanded('a');
		store.isExpanded('b', false);
		store.isExpanded('c', true);

		expect(localStorageMock.setItem).not.toHaveBeenCalled();
		expect(storage.get(STORAGE_KEY)).toBeUndefined();
	});

	it('isExpanded reads hydrated state', () => {
		storage.set(STORAGE_KEY, JSON.stringify(['saved']));
		const store = createCollapseStore();

		expect(store.isExpanded('saved', false)).toBe(true);
		expect(store.isExpanded('saved', true)).toBe(false);
		expect(store.isExpanded('absent', false)).toBe(false);
		expect(store.isExpanded('absent', true)).toBe(true);
	});

	it('toggle enforces the size CAP, evicting oldest-first while preserving the just-written key', () => {
		const store = createCollapseStore();
		const CAP = 3000;

		for (let i = 0; i < CAP + 5; i++) {
			store.toggle(`k${i}`);
		}

		const stored = JSON.parse(storage.get(STORAGE_KEY) ?? '[]') as string[];
		expect(stored.length).toBe(CAP);
		expect(stored).toContain(`k${CAP + 4}`);
		expect(store.isCollapsed(`k${CAP + 4}`)).toBe(true);
		expect(stored).not.toContain('k0');
		expect(stored).not.toContain('k4');
		expect(stored).toContain('k5');
	});

	it('loadFromStorage caps an oversized persisted set to CAP, keeping the newest', () => {
		const CAP = 3000;
		const oversized = Array.from({ length: CAP + 10 }, (_, i) => `h${i}`);
		storage.set(STORAGE_KEY, JSON.stringify(oversized));

		const store = createCollapseStore();

		expect(store.isCollapsed('h0')).toBe(false);
		expect(store.isCollapsed('h9')).toBe(false);
		expect(store.isCollapsed('h10')).toBe(true);
		expect(store.isCollapsed(`h${CAP + 9}`)).toBe(true);
	});

	it('B6: a fresh store instance does not re-apply defaults for keys another instance already decided', () => {
		const store1 = createCollapseStore();
		store1.setDefault('turn:1', true);
		store1.setDefault('turn:2', true);
		store1.setDefault('turn:3', false);
		expect(store1.isCollapsed('turn:1')).toBe(true);
		expect(store1.isCollapsed('turn:2')).toBe(true);
		expect(store1.isCollapsed('turn:3')).toBe(false);

		store1.setAll(['turn:1', 'turn:2', 'turn:3'], false);
		expect(store1.isCollapsed('turn:1')).toBe(false);
		expect(store1.isCollapsed('turn:2')).toBe(false);

		const store2 = createCollapseStore();

		store2.setDefault('turn:1', true);
		store2.setDefault('turn:2', true);
		store2.setDefault('turn:3', false);

		expect(store2.isCollapsed('turn:1')).toBe(false);
		expect(store2.isCollapsed('turn:2')).toBe(false);
		expect(store2.isCollapsed('turn:3')).toBe(false);
		const persisted = JSON.parse(storage.get(STORAGE_KEY) ?? '[]') as string[];
		expect(persisted).not.toContain('turn:1');
		expect(persisted).not.toContain('turn:2');
	});

	it('B6: full repro — expand-all, HMR-simulated instance swap, then collapse-all reaches every key', () => {
		const store1 = createCollapseStore();
		const keys = ['turn:1', 'turn:2', 'turn:3', 'turn:4', 'turn:5'];
		for (const k of keys.slice(0, 4)) store1.setDefault(k, true);
		store1.setDefault('turn:5', false);

		store1.toggle('turn:1');
		store1.toggle('turn:2');
		expect(store1.isCollapsed('turn:1')).toBe(false);
		expect(store1.isCollapsed('turn:2')).toBe(false);

		store1.setAll(keys, false);
		for (const k of keys) expect(store1.isCollapsed(k)).toBe(false);

		const store2 = createCollapseStore();
		for (const k of keys.slice(0, 4)) store2.setDefault(k, true);
		store2.setDefault('turn:5', false);

		for (const k of keys) expect(store2.isCollapsed(k)).toBe(false);

		store2.setAll(keys, true);

		for (const k of keys) expect(store2.isCollapsed(k)).toBe(true);
		const persisted = JSON.parse(storage.get(STORAGE_KEY) ?? '[]') as string[];
		expect(new Set(persisted)).toEqual(new Set(keys));
	});

	it('B6: setDefault persists the seeded-lock to sessionStorage so a fresh instance inherits it', () => {
		const store1 = createCollapseStore();
		store1.setDefault('turn:x', true);

		const seededRaw = sessionStorage.getItem('proxy-inspector:collapsed-seeded');
		expect(seededRaw).toBeDefined();
		const seeded = JSON.parse(seededRaw ?? '[]') as string[];
		expect(seeded).toContain('turn:x');

		store1.toggle('turn:x');
		expect(store1.isCollapsed('turn:x')).toBe(false);

		const store2 = createCollapseStore();
		store2.setDefault('turn:x', true);
		expect(store2.isCollapsed('turn:x')).toBe(false);
	});
});
