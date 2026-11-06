import { beforeEach, describe, expect, it } from 'vitest';
import { createLocalStorageMock } from '$lib/test-support/localStorage-mock';

const { storage, localStorageMock } = createLocalStorageMock();

Object.defineProperty(globalThis, 'localStorage', {
	value: localStorageMock,
	writable: true
});

import {
	formatProjectParam,
	GLOBAL_PROJECT_PARAM,
	loadRememberedProject,
	parseProjectParam,
	resolveInitialProject,
	saveRememberedProject,
	WORKBENCH_PROJECT_STORAGE_KEY
} from './workbenchProject';

beforeEach(() => {
	storage.clear();
});

describe('parseProjectParam', () => {
	it('parses a project path through unchanged', () => {
		expect(parseProjectParam('/mnt/data/projects/temet/temet-proxy')).toBe(
			'/mnt/data/projects/temet/temet-proxy'
		);
	});

	it('parses missing, empty, or "global" as Global (null)', () => {
		expect(parseProjectParam(null)).toBeNull();
		expect(parseProjectParam('')).toBeNull();
		expect(parseProjectParam(GLOBAL_PROJECT_PARAM)).toBeNull();
	});
});

describe('formatProjectParam', () => {
	it('formats a project path as itself', () => {
		expect(formatProjectParam('/mnt/data/projects/other')).toBe('/mnt/data/projects/other');
	});

	it('formats Global (null) as "global"', () => {
		expect(formatProjectParam(null)).toBe('global');
	});
});

describe('loadRememberedProject / saveRememberedProject', () => {
	it('round-trips a saved project path', () => {
		saveRememberedProject('/mnt/data/projects/other');
		expect(loadRememberedProject()).toBe('/mnt/data/projects/other');
	});

	it('returns null when nothing is stored', () => {
		expect(loadRememberedProject()).toBeNull();
	});

	it('clears the stored key when saving Global (null)', () => {
		saveRememberedProject('/mnt/data/projects/other');
		saveRememberedProject(null);
		expect(loadRememberedProject()).toBeNull();
		expect(storage.has(WORKBENCH_PROJECT_STORAGE_KEY)).toBe(false);
	});
});

describe('resolveInitialProject', () => {
	it('prefers an explicit URL project param over a remembered one', () => {
		saveRememberedProject('/mnt/data/projects/remembered');
		expect(resolveInitialProject('/mnt/data/projects/from-url')).toBe(
			'/mnt/data/projects/from-url'
		);
	});

	it('an explicit URL "global" overrides a remembered project rather than being ignored', () => {
		saveRememberedProject('/mnt/data/projects/remembered');
		expect(resolveInitialProject(GLOBAL_PROJECT_PARAM)).toBeNull();
	});

	it('falls back to the remembered project when the URL carries no param', () => {
		saveRememberedProject('/mnt/data/projects/remembered');
		expect(resolveInitialProject(null)).toBe('/mnt/data/projects/remembered');
	});

	it('falls back to Global when neither the URL nor localStorage says otherwise', () => {
		expect(resolveInitialProject(null)).toBeNull();
	});
});
