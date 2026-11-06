---
title: Release content and source exposure review
status: draft
created: 2026-10-13
updated: 2026-11-06
summary: Redacted local PUBLIC01/PUBLIC02 inventory and hash-pinned source-export dry runs, the one-shot public tree export and what it did and did not catch, with explicit human approval and coverage limits.
---

# Release content and source exposure review

This is the local implementation of the content and source-exposure review.
It produces review evidence. It does **not** approve a release, expose source, rotate a
credential, publish an archive, change repository access, or rewrite history.
[`RELEASING.md`](RELEASING.md) remains authoritative for release scope.
The public repository `temet-ai/mike` already exists (2026-11-06, from the one-shot
export described below); neither gate has a verdict recorded in this repository.

## Two independent gates

- **PUBLIC01, before any distribution:** review the exact candidate archive hashes,
  binary strings, embedded SPA, source maps, bundled resources, configuration examples,
  diagnostics, notices, download-repository contents and destination access. The alpha
  is public, so this reviews what the public destination shows. A binary-only
  destination would have to be a separate repository, not access to the unchecked
  development repository.
- **PUBLIC02, before source exposure:** review the proposed source tree and the history
  being exposed, private prose/screenshots, unresolved credential candidates, rights,
  automatic source archives, destination and development-SHA mapping. Keeping development
  history private is independent of completing PUBLIC01 for a binary-only release.

Both gate statuses in machine output are always `NOT_RUN`; all human approvals also stay
`NOT_RUN`. An exit status or a zero-match scan cannot change them. Tests of these helpers
are synthetic contract tests, not independent verification of a real distribution.

## Run locally

Requires Python 3.11+ and Git; no Cargo, Node, network or external scanning service.
Run from the repository root. Store reports in private ignored `dist-out/` staging.
`--output` creates a new mode-0600 file and refuses to overwrite an existing path or symlink.
Without `--output`, the redacted report goes to stdout. Do not pipe a command to hide its
exit status.

```sh
mkdir -p dist-out/content-review
python3 scripts/release/security_review.py scan --repo . \
  --output dist-out/content-review/tracked-tree.json

python3 scripts/release/security_review.py scan --repo . --history \
  --output dist-out/content-review/tree-and-local-history.json

python3 scripts/release/security_review.py scan --artifact-only \
  --archive dist-out/CANDIDATE/mike-vVERSION-TARGET.tar.gz \
  --archive dist-out/CANDIDATE/mike-mcp-vVERSION-TARGET.tar.gz \
  --output dist-out/content-review/artifacts.json

python3 -m unittest discover -s scripts/release -p 'security_test*.py' -v
```

Replace uppercase placeholders with the candidate paths. Name every archive explicitly;
the tool never searches other directories for captures or candidate artifacts. Review
diagnostics and download-repository source as separate explicit candidate archives or
tracked-tree scans of those authorized repositories. An artifact-only scan does not
silently scan a source checkout or claim its history was checked.

Exit codes:

- `0`: requested mechanical work finished without detected candidates/missing review claims;
  **not a security, rights, distribution, or source-exposure pass**
- `1`: review candidates or required review claims remain
- `2`: an input error, unsafe input, incomplete coverage or limit was encountered

The export-template command returns zero when it wrote an **unapproved** proposal.
The template is private and contains exact requested paths for review. Scan reports do
not contain raw paths, matched values, text snippets, package names, resource URLs,
Git commit messages, author identities, or subprocess error output. Do not publish even
these redacted reports without checking the destination and intended audience.

## What is inspected

The current-tree scan uses the Git index to select tracked working-tree files. It records
HEAD, each index object, file hashes, modes, a content-snapshot hash and the dirty flag.
It does not pretend dirty working-tree bytes are the named commit. Untracked/ignored
files are excluded. Symbolic links and symlink parents are not followed; submodules are
not traversed. Reads use no-follow descriptors. Concurrent changes observed during a
read are a coverage gap; prepare the final evidence from a frozen candidate.

`--history` enumerates objects reachable from **local** refs with `--all`, including
commit/tag content and binary tree filenames. Removed files are included when their
objects remain reachable. Branch/ref names are inspected without reporting their text.
No fetch is attempted, including lazy-fetch of partial-clone objects. Missing objects,
shallow history, unavailable objects and limits are recorded as incomplete. Remote-only
refs, reflogs and unreachable objects are excluded. A shallow checkout cannot certify
full development history; use an independently authorized complete repository when that
history is the proposed publication surface.

