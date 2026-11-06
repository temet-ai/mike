import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FilterRuleDto } from '$lib/types';

vi.mock('$lib/api', async () => {
	const { ApiError } = await import('$lib/test-support/apiError');
	return {
		ApiError,
		listFilterRules: vi.fn(),
		createFilterRule: vi.fn(),
		patchFilterRule: vi.fn(),
		deleteFilterRule: vi.fn()
	};
});

import {
	ApiError,
	createFilterRule,
	deleteFilterRule,
	listFilterRules,
	patchFilterRule
} from '$lib/api';
import { createFilterRulesStore } from './filterRules.svelte';
import { buildFilterRule } from '$lib/test-support';

const mockList = vi.mocked(listFilterRules);
const mockCreate = vi.mocked(createFilterRule);
const mockPatch = vi.mocked(patchFilterRule);
const mockDelete = vi.mocked(deleteFilterRule);

beforeEach(() => {
	mockList.mockReset();
	mockCreate.mockReset();
	mockPatch.mockReset();
	mockDelete.mockReset();
	mockList.mockResolvedValue([]);
});

afterEach(() => {
	vi.restoreAllMocks();
});

describe('createFilterRulesStore — hydration', () => {
	it('keeps disabled rules in the snapshot on refresh (G4: the switch must stay reversible)', async () => {
		mockList.mockResolvedValue([
			buildFilterRule({ id: 'active' }),
			buildFilterRule({ id: 'disabled', enabled: false })
		]);

		const store = createFilterRulesStore();
		await store.refresh();

		expect(store.rules).toHaveLength(2);
		expect(store.rules.map((r) => r.id).sort()).toEqual(['active', 'disabled']);
		expect(store.rules.find((r) => r.id === 'disabled')?.enabled).toBe(false);
	});

	it('initialized is false while the initial fetch is in flight, true after it resolves', async () => {
		let resolveList: (rules: FilterRuleDto[]) => void = () => {};
		mockList.mockImplementation(
			() =>
				new Promise<FilterRuleDto[]>((resolve) => {
					resolveList = resolve;
				})
		);

		const store = createFilterRulesStore();
		expect(store.initialized).toBe(false);
		expect(store.loading).toBe(false);

		const pending = store.refresh();
		expect(store.loading).toBe(true);
		expect(store.initialized).toBe(false);

		resolveList([buildFilterRule()]);
		await pending;
		expect(store.initialized).toBe(true);
		expect(store.loading).toBe(false);
		expect(store.rules).toHaveLength(1);
	});

	it('initialized flips to true even when the initial fetch rejects', async () => {
		mockList.mockRejectedValue(new ApiError('boom', 503, '/api/filter/rules'));
		const store = createFilterRulesStore();
		await store.refresh();
		expect(store.initialized).toBe(true);
		expect(store.error).toBe('boom');
		expect(store.rules).toEqual([]);
	});

	it('concurrent refresh() calls deduplicate to a single fetch', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'x' })]);
		const store = createFilterRulesStore();
		const a = store.refresh();
		const b = store.refresh();
		await Promise.all([a, b]);
		expect(mockList).toHaveBeenCalledTimes(1);
	});
});

describe('create()', () => {
	it('adds the returned rule to the array', async () => {
		const store = createFilterRulesStore();
		await store.refresh();

		const created = buildFilterRule({ id: 'new-1', value: 'foo' });
		mockCreate.mockResolvedValue({ rule: created, warnings: [] });

		const returned = await store.create({
			kind: 'xml_tag',
			value: 'foo',
			action: 'drop'
		});
		expect(returned).toBe(created);
		expect(store.rules).toHaveLength(1);
		expect(store.rules[0]?.id).toBe('new-1');
	});

	it('4xx error leaves rules untouched and exposes the message via error', async () => {
		const store = createFilterRulesStore();
		await store.refresh();
		mockCreate.mockRejectedValue(new ApiError('duplicate (kind, value)', 409, '/api/filter/rules'));

		await expect(store.create({ kind: 'xml_tag', value: 'foo', action: 'drop' })).rejects.toThrow();

		expect(store.rules).toEqual([]);
		expect(store.error).toBe('duplicate (kind, value)');
	});
});

