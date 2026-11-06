"""Score explicitly recorded comparative executions; never infer success from bytes."""
from .core import timestamp, text_bytes


def score(run):
    if run.get('execution_kind') != 'actual' or not run.get('execution_ref'):
        raise ValueError('Trial requires an actual execution and its evidence reference')
    if run.get('correct') not in (True, False) or not isinstance(run.get('correct'), bool):
        raise ValueError('Trial correctness requires an independent boolean verdict')
    if not run.get('verdict_ref'):
        raise ValueError('Trial requires independent verdict evidence')
    calls = run['calls']
    if len({c['id'] for c in calls}) != len(calls):
        raise ValueError('Duplicate trial calls')
    duration = timestamp(run['finished_at']) - timestamp(run['started_at'])
    if duration < 0:
        raise ValueError('Trial finishes before it starts')
    sizes = [text_bytes(c.get('content')) for c in calls]
    turns = run.get('model_turns', [])
    if len({t['id'] for t in turns}) != len(turns):
        raise ValueError('Duplicate model turns')
    tokens = None
    if turns and all(isinstance(t.get('input_tokens'), int) and isinstance(t.get('output_tokens'), int)
                     and t['input_tokens'] >= 0 and t['output_tokens'] >= 0 for t in turns):
        tokens = sum(t['input_tokens'] + t['output_tokens'] for t in turns)
    facts = run.get('required_facts', [])
    if len({f['id'] for f in facts}) != len(facts):
        raise ValueError('Duplicate required facts')
    for fact in facts:
        found = fact.get('first_call_id')
        if found is not None and found not in {c['id'] for c in calls}:
            raise ValueError('Fact refers to an absent call')
    return {'correct': run['correct'], 'tool_calls': len(calls), 'model_turns': len(turns),
            'elapsed_seconds': duration, 'actual_total_tokens': tokens,
            'known_result_text_bytes': sum(n for n in sizes if n is not None),
            'missing_results': sum(n is None for n in sizes),
            'required_facts': len(facts),
            'facts_obtained': sum(f.get('first_call_id') is not None for f in facts),
            'facts_in_first_result': sum(bool(calls) and f.get('first_call_id') == calls[0]['id'] for f in facts)}


def compare(pairs):
    output = []
    for pair in pairs:
        original, compact = pair['original'], pair['compact']
        for field in ('task_hash', 'ground_truth_hash', 'artifact_hash', 'model_config_hash'):
            if not original.get(field) or original[field] != compact.get(field):
                raise ValueError(f'Paired arms require identical {field}')
        if {f['id'] for f in original.get('required_facts', [])} != {f['id'] for f in compact.get('required_facts', [])}:
            raise ValueError('Paired arms require identical fact identities')
        a, b = score(original), score(compact)
        output.append({'task_id': pair['task_id'], 'original': a, 'compact': b,
                       'call_delta_compact_minus_original': b['tool_calls'] - a['tool_calls'],
                       'token_delta_compact_minus_original': None if a['actual_total_tokens'] is None or b['actual_total_tokens'] is None
                       else b['actual_total_tokens'] - a['actual_total_tokens'],
                       'both_correct': a['correct'] and b['correct']})
    return output