TAR (including gzip/bzip2/xz) and ZIP archives are inspected without writing extracted
files or executing a binary. The raw archive hash, uncompressed member hashes, filenames,
metadata, byte patterns and ASCII/UTF-16 string candidates are recorded. Duplicate names,
unsafe paths, symlinks/special files, encrypted ZIP entries, malformed archives, trailing
nonzero TAR payloads and decoding limits are gaps. Embedded asset references are hashed;
font/image/source-map and license/notice candidates have separate categories. No archive
is accepted as an executable or imported as a plugin.

Detectors flag likely keys, credential assignments, cookies, database URLs, home/workspace
absolute paths, raw-conversation structures, private-material filenames, screenshots,
source maps and fonts. Examples and synthetic fixtures can match; every match needs a
reviewer disposition. A match is never a claim that a credential is usable. Do not test
a credential against a live service; a confirmed credential needs owner-controlled
rotation and separately recorded verification before exposure.

### Hard coverage limits

Defaults are 256 MiB per file/object, 2 GiB total scanned bytes, 50,000 inventory entries,
20,000 historical objects, 1,000 unique embedded resource references per input, and 1 MiB
per decoded TAR metadata read. TAR decoding also has a 2 GiB stream limit. Reaching a limit
is incomplete evidence, not a skipped pass. Manifest parsing is limited to 16 MiB.

The tool does not interpret screenshot pixels, recognize arbitrary private prose, verify
actual linked dependencies, decode compressed/encrypted/obfuscated executable resources,
recursively unpack nested archives, retrieve LFS payloads, or prove embedded-resource
completeness. LFS pointers and nested-container members are explicit gaps. UTF-16 and ASCII
string matching is heuristic, not decompilation. An independent reviewer must inspect
actual rendered screenshots/conversations and the build's embedded SPA/resource inventory.

For private triage, `path_ref` is `sha256:` plus the SHA-256 of the exact UTF-8 relative path
(surrogate-escaped Git filename bytes are preserved). A history filename reference is for
that tree entry's basename and includes its tree object ID. Package/resource references use
the same hash construction on their original strings. File and Git object hashes identify
the inspected content; matched credential values are neither printed nor individually hashed.
Resolve these references only in the authorized private workspace.

## Dependency, font, asset and notice review

A tracked-tree scan inventories Cargo.lock packages, frontend direct dependency declarations,
and package keys in pnpm lockfile v9. Unsupported lock syntax is a gap, not an empty dependency
list. Fontsource packages are labeled as font candidates. Tracked font/image files and
LICENSE/LICENCE/NOTICE/COPYING/OFL filename candidates are recorded separately. Archive
reports include notice references from their actual members.

Every dependency's rights, license, shipped status and required notice mapping remain
unresolved/unverified. Lockfiles do not contain sufficient license evidence, and the root
MIT license does not automatically license dependencies, fonts, icons or artwork. For the
exact built artifact, the release reviewer must retain a private ledger containing:

1. Package/asset identity, pinned version, source and shipped hash or corresponding artifact
2. The actual upstream license text and applicable notice/attribution obligations
3. The reviewed redistribution basis, restrictions and owner/legal reviewer evidence
4. The exact bundled notice path/hash, or why no notice is required under the reviewed license
5. Any unresolved provenance, custom/commercial terms or generated artwork rights

Do not infer a font's license from its package name, an image's permission from its presence
in the repository, or distribution permission from a dependency allowlist. `NOTICE.json`
with `third_party_notice_review: not_run` is a blocker to a rights-complete PUBLIC01 review,
not a substitute for notices. Network license fetching and dependency vulnerability audits
are not performed by this offline tool.

## The public tree: one-shot export and private words

The public repository becomes the one repository (the same route as gg), so
the public-export step, which lives in the private repository, runs once, for the first push, and has no sync or push mode.
It has run: `temet-ai/mike` was created from its output on 2026-11-06 as a collapsed
history, so its commits are not the development repository's SHAs.
`OUT_DIR` must not exist; point it at a scratch folder outside the checkout. It exports
`HEAD`, removes plans, internal docs, agent configuration and itself (its `EXCLUDE` list),
strips `session_id:` front matter, then refuses on a gitleaks finding, a private word, a
maintainer path or session link, a link to excluded material, or a missing `LICENSE`.
The licence is MIT with no CLA. Pushing is the operator's step and is not scripted.

