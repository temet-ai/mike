import argparse
import json
import os
from pathlib import Path
import time
import uuid

from .collector import Client, atomic_write, sync_sources
from .lessons import sync_lessons
from .machine import default_state_dir, machine_identity


def register(client, repo, config, machine):
    return client.request('/api/knowledge/locations', {
        'project': config['project_uuid'], **machine, 'root_path': str(repo)})


def initialize(client, repo, args, machine):
    config_path = repo / '.mike' / 'knowledge.json'
    if config_path.exists():
        raise ValueError(f'{config_path} exists; use sync or edit the source configuration')
    if not args.project and args.name:
        defined = client.request('/api/knowledge/projects', {'name': args.name})
        args.project = defined['project_uuid']
    if not args.project:
        raise ValueError('init requires --project or --name for a new logical project')
    location = client.request('/api/knowledge/locations', {
        'project': args.project, **machine, 'root_path': str(repo)})
    sources = []
    for name in ['artifacts', 'docs']:
        if (repo / name).is_dir():
            sources.append({'name': name, 'path': name, 'exclude': ['knowledge']})
    config = {'version': 1, 'project_uuid': location['project_uuid'], 'sources': sources}
    atomic_write(config_path, json.dumps(config, indent=2) + '\n')
    print(json.dumps({'config': str(config_path), **location}, indent=2))


def promote(client, repo, config, args):
    from urllib.parse import urlencode
    if not args.reference:
        raise ValueError('promote requires --reference note:<uuid>')
    if not args.reference.startswith('note:'):
        raise ValueError('promote accepts a session note reference')
    note = client.request('/api/knowledge/read?' + urlencode({
        'project': config['project_uuid'], 'reference': args.reference}))
    import yaml
    metadata = {'artifact_uuid': str(uuid.uuid5(uuid.UUID(config['project_uuid']), args.reference)),
                'title': note['title'], 'kind': note['kind'], 'status': note['status'] or 'unknown',
                'source_reference': args.reference, 'source_updated_at': note['updated_at'],
                'session_id': [note['session_uuid']], 'source_data': note['data']}
    target = repo / 'artifacts' / 'decisions' / (args.reference.replace(':', '-') + '.md')
    content = '---\n' + yaml.safe_dump(metadata, sort_keys=False) + '---\n\n' + note['body']
    if target.exists() and target.read_text() != content:
        raise ValueError(f'{target} already exists with different content; resolve it explicitly')
    atomic_write(target, content)
    print(target)


def sync_once(client, repo, config, machine):
    location = register(client, repo, config, machine)['location_uuid']
    result = {}
    errors = []
    for key, step in (('lessons', sync_lessons),
                      ('sources', lambda c, r, loc: sync_sources(c, r, config, loc))):
        try:
            result[key] = step(client, repo, location)
        except (OSError, ValueError, RuntimeError) as error:
            errors.append(f'{key}: {error}')
    if errors:
        result['error'] = '; '.join(errors)
    return result


def main():
    parser = argparse.ArgumentParser(description='Portable project knowledge collector (requires PyYAML)')
    parser.add_argument('command', choices=['projects', 'init', 'sync', 'watch', 'promote', 'session'])
    parser.add_argument('--endpoint', default=os.environ.get('MIKE_INSPECT_URL', 'http://localhost:9989'))
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--project')
    parser.add_argument('--name')
    parser.add_argument('--session-id')
    parser.add_argument('--goal')
    parser.add_argument('--reference')
    parser.add_argument('--interval', type=float, default=30)
    parser.add_argument('--state-dir', type=Path, default=default_state_dir())
    args = parser.parse_args()
    client = Client(args.endpoint)
    repo = args.repo.resolve()
    try:
        if args.command == 'projects':
            print(json.dumps(client.request('/api/knowledge/locations'), indent=2))
            return
        machine = machine_identity(args.state_dir)
        if args.command == 'init':
            initialize(client, repo, args, machine)
            return
        config = json.loads((repo / '.mike/knowledge.json').read_text())
        if config.get('version') != 1:
            raise ValueError('Unsupported knowledge configuration version')
        if args.command == 'session':
            if not args.session_id:
                raise ValueError('session requires --session-id')
            location = register(client, repo, config, machine)
            result = client.request('/api/workspace/register', {
                'session_id': args.session_id, 'cwd': str(repo), 'registered_via': 'mcp',
                'location_uuid': location['location_uuid'], 'intent': args.goal})
            print(json.dumps(result, indent=2))
            return
        if args.command == 'promote':
            promote(client, repo, config, args)
            return
        if args.interval <= 0:
            raise ValueError('interval must be positive')
        while True:
            try:
                result = sync_once(client, repo, config, machine)
                print(json.dumps(result), flush=True)
                if 'error' in result and args.command != 'watch':
                    raise RuntimeError(result['error'])
            except (OSError, ValueError, RuntimeError) as error:
                if args.command != 'watch':
                    raise
                print(f'Sync failed; previous index preserved for unscanned sources: {error}', flush=True)
            if args.command != 'watch':
                break
            time.sleep(args.interval)
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f'{error}\n')
