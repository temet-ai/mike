---
title: Same-byte release publication
status: draft
created: 2026-10-13
updated: 2026-11-06
summary: Offline archive and Homebrew proposals with explicit same-byte publication adapters; synthetic tests do not approve a release or qualify a distribution channel. The tag workflow publishes alphas by its own route.
---

# Same-byte release publication

This is the archive and Homebrew publication tooling. It authorizes no destination
and certifies no artifact. The current archive acceptance and PUBLIC review runners
deliberately produce unqualified reports; **those reports are rejected before any
network access**. `scripts/release.sh --publish` remains disabled. The separate entry
point is `python3 scripts/release/publication.py` for archives and
`python3 scripts/release/homebrew.py` for the separately qualified tap pair.

**This is not how the alpha is published.** A pushed `v0.1.0-alpha.N` tag runs
`.github/workflows/release-alpha.yml`, which builds on the public repository's runners,
creates the pre-release and updates the tap by itself
([`RELEASING.md`](RELEASING.md#the-tag-triggered-alpha-workflow)). As of 2026-11-06 it
has published `0.1.0-alpha.3`. The tooling below is the stricter route for a qualified release, and
the two do not share state. Because the public repository was created from a collapsed
export, its commits are not the development repository's SHAs: a destination's
`tag_commit` must be the public commit that built the candidate.

## Archive adapter boundaries

- `inspect` and `plan` are entirely offline. They copy and hash selected files;
  they never execute candidates, rebuild, run Cargo, or contact GitHub.
- `preflight` performs authenticated read-only GitHub checks only after all local
  qualification gates pass and the exact repository is explicitly named again.
- `publish` additionally requires the exact offline plan hash. It creates or
  resumes a matching draft, uploads missing assets, downloads and hashes the
  actual remote bytes, then publishes a prerelease without promoting `latest`.
- No repository, tag, tap, credential, permission, or visibility is created or
  changed by an explicit tool operation. The repository and exact tag must
  already exist. The tool has no ref-write, delete, rebuild, or clobber operation.
- Publication does not establish installation from the channel. AC26 and the
  post-distribution feedback/update observations remain NOT RUN.
- This archive adapter supports only GitHub.com archive prereleases. It refuses
  Homebrew channels rather than silently treating archive evidence as Homebrew
  qualification. The separate tap adapter below needs its own exact-formula native
  evidence. Stable releases, custom hosts and other channels remain unsupported.

A source-exposing destination must map its existing tag to the candidate source
commit and pass PUBLIC02. A binary-only destination must be a separate repository
with a different reviewed distribution commit. PUBLIC01 must explicitly cover its
repository contents and GitHub's automatically generated source archives. A
private repository alone does not make a source-containing release binary-only.

The original development repository's visibility is never changed. Its existing
privacy status and the proposed destination/history mapping are part of the
human PUBLIC review. The tool verifies the selected destination's current
visibility, not every repository or collaborator in an organization's access
model. The alpha is public, so PUBLIC01 reviews what the public destination shows.

## Preparation and review inputs

Keep the prepared candidate immutable, including its `qualified: false` field.
Qualification is a separate, reviewed evidence bundle, never an edit to the
manifest. For every selected native target, the tool uploads exactly two archive
and checksum pairs plus the unchanged manifest bytes, named
`manifest-<target>.json` to avoid collisions. One plan can contain Linux x86_64,
Linux ARM64 and Apple Silicon macOS candidates from the same version/source commit, but each
requires its own complete target evidence. Select every advertised target before
publication; adding a target later changes the plan and conflicts with the
existing release. An unqualified target stays unsupported.

First inventory trusted local prepared inputs, without running them:

```sh
python3 scripts/release/publication.py inspect \
  --candidate /absolute/path/to/prepared-candidate \
  --output /absolute/path/to/new-private-inventory.json
```

Repeat `--candidate` for a second independently prepared native target. Output
files must be new; existing files are never overwritten. The inventory supplies
`candidate_identities` for the review bundle, including version, source commit,
target, exact manifest hash, and all uploaded file hashes/sizes. Inventory is not
qualification. A malformed, modified, symlinked, missing or extra input is refused.

The operator/reviewer prepares a destination JSON with exactly these fields:

```json
{
  "schema_version": 1,
  "kind": "mike-publication-destination",
  "repository": "temet-ai/mike",
  "source_repository": "temet-ai/mike",
  "visibility": "public",
  "tag": "v0.1.0-alpha.N",
  "tag_commit": "FULL_EXISTING_CANDIDATE_SOURCE_COMMIT",
  "source_exposure": true,
  "channels": ["archive"],
  "prerelease": true
}
```

These placeholders are intentionally invalid. Choosing a repository, public
version, tag and distribution content remains an operator decision. The version
must be `0.1.0-alpha.N` and the destination public; the tool never changes
visibility. A separate already-authorized process must establish the destination
and tag before this tool can proceed.

## Qualification contract

The independently reviewed qualification JSON has:

- `schema_version: 1`, `kind: "mike-publication-qualification"`, `state: "APPROVED"`
  and `qualified: true`, only after actual acceptance is established
- `policy`: the repository path of the [acceptance matrix](RELEASE_ACCEPTANCE_MATRIX.md), `docs/RELEASE_ACCEPTANCE_MATRIX.md`
- `destination`: the complete destination object above, with actual reviewed values
- `candidate_identities`: the exact sorted array emitted by `inspect`
- `gates`: SG01, SG02, SG03, PUBLIC01, and PUBLIC02 when source is exposed
- `target_cases`: a map from each selected target triple to AC01–AC25 and AC27

Every required gate, target case and prerequisite must contain `status: "PASS"`,
a nonempty `reviewed_by`, an actual `observed` result, and a nonempty `evidence`
array. Each evidence reference names a relative JSON `file` and its exact
`sha256`. The file must remain within the review bundle, with no symlink or parent
traversal. Its own `status` must be `PASS`, and `qualified: false` is rejected.
Source logs can be retained in an independently reviewed JSON evidence envelope;
include real commands, exit statuses, actual observations, counts and input
identity as required by the matrix. Private raw logs should remain private.

Do not promote an incomplete report by changing a label or wrapping it in a
manufactured passing envelope. The validator checks the review contract and
bytes; it cannot establish that a human observation really happened or authenticate
a reviewer. It is an operator-controlled attestation boundary, not a signature,
independent verifier, or substitute for acceptance testing. No tool in this slice
writes an approved qualification report.

Additional explicit scope fields prevent accidental scope confusion:

- AC08: `channels: ["archive"]`
- AC15: `manual_restart: true`; this tool advertises no service autostart
- AC20: `duration_seconds` at least 900
- AC23: `predecessor_kind` is `"internal"` or `"published"`
- PUBLIC01, for binary-only delivery:
  `binary_only_destination_and_generated_source_archives_reviewed: true`

This contract is stricter than the alpha bar (six cases, see
[`RELEASE_ACCEPTANCE.md`](RELEASE_ACCEPTANCE.md)): a publication through this tool
needs real AC19 client compatibility, native AC08, the source gates, supported
database paths and every other case in `target_cases` as PASS. Hosted execution that has not run remains NOT RUN. No CI pass,
clean-account qualification, private-content approval or external-client result
is inferred from this implementation's local synthetic tests.

AC26 is deliberately not a pre-publication gate: public-channel AC26 and
corrective-update feedback follow publication. PUBLIC02 is required for source
exposure, not automatically for a reviewed binary-only alpha.

## Offline plan, read-only preflight, explicit publication

With genuinely approved evidence, create an offline plan:

```sh
python3 scripts/release/publication.py plan \
  --candidate /absolute/path/to/prepared-candidate \
  --qualification /absolute/path/to/review/qualification.json \
  --destination /absolute/path/to/review/destination.json \
  --output /absolute/path/to/new-private-plan.json
```

The plan hash binds destination, source/version, all selected targets and
bytes, and the exact qualification document. Keep the whole review bundle and
prepared archives. Neither the qualification document nor private evidence files
are uploaded. The selected manifests already contain intentional build provenance;
PUBLIC01 must review those bytes as well as the archives.

Read-only preflight repeats the same input flags and uses:

```text
preflight ... --confirm-repository OWNER/REPOSITORY
```

Actual publication is a separate, consequential invocation:

```text
publish ... --confirm-repository OWNER/REPOSITORY --approve-plan EXACT_OFFLINE_PLAN_SHA256
```

Here `...` means the same explicit candidate, qualification and destination flags,
not a shell command to copy verbatim. The operator must separately authorize this
specific release, files and audience before running `publish`. Existing authorized
`gh` credentials are used; the tool does not log in, inspect/print tokens, create
credentials, or grant access. Use the least-privilege credential for this repository
and keep it away from untrusted PR jobs. Do not add credentials to these JSON files.

Preflight checks repository identity/visibility, nonarchived/nondisabled state,
reported `permissions.push`, the exact existing tag commit, matching release
provenance and every existing remote asset's actual downloaded bytes. Missing,
denied or malformed checks stop the operation. A repository push role is not
proof that every fine-grained token permission or server-side write policy permits
upload/publication; a subsequent denied write halts immediately with no fallback
or permission expansion. Such failure can leave a draft for explicit same-plan
resume.

## Resume and conflict behavior

The remote release is the resumable record. Its exact deterministic provenance
body binds it to the plan. A partial matching draft reuses every matching asset
and uploads only missing names. Local inputs are privately snapshotted before
verification; replacing the caller's files during a run cannot replace upload
bytes. Archives and manifests are never rebuilt or relabeled during retry.

- Unknown create/upload/publish outcomes stop immediately. There is no automatic
  mutation retry. Rerun the explicitly approved same plan after inspecting status.
- A matching already-published release with complete identical bytes succeeds
  read-only. It never receives more files or a metadata rewrite.
- Changed or unexpected assets, duplicate names, mismatched release provenance,
  an incomplete already-published release, or a moved/missing tag cause refusal.
  The tool does not delete, overwrite, repair by force, or move a tag.
- Permission/visibility/tag identity is rechecked during upload. Every remote file
  is downloaded and hashed again before publication and after publication.
- Publishing errors report an unconfirmed remote outcome. Do not infer that
  nothing happened from a failed invocation; resume discovers the actual state.

The adapter follows GitHub’s [release API](https://docs.github.com/en/rest/releases/releases)
and [gh api interface](https://cli.github.com/manual/gh_api). An exclusive operator
release window is still required. GitHub does not provide
this adapter with an atomic transaction spanning tag identity, assets and draft
publication. Concurrent administrators could edit a release or delete a tag
between checks; publishing against a removed tag may cause GitHub's release API
to recreate a ref. Do not run concurrent tag/release editors. The adapter itself
never calls ref writes, and observed conflicts always stop rather than overwrite.
This is additive retry safety, not a claim of server-side release immutability.

## Homebrew: offline formula proposal and separate tap adapter

The Homebrew entry point consumes a prepared Apple Silicon candidate and an
explicit **public alpha archive destination**. Repeat `--candidate` with the Linux
candidate if the archive release plan includes both targets: the tap plan must
bind the entire existing archive release, even though the two generated formulae
install only `aarch64-apple-darwin` bytes. Linux-only, private-destination Homebrew,
unreviewed platforms and cross-build substitutions are refused.

Preparation never edits `homebrew-formulae/`, runs a binary, invokes Ruby/brew,
creates a tap, or calls a network service. The output is a new private directory
outside the source checkout and selected candidate directories, containing only
`proposal.json`, `Formula/mike.rb` and `Formula/mike-mcp.rb`. Each formula pins the
reviewed public archive URL and SHA-256, version, candidate manifest hash, source
commit, ARM64 requirement and explicitly reviewed minimum macOS version. Install
copies prebuilt bytes only; it does not compile, install a service/database, edit
shell profiles or register a client. Both formulae must be installed for the pair.
The Mike formula keeps the optional Bash/Python 3.11+ maintenance kit in `libexec`;
that kit manages separate archive-owned prefixes. Brew owns formula updates/removal.

A separate tap destination JSON must contain exactly these fields:

```json
{
  "schema_version": 1,
  "kind": "mike-homebrew-destination",
  "repository": "temet-ai/homebrew-mike",
  "branch": "REVIEWED_EXISTING_DEFAULT_BRANCH",
  "base_commit": "FULL_REVIEWED_EXISTING_TAP_COMMIT",
  "formula_blobs": {
    "Formula/mike.rb": "FULL_REVIEWED_PREDECESSOR_GIT_BLOB_SHA1",
    "Formula/mike-mcp.rb": null
  },
  "minimum_macos": "REVIEWED_NATIVE_MINIMUM"
}
```

These placeholders are intentionally invalid. A `null` predecessor means that
exact formula path must be absent at the reviewed base commit. Both predecessor
paths are mandatory. The tap must be a separate, already-existing public
`OWNER/homebrew-NAME` repository with an existing default branch. Supported
minimum-macOS spellings are `big_sur`, `monterey`, `ventura`, `sonoma`, `sequoia`
and `tahoe`; accepting a spelling does not qualify that baseline. The native
review must establish the actual candidate's runtime baseline before choosing it.

Create and independently review a deterministic proposal without qualification:

```sh
python3 scripts/release/homebrew.py propose \
  --candidate /absolute/path/to/prepared-apple-silicon-candidate \
  --destination /absolute/path/to/review/public-archive-destination.json \
  --tap /absolute/path/to/review/tap-destination.json \
  --proposal /absolute/path/to/new-formula-proposal
```

`validate` repeats those flags against the existing proposal. It regenerates the
expected content and refuses changed formula bytes, added files, symlinks,
nonregular/oversized inputs or changed source/candidate/destination identity.
Both commands emit `UNQUALIFIED_FORMULA_PROPOSAL`, `qualified: false` and
`AC26: NOT_RUN`. They need no qualification document or credentials.

Before `plan`, supply the independently approved archive qualification plus an
independent Homebrew qualification document. The latter has `schema_version: 1`,
`kind: "mike-homebrew-qualification"`, `state: "APPROVED"`, `qualified: true`,
the same pinned `policy` URL, the exact `proposal_sha256`, and the exact raw
archive-qualification file hash as `archive_qualification_sha256`. Its `gates`
map requires PUBLIC01, HOMEBREW_AUDIT, HOMEBREW_NATIVE, AC08, AC23 and AC25. Every
gate uses the same reviewed observation and hashed JSON evidence contract above.
In addition:

- AC08 requires `channels: ["homebrew"]` and `both_binaries: true`.
- AC23 requires `channel: "homebrew"`, `predecessor_kind: "published"`,
  `data_retained: true`, `interrupted_retry: true` and
  `incompatible_downgrade_refused: true`.
- HOMEBREW_NATIVE requires `target: "aarch64-apple-darwin"`, the exact reviewed
  `minimum_macos`, `clean_account: true`, `runtime_dependencies_verified: true`,
  `signature_policy_verified: true` and `quarantine_policy_verified: true`.

These are acceptance attestations, not automatic native checks. A generic archive
PASS, a maintainer's existing installation, the generated proposal, or the tests
below cannot establish them. Rehearse the exact formula bytes on a clean native
account against the actual intended archive transport and frozen published
predecessor; retain real audit, runtime, quarantine/signature, install, upgrade,
interruption/retry and downgrade observations. AC26 remains a separate
post-publication observation from the final authorized public tap. No approved
Homebrew report is generated by this tool.

Use `plan` with the same candidate/destination/tap/proposal flags plus
`--qualification /absolute/path/to/archive-qualification.json` and
`--homebrew-qualification /absolute/path/to/homebrew-qualification.json`.
The emitted plan hash binds both review documents, all selected archive identities,
formula bytes and the exact base/predecessor contract. `plan` is entirely offline.

The Apple Silicon Homebrew row of the [acceptance matrix](RELEASE_ACCEPTANCE_MATRIX.md)
needs a Mac and is run by the maintainer once a release exists; see
[where the matrix rows run](RELEASING.md#where-the-matrix-rows-run).

`preflight` repeats all plan inputs and additionally requires
`--confirm-archives OWNER/BINARY-DOWNLOADS --confirm-tap OWNER/homebrew-TAP`.
It performs read-only requests: the archive release must already be published
with every qualified file present and its downloaded bytes verified; the tap must
have the reviewed identity, visibility, default branch, base tree and predecessor
blobs. Both repositories must report the required push role under the current
shared archive preflight contract. This role is not proof that token scopes or
branch-protection rules will permit subsequent writes. Use only existing,
least-privilege authorized credentials; never expand permissions as a fallback.

`publish` also requires `--approve-plan EXACT_OFFLINE_TAP_PLAN_SHA256`, after the
operator separately authorizes these exact formula bytes and tap audience. It
creates one tree changing only the two formula paths, downloads and checks actual
formula blob bytes against SHA-256 and Git object identity, creates one commit
with the reviewed base as its sole parent and a plan-hash marker, then advances
only the existing default branch with `force: false`. It rechecks the full archive
and tap state before advancing and verifies the observed commit/tree/blob bytes
afterward. It never creates/recreates a branch, tag, repository or release, changes
branch protection, deletes files, or writes archive assets.

Unknown tree/commit/branch-update outcomes stop immediately; there is no automatic
mutation retry. A failed attempt can leave unreferenced Git objects. An explicitly
approved same-plan retry may recreate identical objects while the reviewed branch
remains unchanged, or recognize the already-published exact commit/tree read-only.
A moved branch, wrong parent/marker, changed predecessor, changed archive/formula,
unexpected tree modification, deleted branch or denied permission stops the run.
Do not edit the base contract to disguise a conflict: a different base is a new
proposal and requires fresh review/approval. Keep an exclusive operator window;
GitHub does not provide a transaction spanning the archive and tap repositories.
An uncertain outcome must be inspected before explicit resume. Native channel
installation and AC26 stay NOT RUN in every emitted plan/report.

## Local verification and remaining evidence

```sh
python3 -m unittest discover -s scripts/release -p 'test_publication*.py' -v
python3 -m unittest discover -s scripts/release -p 'test_homebrew*.py' -v
```

Tests use synthetic candidate bytes and in-memory/fully mocked transports only.
They cover denied permissions, immutable conflicts, interrupted/ambiguous create,
upload and publish, partial resume, snapshot changes, alpha-version/source gates,
real-byte hash comparisons, API pagination and sanitized errors. They do not run
`gh`, Ruby or brew, access a network, prepare a real candidate, or approve
distribution. The Homebrew tests additionally cover deterministic formula
regeneration, two-target archive binding, native-evidence gates, exact predecessor
identity, whole-tree preservation, actual formula blob verification, non-force
branch updates, denied permissions and explicit resume after unknown outcomes.

Live credential behavior, actual GitHub publication/downloads, clean-account
channel install, native macOS policy, real Homebrew/tap integration and external
feedback remain unobserved. Mocked adapter coverage is not those observations. Keep AC25
NOT RUN until the complete preparation/rehearsal evidence is independently accepted;
close AC26 only after installation from the actual authorized published channel.
Neither gates the alpha.