The private word list lives outside the repository: `~/.config/mike-dev/private-words.txt`
(or `$MIKE_PRIVATE_WORDS`), one case-insensitive extended regex per line. The same list
drives `scripts/private-words-check`, which the pre-commit hook runs on staged lines and
file names and the commit-msg hook runs on the message. Do not write a listed word into
any tracked file, including this one; a script that has to name one writes it as a regex
with a bracketed letter. The checks are reinstalled with `scripts/db-code-graders/install-hooks.sh`.

Fixtures, stories and tests use invented names. The replacement map, so a later fixture
stays consistent:

| Real kind | Used instead |
|---|---|
| employer, client and organisation names | `ankh-morpork` |
| a project repository | `night-watch` (prefix `am-` for the group folder) |
| a service repository | `ledger-clerk` |
| the owner's work email | `dawid@example.com` |

## Candidate-specific reviewed notice input

`release.sh --reviewed-notices PATH` can package an explicitly prepared offline
review manifest and its actual license/notice texts. Omitting the option preserves
`third_party_notice_review: not_run`. Supplying a template, incomplete review,
changed text or mismatched source fails preparation; the tool never fills in a
reviewer, review decision, upstream license, or redistribution permission.

First prepare the ordinary local candidate so that its generated `frontend/build`
exists. From that same clean source, create a new private review directory and an
unreviewed template (the output file must not already exist):

```sh
mkdir -p dist-out/content-review/notices
python3 scripts/release/notice_inputs.py template --repo . \
  --source "$(git rev-parse HEAD)" --version 0.1.0 \
  --target x86_64-unknown-linux-gnu \
  --output dist-out/content-review/notices/review.json
```

Use the exact candidate version, full source SHA and native target. A later notice
preparation rebuilds the SPA: changed source, target, lockfiles or generated asset
bytes invalidate the review. Do not edit the generated binding, inventory, its
SHA-256, or subject IDs to disguise a stale review; regenerate and review it.

The inventory conservatively includes every Cargo.lock package, every pnpm v9
package record and every generated SPA file, including fonts, images and maps.
Cargo source identifiers, lockfile hashes, package-record hashes and full asset
path/hash/size lists are retained. `hash_scope: package_lock_record` hashes the
lockfile record, not downloaded or linked package payloads. `spa_file_bytes`
hashes the actual generated file. The tool does not resolve the actual linked
Rust graph, infer an asset's author, or determine whether a development dependency
ships. Unsupported package-key syntax is an error rather than a skipped package.
The human must identify the upstream provenance and artifact applicability for
every subject, including apparently first-party files and excluded dependencies.

The authorized reviewer completes the template as follows:

1. Record a real `review.reviewer`, UTC `reviewed_at` (`YYYY-MM-DDTHH:MM:SSZ`), and
   `evidence_reference`, then set the review status to `REVIEWED`. These remain
   unauthenticated human claims; the tool performs no reviewer authentication.
2. Complete every subject review in its original order. Supply `upstream`,
   `license_expression`, `basis` and `evidence_reference`, and set `status` to
   `REVIEWED`. Choose `notice_required`, `no_notice_required`, or `not_shipped`.
   The first two require an explicit sorted `binaries` list (`mike`, `mike-mcp`,
   or both); `not_shipped` requires an empty list. Any no-notice or not-shipped
   conclusion still needs the review evidence and explanation.
3. Place the actual reviewed UTF-8 license/notice texts next to the manifest as
   regular, non-hardlinked files. Add a sorted `texts` list with each distinct
   `id`, adjacent filename `path`, exact byte `size`, and lowercase SHA-256.
   Map every `notice_required` subject to one or more of these IDs using sorted
   `notice_ids`. Other dispositions use no IDs. Texts can be shared by multiple
   subjects; duplicate, missing and unused text entries are rejected.
4. Keep `publication_authorized: false`. Reviewer identities, evidence references,
   upstream descriptions and explanations are included in the candidate. Use
   references suitable for the intended audience, not secrets or private paths,
   and review the resulting bytes under PUBLIC01 before any distribution.

