import argparse
import hashlib
import html
import json
import os
from pathlib import Path

from .core import analyze
from .trials import compare


def read(path):
    return json.loads(Path(path).read_text())


def main():
    parser = argparse.ArgumentParser(description='Offline compressor tool-sequence evaluation; never executes captured commands.')
    parser.add_argument('--calls', required=True, help='Captured call metadata JSON array')
    parser.add_argument('--results', required=True, help='Decoded captured results JSON array')
    parser.add_argument('--sources', help='Optional fixed source tool-ID array; results can include follow-up evidence')
    parser.add_argument('--labels', help='Reviewed edge labels JSON array')
    parser.add_argument('--trials', help='Optional actual paired executions JSON array')
    parser.add_argument('--window-seconds', type=int, default=900)
    parser.add_argument('--out', required=True, help='New output directory; existing directories are rejected')
    args = parser.parse_args()
    try:
        report = analyze(read(args.calls), read(args.results), read(args.labels) if args.labels else [], args.window_seconds,
                         read(args.sources) if args.sources else None)
        report['comparative_executions'] = compare(read(args.trials)) if args.trials else []
        report['input_sha256'] = {name: hashlib.sha256(Path(path).read_bytes()).hexdigest()
                                  for name, path in [('calls', args.calls), ('results', args.results),
                                                     ('labels', args.labels), ('trials', args.trials), ('sources', args.sources)] if path}
        report['implementation_sha256'] = {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                                            for path in sorted(Path(__file__).parent.glob('*.py'))
                                            if not path.name.startswith('test_')}
        destination = Path(args.out)
        destination.mkdir(mode=0o700, parents=True, exist_ok=False)
        os.chmod(destination, 0o700)
        output = json.dumps(report, indent=2)
        (destination / 'report.json').write_text(output + '\n')
        rows = ''.join(f'<tr><td>{html.escape(k)}</td><td>{v}</td></tr>' for k, v in report['counts'].items())
        page = ('<!doctype html><meta charset="utf-8"><title>Compressor sequence evaluation</title>'
                '<style>body{font:16px/1.5 system-ui;max-width:1100px;margin:40px auto;padding:20px}'
                'td{padding:6px 16px;border:1px solid #ddd}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style>'
                '<h1>Compressor sequence evaluation</h1><p>Observed associations, reviewed evidence and actual comparative executions are separate.</p>'
                f'<table>{rows}</table><h2>Limits</h2><ul>' + ''.join(f'<li>{html.escape(s)}</li>' for s in report['limitations'])
                + '</ul><h2>Complete measurements and provenance</h2><p><a href="report.json">JSON report</a></p><pre>'
                + html.escape(output) + '</pre>')
        (destination / 'report.html').write_text(page)
        for file in destination.iterdir():
            file.chmod(0o600)
        print(json.dumps({'output': str(destination), 'counts': report['counts'],
                          'comparative_executions': len(report['comparative_executions'])}, indent=2))
    except (ValueError, KeyError, OSError, TypeError) as error:
        parser.exit(2, f'Evaluation failed: {error}\n')
