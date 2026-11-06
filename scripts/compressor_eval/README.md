# Compressor sequence evaluation

Offline, standard-library-only evaluation of tool results and subsequent artifact
reads. Does not execute captured commands, query a database, call models or change
compression settings. Invoke from any working directory:

```sh
/path/to/repo/scripts/compressor-sequence-eval \
  --calls /private/call-sequences.json --results /private/results.json \
  --labels /private/reviewed-edges.json --out /private/new-report
python3 -m unittest discover -s scripts/compressor_eval -p 'test_*.py'
```

Output is a private new directory containing JSON and HTML. Raw commands, results
and paths are not copied into reports. Review evidence supplied by the caller is
included verbatim; do not put secrets in labels. Input hashes identify the exact
baseline. Existing outputs cannot be overwritten. No captured data belongs in Git.

## Capture contract

`--calls` is an array with unique `id`, `tool_name`, `input`, `request_id`,
`session_id`, `agent_instance_id`, `claude_code_agent_id`, `operator_turn_id`,
`created_at`, `result_request_id`, and `result_at`. Timestamps must include timezone.
Request IDs distinguish emitting a tool call from receiving its result. Identical
repeated rows collapse; conflicting rows fail rather than silently overwrite.

`--results` is an array with `id` (tool-use database ID), `result_request_id`, and
`content` (decoded string or typed content blocks). Supply all available results,
not only outputs a candidate compressor changes. Use `--sources ids.json` to fix
the source tool-ID cohort while supplying additional follow-up results for costing. Only text blocks contribute to
text bytes; images are excluded. Missing results remain unknown. Captured serialized
`content_bytes` is deliberately not used as decoded text size.

The scanner recognizes Read and simple shell readers of absolute paths, including
literal local variable assignments. It declines heredocs, command substitution,
redirection and malformed shell. It cannot resolve cwd, mutable file versions,
relative paths, dynamic variables or arbitrary programs. Recognized commands in a
mixed invocation are labeled as such. A matched argument may be a search pattern;
all discovered associations need review. Null agent identity is explicitly flagged.
Follow-ups must be emitted after result receipt, within the chosen window, in the
same session and agent lane. Tool errors remain evidence, not filtered successes.

Counts deduplicate tool IDs across overlapping edges. Unique emitting requests are
not sequential rounds. Absence of follow-ups does not establish result sufficiency.

## Reviewed edges

Optional `--labels` is an array such as:

```json
[{
  "source_tool_id": 808536,
  "followup_tool_id": 808542,
  "purposes": ["omitted_detail_recovery", "mixed"],
  "evidence": "Result requests 473363 and 473366: outcome-only log read followed by panic extraction; also contains diff/calculation work."
}]
```

Allowed purposes: preparation, omitted_detail_recovery, pagination, polling, rerun,
debugging, mixed, unrelated. Labels must reference discovered edges, cannot duplicate
an edge, and need evidence. They are human judgments, not automatically inferred
causal savings. Snapshot identity remains unknown without independent run evidence.

## Actual paired executions

`--trials` accepts an array of `{task_id, original, compact}`. Both arms require
identical nonempty `task_hash`, `ground_truth_hash`, `artifact_hash`,
`model_config_hash`, and matching `required_facts` identities. Each arm supplies:

- `execution_kind: "actual"`, `execution_ref`, independently graded `correct`
  boolean and `verdict_ref`;
- timezone-aware `started_at` and `finished_at`;
- `calls`: ordered `{id, content}` records, including retrievals, retries and rework;
- `model_turns`: unique `{id, input_tokens, output_tokens}` from measured usage;
- `required_facts`: `{id, first_call_id}`; null means not obtained.

`input_tokens` must be total per-turn input including cached input; normalize provider
usage before import. This is a caller-supplied evidence contract, not a model runner
or proof that supplied verdicts are honest. Missing token usage yields null, never a
byte-derived estimate. Full results are required to measure total returned text;
missing results are counted. Fact first-call annotations require independent review.
The scorecard reports correctness beside cost deltas; fewer calls with a wrong answer
is not a successful optimization. Without trials, comparative results remain empty.

## Persistent compression investigation flags

`scripts/compression-review` provides a local review queue for captured requests,
responses and individual tool results. The CLI and inspect UI share this queue; flags do not change live compression. It stores references and review notes separately
from immutable capture data. A flag nominates an investigation; it never enables a
rewrite or establishes savings.

```sh
scripts/compression-review flag request 473363 \
  --reason 'Repeated context worth examining' --evidence 'Manual inspection' --actor dawid
scripts/compression-review flag response 473363 \
  --reason 'Verbose repeated status' --evidence 'Manual inspection' --actor dawid
scripts/compression-review flag tool_result 473363 --tool-id 808536 \
  --reason 'Outcome view omitted assertion diagnostics' \
  --evidence 'Follow-up result request 473366' --actor dawid
scripts/compression-review import-sequences /private/report.json --actor sequence-evaluator
scripts/compression-review list --status open
scripts/compression-review show FULL_FLAG_ID
scripts/compression-review review FULL_FLAG_ID investigating \
  --note 'Checking original and subsequent calls' --actor dawid
```

For `tool_result`, the request ID is the request **receiving the result** and
`--tool-id` is its database tool-use ID. `show` provides original-body API links and
full nomination/review history. IDs are locally syntax-validated, not verified against
the capture server; `capture_verified: false` makes that boundary explicit. Use the
original body to validate the target before evaluating it. Tool IDs distinguish
multiple results in a single request.

The default queue is `~/.local/state/mike-compression-review`; set
`MIKE_COMPRESSION_QUEUE` or pass `--state DIRECTORY` before the subcommand to choose
a shared local queue. State is outside Git, private (0700/0600), append-only and
locked against concurrent writers. Never include secrets or full traffic in notes.
No credential, capture-database mutation, daemon or deployment is required.

Re-importing the identical report by the same actor is idempotent. Different evidence
for the same target adds a nomination to its history. Review states are `open`,
`investigating`, `evaluated`, and `dismissed`; every status change requires a note and
actor, and returning to `open` is explicit. New nominations never silently reopen a
reviewed flag. `evaluated` means investigation recorded, not approved for compression.

Sequence imports retain the report SHA-256 and follow-up request/tool identities.
They nominate source tool results only: evidence about a tool-result sequence does
not automatically justify flagging an entire request or assistant response. Import
reports with `import-sequences` after the evaluator completes, then review the queue.
No automatic flags are inferred from payload size alone.

### Inspect UI

Open **Compression review** in the navigation to filter flags, read their evidence
and history, open original captures and record a review status with a note.
Request and response body panels have **Flag for compression** actions. Expanded
session tool results offer the same action when the capture supplies both the
database tool-use ID and result-receiving request ID. Older servers without that
provenance do not offer a misleading tool-result action.

The inspect API exposes GET/POST `/api/compression-review` and POST
`/api/compression-review/{id}/review`. It uses the same local queue path and file lock
as the CLI. UI nominations record actor `inspect-ui`; this is a source label, not an
authenticated user identity. These are local investigation annotations, with no
captured-content egress, migrations or compressor settings changes.
