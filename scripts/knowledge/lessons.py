import hashlib
import json
from pathlib import Path
from urllib.parse import urlencode

import yaml

from .collector import atomic_write


def render(payload, version):
    metadata = {k: v for k, v in payload.items() if k != 'body'}
    metadata['sync_version'] = version
    metadata['sync_hash'] = digest(json.dumps(payload, sort_keys=True))
    return '---\n' + yaml.safe_dump(metadata, sort_keys=False, allow_unicode=True) + '---\n' + payload.get('body', '')


def parse(content):
    if not content.startswith('---\n') or '\n---\n' not in content[4:]:
        raise ValueError('Lesson file requires YAML frontmatter')
    header, body = content[4:].split('\n---\n', 1)
    data = yaml.safe_load(header)
    if not isinstance(data, dict) or 'lesson_uuid' not in data:
        raise ValueError('Portable lesson_uuid is required')
    version = int(data.pop('sync_version', 0))
    data.pop('sync_hash', None)
    data = json.loads(json.dumps(data, default=str))
    data['body'] = body
    return data, version


def digest(content):
    return hashlib.sha256(content.encode()).hexdigest()


def sync_lessons(client, repo, location):
    response = client.request('/api/knowledge/lessons?' + urlencode({'location_uuid': location}))
    remote = {row['payload']['lesson_uuid']: row for row in response['lessons']}
    directory = repo / '.claude' / 'lessons'
    local = {}
    if directory.exists():
        for path in sorted(directory.glob('*.md')):
            content = path.read_bytes().decode('utf-8')
            if 'lesson_uuid:' not in content:
                continue  # Pre-portability exports remain untouched; DB backfill supplies UUID copies.
            payload, version = parse(content)
            identity = payload['lesson_uuid']
            if identity in local:
                raise ValueError(f'Duplicate lesson UUID: {identity}')
            local[identity] = (path, payload, version, content)
    conflicts = []
    receipts = []
    for identity in sorted(set(remote) | set(local)):
        row = remote.get(identity)
        item = local.get(identity)
        if row and item:
            path, payload, version, content = item
            stored_hash = yaml.safe_load(content[4:].split('\n---\n', 1)[0]).get('sync_hash')
            modified = stored_hash != digest(json.dumps(payload, sort_keys=True))
            if payload != row['payload'] and version != row['version'] and modified:
                conflicts.append(identity)
                continue
            if payload != row['payload'] and modified:
                updated = client.request('/api/knowledge/lessons', {
                    'location_uuid': location, 'expected_version': version, 'payload': payload})
                row = {'payload': updated.get('payload', payload), 'version': updated['version']}
        elif item:
            path, payload, version, content = item
            if payload.get('deleted'):
                continue
            updated = client.request('/api/knowledge/lessons', {
                'location_uuid': location, 'expected_version': 0, 'payload': payload})
            row = {'payload': updated.get('payload', payload), 'version': updated['version']}
        else:
            path = directory / (identity + '.md')
            content = None
        rendered = render(row['payload'], row['version'])
        # A second read closes the normal editor-vs-scan window before replacement.
        if path.exists() and path.read_bytes().decode('utf-8') != content:
            conflicts.append(identity)
            continue
        atomic_write(path, rendered)
        receipts.append({'lesson_uuid': identity, 'version': row['version']})
    if receipts:
        client.request('/api/knowledge/lessons/ack', {'location_uuid': location, 'receipts': receipts})
    if conflicts:
        raise RuntimeError('Lesson sync conflicts (files preserved): ' + ', '.join(conflicts))
    return {'lesson_files': len(receipts)}