For example, one text descriptor has this shape; the hash and byte size must be
computed from the actual reviewed file, never copied from a placeholder:

```json
{"id": "upstream-license", "path": "upstream-license.txt", "sha256": "ACTUAL_LOWERCASE_SHA256", "size": 1234}
```

Then prepare and independently verify the new candidate:

```sh
./scripts/release.sh --reviewed-notices dist-out/content-review/notices/review.json
./scripts/release.sh --verify dist-out/CANDIDATE
python3 -m unittest discover -s scripts/release -p 'test_notice_inputs.py' -v
```

The reviewed manifest and all exact texts are embedded in each archive's existing
`NOTICE.json`, with canonical manifest, inventory and text hashes. No extra archive
paths are introduced, and old unreviewed candidates retain their existing format.
Candidate verification checks the full recorded review schema, source/target/version,
lockfile/SPA binding, every text hash and byte size, and actual packaged text
membership. During preparation, it also compares the entire supplied inventory
against the source lockfiles and built SPA. Verification without the source cannot
independently reconstruct the lockfile records; their source pins and operator
review remain part of the artifact's provenance.

Successful mechanical validation records `third_party_notice_review:
recorded_unverified`, `human_review_claims: UNVERIFIED`, `rights_approval: NOT_RUN`
and `publication_authorized: false`. It does not mark PUBLIC01/PUBLIC02, acceptance,
license sufficiency, redistribution rights or publication approval as passed.
The separate reviewer must still assess the exact final archive hashes, generated
code/assets, legal obligations and distribution destination. No license acceptance,
network fetching, publication or external communication occurs.

Bounds are 16 MiB per input manifest or lockfile, 20,000 inventory subjects or SPA
traversal entries, 64 MiB per generated SPA file, 10,000 notice texts, 256 KiB per
text, 8 MiB of combined raw text and 32 MiB per packaged `NOTICE.json`. Text inputs
must be adjacent filenames; absolute, nested, parent-traversing, symlinked,
hardlinked, special, non-UTF-8, empty, NUL-containing or concurrently changed inputs
are refused. These are hard errors, not incomplete reviews treated as passes.

## Concrete source-publication proposal: a new reviewed root

The proposed path is a new-root export in a separately approved source destination,
retaining private development history and a hash-pinned provenance mapping. This is a
proposal, not a selected or created destination. No destructive sanitization is needed for
the development repository. No current source file is automatically approved for export.

Build the exact allowlist from the intended product source, lockfiles, build metadata,
required SQLx metadata/migrations, frontend source and assets, tests with synthetic data,
necessary build scripts, user-facing documentation and notices. Review each path and blob.
Do not broadly allowlist the checkout, `.claude`, raw captures, session notebooks, logs,
database dumps, environment files, personal configuration or screenshots. Some SQL/test
files may be necessary; their contents still require privacy review. A runnable source
export must separately pass its own offline build/tests after review.

The following example deliberately proposes only LICENSE. It is a tooling example, not a
complete or buildable publication tree. Add each intended file as an exact `--path`; globs,
directories, symlinks, submodules, duplicate paths and `.git` paths are rejected.

```sh
python3 scripts/release/security_review.py export-template --repo . \
  --source FULL_DEVELOPMENT_COMMIT_SHA --path LICENSE \
  --output dist-out/content-review/source-allowlist.json

python3 scripts/release/security_review.py export-check --repo . \
  --allowlist dist-out/content-review/source-allowlist.json \
  --output dist-out/content-review/source-dry-run.json

python3 scripts/release/security_review.py export-check --repo . \
  --allowlist dist-out/content-review/source-allowlist.json \
  --archive dist-out/REVIEWED_SOURCE_ARCHIVE.tar.gz \
  --output dist-out/content-review/source-archive-dry-run.json
```

The private proposal pins a full development SHA, each Git blob ID, SHA-256 and executable
mode. It starts with every privacy/rights/source-owner/legal/distribution/access/source-
archive review `NOT_RUN`. An authorized reviewer can record an `APPROVED` claim and private
`evidence_reference` for each required review; the tool never invents evidence or authenticates
that claim. The output always labels recorded claims `UNVERIFIED` and release approval
`NOT_RUN`. Keep `publication_authorized: false` and `history_retained: false`: this tool has
no publish mode.

