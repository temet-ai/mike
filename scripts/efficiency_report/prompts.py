"""Measure captured prompt bytes and source matches without exporting captured text."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import re

from .collect import get


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


def normalize(text):
    return re.sub(r'\s+', ' ', text).strip()


def texts(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for block in value:
            if isinstance(block, dict) and isinstance(block.get('text'), str):
                yield block['text']


def sources(root):
    paths = [root / 'CLAUDE.md', root / 'AGENTS.md', root / '.claude/skills/fleet-worker/SKILL.md']
    paths += sorted((root / '.claude/fleet').glob('*.md'))
    paragraphs = {}
    files = []
    for path in paths:
        if not path.is_file():
            continue
        text = path.read_text()
        name = str(path.relative_to(root))
        files.append(dict(path=name, bytes=len(text.encode()), words=len(text.split()), sha256=digest(text)))
        for paragraph in text.split('\n\n'):
            normalized = normalize(paragraph)
            if len(normalized) >= 80:
                entry = paragraphs.setdefault(normalized, set())
                entry.add(name)
    return files, paragraphs


def measure(body, paragraphs):
    system = list(texts(body.get('system')))
    messages = [text for message in body.get('messages', []) for text in texts(message.get('content'))]
    normalized = normalize('\n'.join(system+messages))
    matches = []
    for paragraph, paths in paragraphs.items():
        count = normalized.count(paragraph)
        if count:
            matches.append(dict(sha256=digest(paragraph), normalized_bytes=len(paragraph.encode()),
                                occurrences=count, source_paths=sorted(paths)))
    tools = [json.dumps(t, sort_keys=True, separators=(',', ':')) for t in body.get('tools', [])]
    tool_counts = Counter(digest(t) for t in tools)
    return dict(system_text_bytes=sum(len(t.encode()) for t in system),
                tool_schema_canonical_json_bytes=sum(len(t.encode()) for t in tools),
                tool_count=len(tools), duplicated_tool_definitions=sum(n-1 for n in tool_counts.values()),
                source_matches=sorted(matches, key=lambda m: m['normalized_bytes']*m['occurrences'], reverse=True))


def audit(base, ids, repo):
    files, paragraphs = sources(Path(repo))
    observations = []
    for request_id in sorted(set(ids)):
        for source in ('request', 'request_sent'):
            payload = get(base, f'/api/requests/{request_id}/body/{source}')
            if not payload.get('has_body') or not isinstance(payload.get('text'), str):
                observations.append(dict(request_id=request_id, source=source, available=False))
                continue
            text = payload['text']
            body = json.loads(text)
            observations.append(dict(request_id=request_id, source=source, available=True,
                                     body_sha256=digest(text), body_bytes=len(text.encode()),
                                     reconstructed=payload.get('reconstructed'), **measure(body, paragraphs)))
    return dict(schema='prompt-overhead-audit-v1', repo=str(Path(repo).resolve()), files=files,
                observations=observations, units='UTF-8 bytes and source words, not measured tokens or savings',
                coverage='Explicit request sample; exact normalized paragraph matches, which can overlap; raw captured text is never exported')
