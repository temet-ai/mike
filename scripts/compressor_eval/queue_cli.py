import argparse
import json
import os
from pathlib import Path

from . import review_queue as queue


def main():
    parser = argparse.ArgumentParser(description='Flag captured requests, responses and tool results for compression investigation.')
    parser.add_argument('--state', default=os.environ.get('MIKE_COMPRESSION_QUEUE', str(Path.home() / '.local/state/mike-compression-review')))
    commands = parser.add_subparsers(dest='command', required=True)
    flag = commands.add_parser('flag')
    flag.add_argument('kind', choices=sorted(queue.KINDS))
    flag.add_argument('request_id', type=int, help='Request containing the captured body/result')
    flag.add_argument('--tool-id', type=int, help='Database tool-use ID, not the wire string ID')
    flag.add_argument('--reason', required=True)
    flag.add_argument('--evidence', required=True)
    flag.add_argument('--actor', required=True)
    listing = commands.add_parser('list')
    listing.add_argument('--status', choices=sorted(queue.STATUSES))
    show = commands.add_parser('show')
    show.add_argument('id')
    review = commands.add_parser('review')
    review.add_argument('id')
    review.add_argument('status', choices=sorted(queue.STATUSES))
    review.add_argument('--note', required=True)
    review.add_argument('--actor', required=True)
    importer = commands.add_parser('import-sequences')
    importer.add_argument('report', type=Path)
    importer.add_argument('--actor', required=True)
    args = parser.parse_args()
    try:
        if args.command == 'flag':
            output = queue.flag_many(args.state, [{'target': {'kind': args.kind, 'request_id': args.request_id, 'tool_id': args.tool_id},
                                                  'reason': args.reason, 'evidence': args.evidence}], args.actor)
        elif args.command == 'review':
            output = queue.review(args.state, args.id, args.status, args.note, args.actor)
        elif args.command == 'import-sequences':
            raw = args.report.read_bytes()
            import hashlib
            ref = 'sequence-report-sha256:' + hashlib.sha256(raw).hexdigest()
            output = queue.flag_many(args.state, queue.nominations(json.loads(raw), ref), args.actor)
            output = {'nominations': len(output), 'new_evidence_events': sum(x['added'] for x in output),
                      'unique_flags': len({x['id'] for x in output}), 'report_sha256': ref}
        else:
            items = queue.read(args.state)
            if args.command == 'show':
                item = items[args.id]
                output = {**item, 'capture_links': queue.links(item), 'capture_verified': False}
            else:
                output = [{k: v for k, v in item.items() if k != 'history'}
                          for item in sorted(items.values(), key=lambda x: (x['created_at'], x['id']))
                          if args.status is None or item['status'] == args.status]
        print(json.dumps(output, indent=2))
    except (ValueError, KeyError, OSError, TypeError) as error:
        parser.exit(2, f'Compression review failed: {error}\n')