`export-check` uses committed blobs, not potentially changed working-tree files. It verifies
all pins and computes a canonical allowlist SHA-256. If a separately prepared archive is
provided, file membership, bytes and executable modes must match exactly. Set
`source_archive_prefix` to an exact safe wrapper ending in `/` when a host-style archive
has a root directory, or leave it empty. Extra/missing files, changed hashes, unsafe members,
decode gaps and mode changes fail validation. Archive metadata and private prose still need
manual review. This is a dry run: no export tree, archive, Git object, branch, tag or remote
is created or changed.

Before authorized source publication, the owner must review the **actual** new destination
commit and host-generated source downloads. Record development SHA → allowlist hash →
exported-tree/new-root SHA → source-download hashes, chosen destination, visibility/access
and reviewer evidence. Automatically generated source archives are not disabled by uploading
only binary release assets; the destination's entire tagged source tree must be reviewed.
The dry run does not assert what an unvisited hosting service will generate.

## Human handoff and reporting/maintenance policy

The release owner must finish and sign the following evidence before the applicable stage:

- PUBLIC01: complete artifact inventory, screenshot/conversation review, rights/notices,
  no undeclared maintainer dependencies, exact download bytes and verified destination access
- PUBLIC02: reviewed source tree/history proposal, candidate dispositions, owner-controlled
  rotation evidence for any confirmed credentials, exact source archives, destination and SHA mapping
- Independent verifier: reproduce review evidence against the frozen candidate identities
- Owner/legal/distribution approvals: record separately; none is implied by an automated result

Report suspected vulnerabilities through the route in [SECURITY.md](SECURITY.md): GitHub
private vulnerability reporting on `temet-ai/mike`. On 2026-11-06 GitHub reported that
feature as disabled on the repository, so the route is not verified; until it is
enabled and tried, send no details. Do not post credentials, database URLs, captures,
screenshots of conversations or exploit-bearing private data in public issues. A minimal
initial report can contain the version/target, a synthetic reproduction and
expected/observed behavior, with sensitive details retained for the private follow-up.

For the alpha, the owner must enable and verify that private security reporting route and
name the responsible maintainer. No address, response SLA or currently supported
release is invented here. The proposed maintenance policy is: list qualified supported
versions explicitly; review Rust/JS dependency advisories and font/asset notice changes
before each candidate; requalify fixes and replacement artifacts locally; document migration
and supported-version changes; announce an unsupported predecessor before withdrawing its
support. Cadence, response targets and ownership remain decisions to approve. Until then,
security-reporting readiness and supported-version maintenance remain `NOT_RUN` blockers
for public alpha. The current unqualified internal version is not a supported public release.

## The public tree

The public-export step, which lives in the private repository, builds the first public tree from `HEAD` in a new folder and
pushes nothing. It leaves out the internal material, makes the maintainer's home folders,
real session links and private repository name into made-up ones, turns Markdown links to
left-out files into plain text, and then refuses at the first hit of: gitleaks (default rules
plus the literal fakes in `.gitleaks.toml`), the private word list
(`scripts/private-words-check --tree`), a maintainer path or link, a link to left-out
material, a CLA file, or a missing `LICENSE` / `AGENTS.md`. The tree carries
`CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `THIRD-PARTY-NOTICES.md`
(`scripts/notices`, from `Cargo.lock` through cargo-about) and
`docs/WHAT_MIKE_STORES_AND_SENDS.md`; release archives carry `THIRD-PARTY-NOTICES.md` beside
`LICENSE` and `NOTICE.json`. The licence is MIT with no CLA.

The acceptance policy is `docs/RELEASE_ACCEPTANCE_MATRIX.md`, pinned by repository path in
`acceptance_ledger.py` and `publication_gates.py`; the export tolerates no dead link.

### What the export did not catch (alpha.1)

None of those checks builds the exported tree. The first public tag, `v0.1.0-alpha.1`,
did not compile: `EXCLUDE` removed `.claude` files that the `mike` crate embeds with
`include_str!`/`include_bytes!`. The fix turns the export into an allowlist (`KEEP`) and
refuses when an embedded file is left out. Build an exported tree before pushing it.