describe('update()', () => {
	it('replaces the matching rule in place, and keeps it listed when disabled (G4)', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'r1', enabled: true })]);
		const store = createFilterRulesStore();
		await store.refresh();

		const updated = buildFilterRule({ id: 'r1', enabled: false });
		mockPatch.mockResolvedValue({ rule: updated, warnings: [] });

		const returned = await store.update('r1', { enabled: false });
		expect(returned).toBe(updated);
		expect(store.rules).toHaveLength(1);
		expect(store.rules[0]?.id).toBe('r1');
		expect(store.rules[0]?.enabled).toBe(false);
	});

	it('turning a disabled rule back on keeps it in place, now enabled (G4: the switch is two-way)', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'r1', enabled: false })]);
		const store = createFilterRulesStore();
		await store.refresh();
		expect(store.rules).toHaveLength(1);

		const reEnabled = buildFilterRule({ id: 'r1', enabled: true });
		mockPatch.mockResolvedValue({ rule: reEnabled, warnings: [] });

		const returned = await store.update('r1', { enabled: true });
		expect(returned).toBe(reEnabled);
		expect(store.rules).toHaveLength(1);
		expect(store.rules[0]?.enabled).toBe(true);
	});

	it('4xx error leaves rules untouched and exposes the message via error', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'r1', value: 'before' })]);
		const store = createFilterRulesStore();
		await store.refresh();
		const before = [...store.rules];

		mockPatch.mockRejectedValueOnce(new ApiError('conflict', 409, '/api/filter/rules/r1'));

		await expect(store.update('r1', { value: 'after' })).rejects.toThrow(ApiError);

		expect(store.rules).toEqual(before);
		expect(store.error).toBe('conflict');
	});
});

describe('remove()', () => {
	it('removes the rule on 204 (empty warnings envelope)', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'r1' }), buildFilterRule({ id: 'r2' })]);
		const store = createFilterRulesStore();
		await store.refresh();
		mockDelete.mockResolvedValue({ warnings: [] });

		await store.remove('r1');
		expect(store.rules).toHaveLength(1);
		expect(store.rules[0]?.id).toBe('r2');
	});

	it('404 on delete reconciles the local array without re-throwing', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'gone' })]);
		const store = createFilterRulesStore();
		await store.refresh();
		mockDelete.mockRejectedValue(new ApiError('not found', 404, '/api/filter/rules/gone'));

		await expect(store.remove('gone')).resolves.toBeUndefined();
		expect(store.rules).toEqual([]);
	});

	it('non-404 error on delete re-throws and leaves rules unchanged', async () => {
		mockList.mockResolvedValue([buildFilterRule({ id: 'r1' })]);
		const store = createFilterRulesStore();
		await store.refresh();
		mockDelete.mockRejectedValue(new ApiError('server error', 500, '/api/filter/rules/r1'));

		await expect(store.remove('r1')).rejects.toThrow();
		expect(store.rules).toHaveLength(1);
		expect(store.error).toBe('server error');
	});
});

describe('handleEvent (Phase B — RowChanged)', () => {
	it('refetches on RowChanged{table:filter_rules}', async () => {
		mockList.mockResolvedValueOnce([buildFilterRule({ id: 'a' })]);
		const store = createFilterRulesStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		mockList.mockResolvedValueOnce([buildFilterRule({ id: 'a' }), buildFilterRule({ id: 'b' })]);
		store.handleEvent({
			type: 'RowChanged',
			table: 'filter_rules',
			id: 'b',
			session_id: null,
			op: 'INSERT'
		});
		await Promise.resolve();
		await Promise.resolve();
		expect(mockList).toHaveBeenCalledTimes(2);
	});

	it('does NOT refetch on RowChanged for unrelated tables', async () => {
		mockList.mockResolvedValueOnce([]);
		const store = createFilterRulesStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({
			type: 'RowChanged',
			table: 'sessions',
			id: 'sess-1',
			session_id: null,
			op: 'UPDATE'
		});
		store.handleEvent({
			type: 'RowChanged',
			table: 'requests',
			id: '42',
			session_id: 'sess-1',
			op: 'INSERT'
		});
		store.handleEvent({
			type: 'RowChanged',
			table: 'prompt_compositions',
			id: 'comp-uuid',
			session_id: null,
			op: 'UPDATE'
		});
		await Promise.resolve();
		await Promise.resolve();
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('does NOT refetch on payload-bearing variants', async () => {
		mockList.mockResolvedValueOnce([]);
		const store = createFilterRulesStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		store.handleEvent({ type: 'TextDelta', id: 1, text: 'hi' });
		store.handleEvent({ type: 'ThinkingDelta', id: 1, thinking: 'mm' });
		store.handleEvent({
			type: 'ResponseComplete',
			id: 1,
			stop_reason: 'end_turn',
			usage: null
		});
		store.handleEvent({ type: 'ToolUseStart', id: 1, tool_id: 'tu', tool_name: 'Read' });
		await Promise.resolve();
		await Promise.resolve();
		expect(mockList).toHaveBeenCalledTimes(1);
	});

	it('refetches on Lagged', async () => {
		mockList.mockResolvedValueOnce([]);
		const store = createFilterRulesStore();
		await store.refresh();
		expect(mockList).toHaveBeenCalledTimes(1);

		mockList.mockResolvedValueOnce([]);
		store.handleEvent({ type: 'Lagged', skipped: 5 });
		await Promise.resolve();
		await Promise.resolve();
		expect(mockList).toHaveBeenCalledTimes(2);
	});
});
