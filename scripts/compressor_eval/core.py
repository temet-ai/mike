"""Offline result-to-follow-up edges with deduplicated observed costs."""
from collections import Counter, defaultdict
from datetime import datetime
import hashlib

from .commands import inspect

PURPOSES = {'preparation', 'omitted_detail_recovery', 'pagination', 'polling',
            'rerun', 'debugging', 'mixed', 'unrelated'}
NARROWING = {'filter', 'projection', 'slice', 'bounded_read'}


def timestamp(value):
    parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('Timestamps require a timezone')
    return parsed.timestamp()


def text_bytes(content):
    if isinstance(content, str):
        return len(content.encode('utf-8'))
    if isinstance(content, list):
        return sum(len(block['text'].encode('utf-8')) for block in content
                   if isinstance(block, dict) and block.get('type') == 'text'
                   and isinstance(block.get('text'), str))
    return None


def lane(call):
    return (call.get('session_id'), call.get('agent_instance_id'),
            call.get('claude_code_agent_id'))


def unique(rows, key):
    output = {}
    for row in rows:
        identity = row[key]
        if identity in output and output[identity] != row:
            raise ValueError(f'Conflicting records for {key}={identity}')
        output[identity] = row
    return output


def analyze(calls, results, labels=(), window=900, source_ids=None):
    if window <= 0:
        raise ValueError('Window must be positive')
    calls = unique(calls, 'id')
    results = unique(results, 'id')
    for identity, result in results.items():
        if identity in calls and result.get('result_request_id') != calls[identity].get('result_request_id'):
            raise ValueError(f'Result pairing mismatch: {identity}')
    indexed, parsed = defaultdict(list), {}
    for identity, call in calls.items():
        paths, kinds = inspect(call)
        parsed[identity] = (paths, kinds)
        if call.get('session_id') is None:
            continue
        for path in paths:
            indexed[(lane(call), path)].append(call)
    sources = set(results) if source_ids is None else set(source_ids)
    if not sources <= results.keys():
        raise ValueError('Every source ID requires a captured result')
    edges = {}
    eligible = 0
    for identity in sorted(sources):
        result = results[identity]
        source = calls.get(identity)
        if source is None or not source.get('result_at'):
            continue
        if result.get('result_request_id') != source.get('result_request_id'):
            raise ValueError(f'Result pairing mismatch: {identity}')
        paths, _ = parsed[identity]
        eligible += bool(paths)
        for path in sorted(paths):
            for target in indexed[(lane(source), path)]:
                delta = timestamp(target['created_at']) - timestamp(source['result_at'])
                if target['id'] == identity or not 0 <= delta <= window:
                    continue
                key = (identity, target['id'])
                if key not in edges:
                    edges[key] = {
                        'source_tool_id': identity, 'followup_tool_id': target['id'],
                        'source_result_request_id': source['result_request_id'],
                        'followup_request_id': target['request_id'],
                        'followup_result_request_id': target.get('result_request_id'),
                        'seconds_after_receipt': delta,
                        'operations': sorted(parsed[target['id']][1]),
                        'same_operator_turn': source.get('operator_turn_id') == target.get('operator_turn_id'),
                        'agent_identity_known': any(lane(source)[1:]),
                        'artifact_path_hashes': [], 'snapshot_identity': 'unknown',
                        'review': None,
                    }
                edges[key]['artifact_path_hashes'].append(hashlib.sha256(path.encode()).hexdigest())
    for label in labels:
        key = (label['source_tool_id'], label['followup_tool_id'])
        if key not in edges:
            raise ValueError(f'Label does not reference a discovered edge: {key}')
        if edges[key]['review'] is not None:
            raise ValueError(f'Duplicate edge label: {key}')
        if not set(label['purposes']) <= PURPOSES or not label['purposes'] or not label.get('evidence'):
            raise ValueError('Labels require recognized purposes and evidence')
        edges[key]['review'] = label
    records = [edges[k] for k in sorted(edges)]
    connected = sorted({i for pair in edges for i in pair})
    followups = sorted({pair[1] for pair in edges})
    sizes = {i: text_bytes(results[i]['content']) if i in results else None for i in connected}
    return {'version': 1, 'window_seconds': window,
            'counts': {'input_calls': len(calls), 'source_results': len(sources),
                       'sources_with_read_paths': eligible, 'candidate_edges': len(edges),
                       'sources_with_followups': len({k[0] for k in edges}),
                       'sources_with_narrowing_followups': len({k[0] for k, e in edges.items()
                                                              if NARROWING.intersection(e['operations'])}),
                       'unique_calls_in_edges': len(connected), 'unique_followup_calls': len(followups),
                       'unique_emitting_requests_in_edges': len({calls[i]['request_id'] for i in connected}),
                       'reviewed_edges': sum(e['review'] is not None for e in records),
                       'known_result_text_bytes_once_per_call': sum(n for n in sizes.values() if n is not None),
                       'calls_missing_decoded_result': sum(n is None for n in sizes.values())},
            'operation_edges': dict(sorted(Counter(k for e in records for k in e['operations']).items())),
            'edges': records, 'costs': {'actual_model_tokens': None, 'avoided_calls': None,
                                      'task_correctness': None},
            'limitations': ['Path association is not causation or immutable run identity.',
                           'Overlapping edges do not multiply call/result counts.',
                           'Emitting requests are not measured sequential rounds.',
                           'Shell parsing is conservative; unsupported commands remain unclassified.',
                           'No follow-up is not evidence of sufficiency; missing labels remain unknown.',
                           'Text bytes exclude image blocks; captured serialized sizes are not text bytes.']}
