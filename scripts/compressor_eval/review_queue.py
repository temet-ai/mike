"""Local, locked append-only investigation queue. Capture data is never modified."""
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path

KINDS = {'request', 'response', 'tool_result'}
STATUSES = {'open', 'investigating', 'evaluated', 'dismissed'}


def target(kind, request_id, tool_id=None):
    if kind not in KINDS or type(request_id) is not int or request_id <= 0:
        raise ValueError('Target requires a supported kind and positive request ID')
    if kind == 'tool_result':
        if type(tool_id) is not int or tool_id <= 0:
            raise ValueError('Tool-result flags require a positive database tool-use ID')
    elif tool_id is not None:
        raise ValueError('Only tool-result targets accept a tool-use ID')
    return {'kind': kind, 'request_id': request_id, 'tool_use_id': tool_id}


def identity(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def required(value, name):
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f'{name} must be nonempty')
    return value


def reduce_events(events):
    items = {}
    for event in events:
        key = event['id']
        if event['action'] == 'flag':
            if key not in items:
                items[key] = {'id': key, 'target': event['target'], 'status': 'open',
                              'created_at': event['at'], 'history': []}
        if key not in items:
            raise ValueError('Queue event references an absent flag')
        item = items[key]
        item['history'].append(event)
        item['updated_at'] = event['at']
        if event['action'] == 'review':
            item['status'] = event['status']
    return items


@contextmanager
def locked(directory):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    if directory.stat().st_mode & 0o077:
        raise ValueError('Queue directory must be private (mode 0700)')
    path = directory / 'events.jsonl'
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    with os.fdopen(fd, 'r+', encoding='utf-8') as file:
        fcntl.flock(file, fcntl.LOCK_EX)
        if os.fstat(file.fileno()).st_mode & 0o077:
            raise ValueError('Queue event file must be private (mode 0600)')
        events = [json.loads(line) for line in file if line.strip()]
        yield file, events


def append(file, event):
    event = {**event, 'at': datetime.now(timezone.utc).isoformat(), 'version': 1}
    file.seek(0, os.SEEK_END)
    file.write(json.dumps(event, sort_keys=True) + '\n')
    file.flush()
    os.fsync(file.fileno())
    return event


def flag_many(directory, nominations, actor):
    required(actor, 'Actor')
    validated = []
    for nomination in nominations:
        value = target(**nomination['target'])
        # CLI uses tool_id; persisted targets name the underlying database identity.
        validated.append((value, required(nomination['reason'], 'Reason'),
                          required(nomination['evidence'], 'Evidence')))
    results = []
    with locked(directory) as (file, events):
        seen = {e.get('nomination_key') or identity([e['target'], e['reason'], e['evidence'], e['actor']])
                for e in events if e['action'] == 'flag'}
        for value, reason, evidence in validated:
            key = identity(value)
            nomination_key = identity([value, reason, evidence, actor])
            added = nomination_key not in seen
            if added:
                append(file, {'action': 'flag', 'id': key, 'target': value,
                              'reason': reason, 'evidence': evidence, 'actor': actor,
                              'nomination_key': nomination_key})
                seen.add(nomination_key)
            results.append({'id': key, 'added': added})
    return results


def review(directory, key, status, note, actor):
    if status not in STATUSES:
        raise ValueError('Unsupported review status')
    required(note, 'Review note')
    required(actor, 'Actor')
    with locked(directory) as (file, events):
        if key not in reduce_events(events):
            raise ValueError('Unknown flag ID')
        append(file, {'action': 'review', 'id': key, 'status': status, 'note': note, 'actor': actor})
    return {'id': key, 'status': status}


def read(directory):
    with locked(directory) as (_, events):
        return reduce_events(events)


def links(item):
    value = item['target']
    request = value['request_id']
    endpoint = 'response' if value['kind'] == 'response' else 'request'
    return {'original_body': f'/api/requests/{request}/body/{endpoint}',
            'tool_calls': f'/api/requests/{request}/tool-calls',
            'tool_use_id': value['tool_use_id']}


def nominations(report, evidence_ref):
    if report.get('version') != 1 or not isinstance(report.get('edges'), list):
        raise ValueError('Expected a version-1 compressor sequence report')
    output = []
    for edge in report['edges']:
        operations = ', '.join(edge['operations'])
        output.append({'target': {'kind': 'tool_result', 'request_id': edge['source_result_request_id'],
                                  'tool_id': edge['source_tool_id']},
                       'reason': 'Subsequent same-path tool call: ' + operations,
                       'evidence': f"{evidence_ref}; follow-up tool {edge['followup_tool_id']}, request {edge['followup_request_id']}. Path association only; cause and savings unverified."})
    return output
