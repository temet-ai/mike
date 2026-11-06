<script lang="ts">
	import { ListTodo, CircleCheck, CircleDot, Circle } from '@lucide/svelte';
	import { isRecord } from './toolRegistry';

	let { input }: { input: unknown } = $props();

	type TodoStatus = 'pending' | 'in_progress' | 'completed' | 'unknown';
	interface Todo {
		content: string;
		status: TodoStatus;
	}

	function normStatus(v: unknown): TodoStatus {
		return v === 'pending' || v === 'in_progress' || v === 'completed' ? v : 'unknown';
	}

	const todos = $derived.by<Todo[]>(() => {
		if (!isRecord(input)) return [];
		const arr = input.todos;
		if (!Array.isArray(arr)) return [];
		return arr.filter(isRecord).map((t) => ({
			content:
				typeof t.content === 'string'
					? t.content
					: typeof t.activeForm === 'string'
						? t.activeForm
						: '',
			status: normStatus(t.status)
		}));
	});

	const MARKER_ICON = {
		completed: CircleCheck,
		in_progress: CircleDot,
		pending: Circle,
		unknown: Circle
	} as const;
</script>

{#if todos.length > 0}
	<div class="todo-call">
		<div class="todo-call__head">
			<ListTodo size={13} aria-hidden="true" class="todo-call__icon" />
			<span class="todo-call__count">{todos.length} items</span>
		</div>
		<ul class="todo-call__list" role="list">
			{#each todos as todo, i (i)}
				{@const MarkerIcon = MARKER_ICON[todo.status]}
				<li class="todo-call__item todo-call__item--{todo.status}">
					<span class="todo-call__marker" aria-hidden="true">
						<MarkerIcon size={13} />
					</span>
					<span class="todo-call__content">{todo.content}</span>
				</li>
			{/each}
		</ul>
	</div>
{:else}
	<span class="todo-call__empty">No todos in input.</span>
{/if}

<style>
	.todo-call {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.todo-call__head {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.todo-call :global(.todo-call__icon) {
		color: var(--agent-2-fg);
		flex-shrink: 0;
	}
	.todo-call__count {
		font-size: var(--text-label);
		color: var(--text-3);
		font-variant-numeric: tabular-nums;
	}
	.todo-call__list {
		display: flex;
		flex-direction: column;
		gap: 3px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.todo-call__item {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 3px 0;
	}
	.todo-call__marker {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 14px;
		color: var(--text-3);
	}
	.todo-call__item--completed .todo-call__marker {
		color: var(--status-success-fg);
	}
	.todo-call__item--in_progress .todo-call__marker {
		color: var(--agent-2-fg);
	}
	.todo-call__content {
		font-size: 12px;
		color: var(--text-1);
		line-height: 1.5;
		word-break: break-word;
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.todo-call__item--completed .todo-call__content {
		color: var(--text-3);
	}
	.todo-call__item--in_progress .todo-call__content {
		color: var(--text-strong);
		font-weight: 500;
	}
	.todo-call__empty {
		font-size: var(--text-label);
		font-style: italic;
		color: var(--text-faint);
	}
</style>
