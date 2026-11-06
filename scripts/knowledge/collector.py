import hashlib
import json
import os
from pathlib import Path
import tempfile
import subprocess
import urllib.request
import urllib.error

import yaml


def atomic_write(path, content):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix='.mike-')
    try:
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


class Client:
    def __init__(self, endpoint):
        self.endpoint = endpoint.rstrip('/')

    def request(self, path, data=None, method=None):
        payload = None if data is None else json.dumps(data).encode()
        request = urllib.request.Request(self.endpoint + path, data=payload,
                                        headers={'Content-Type': 'application/json'}, method=method)
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'Mike returned {error.code}: {error.read().decode()}') from error


def parse_frontmatter(lines):
    """Read a document's frontmatter without letting one bad file stop a scan.

    A scan is a reconciliation: a document it leaves out is dropped from the
    index. So a file whose frontmatter does not parse is still indexed, by its
    body, and the reason its metadata is missing travels in the metadata.
    """
    if not lines or lines[0] != '---':
        return {}
    try:
        end = lines.index('---', 1)
    except ValueError:
        return {'frontmatter_error': 'unclosed frontmatter'}
    try:
        metadata = yaml.safe_load('\n'.join(lines[1:end])) or {}
    except yaml.YAMLError as error:
        return {'frontmatter_error': f'invalid frontmatter: {error}'}
    if not isinstance(metadata, dict):
        return {'frontmatter_error': 'frontmatter is not an object'}
    return json.loads(json.dumps(metadata, default=str))


def read_document(path, root):
    before = path.stat()
    body = path.read_bytes().decode('utf-8')
    after = path.stat()
    if (before.st_mtime_ns, before.st_size) != (after.st_mtime_ns, after.st_size):
        raise RuntimeError(f'File changed during scan: {path}; retrying next cycle')
    lines = body.splitlines()
    metadata = parse_frontmatter(lines)
    title = metadata.get('title') or next((x[2:] for x in lines if x.startswith('# ')), path.stem)
    return {'relative_path': path.relative_to(root).as_posix(), 'title': str(title),
            'body': body, 'content_hash': hashlib.sha256(body.encode()).hexdigest(),
            'kind': str(metadata.get('kind', metadata.get('type', 'artifact'))),
            'status': str(metadata.get('status', 'unknown')), 'metadata': metadata}


def scan_root(root, excludes):
    root = Path(root)
    if not root.is_dir():
        raise RuntimeError(f'Source unavailable: {root}; previous index is preserved')
    identity = root.stat()
    documents = []
    git_metadata = {}
    try:
        commit = subprocess.run(['git', '-C', str(root), 'rev-parse', 'HEAD'],
                                capture_output=True, text=True, timeout=10, check=True)
        status = subprocess.run(['git', '-C', str(root), 'status', '--porcelain'],
                                capture_output=True, text=True, timeout=10, check=True)
        git_metadata = {'source_git_commit': commit.stdout.strip(),
                        'source_worktree_dirty': bool(status.stdout)}
    except (OSError, subprocess.SubprocessError):
        pass

    def onerror(error):
        raise error

    def excluded(path):
        rel = path.relative_to(root)
        return any(rel == Path(x) or Path(x) in rel.parents for x in excludes)

    for directory, dirs, files in os.walk(root, followlinks=False, onerror=onerror):
        directory = Path(directory)
        dirs[:] = sorted(d for d in dirs if d not in {'.git', 'node_modules', 'target', '.venv'}
                         and not excluded(directory / d) and not (directory / d).is_symlink())
        for name in sorted(files):
            path = directory / name
            if path.suffix.lower() == '.md' and not excluded(path) and not path.is_symlink():
                document = read_document(path, root)
                document['metadata'].update(git_metadata)
                documents.append(document)
    current = root.stat()
    if (current.st_dev, current.st_ino) != (identity.st_dev, identity.st_ino):
        raise RuntimeError('Source changed during scan; refusing reconciliation')
    return documents


def sync_sources(client, repo, config, location):
    results = []
    for item in config['sources']:
        root = (repo / item['path']).absolute()
        source = client.request('/api/knowledge/sources', {
            'location_uuid': location, 'name': item['name'], 'root_path': str(root)})
        documents = scan_root(root, item.get('exclude', []))
        result = client.request(f"/api/knowledge/sources/{source['source_uuid']}/scan", {
            'generation': source['generation'], 'documents': documents})
        results.append({'source': item['name'], **result})
    return results
