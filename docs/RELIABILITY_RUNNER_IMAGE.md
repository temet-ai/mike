---
title: Isolated normal-default reliability runner image contract
status: active
created: 2026-10-14
updated: 2026-11-06
summary: Explicit local image and Linux verifier prerequisites for the opt-in normal-default reliability adapter, including fail-closed admission, bounded transfers and owned cleanup.
---

# Isolated normal-default reliability runner

The `normal_defaults` reliability adapter is implemented in
`scripts/release/journey_reliability_runner.py`. Its contract tests are offline.
They do not establish that a candidate ran, that an image is compatible, that a
particular Docker installation enforces these controls, or that AC20 passed.
Actual candidate and timed-workload evidence (900 seconds or more) remains **NOT_RUN**
until an explicitly authorized verifier completes the real workload. AC20 is not an
alpha-gating case.

The public acceptance helper requires all four `--normal-runner-*` selectors,
explicit resource budgets, and `--reliability-journey`. See the main release
acceptance documentation for its complete CLI. A normal-default invocation is
standalone: it cannot also run host-install, browser, fault, or other journeys.
The host snapshots candidate bytes without executing their smoke commands.

## Already-provisioned verifier

The adapter supports a native Linux x86-64 host with an already-running local,
rootful Docker engine using cgroup v2, with an already-trusted native 64-bit
x86-64 runc/libseccomp runtime registered under an explicit existing alias such
as `mike-release-native`. The fourth selector, `--normal-runner-runtime`, names
that alias. Reserved `runc` and `default` selections are refused: stock Docker
reports its reserved runtime with a relative path. Compat-built and other OCI
runtime implementations are outside this contract. Docker's actual
`Runtimes[alias].path` must already name an
absolute, non-symlink, non-shared-writable executable owned by root. Every
ancestor directory must also be root-owned and not group/world-writable. Relative/default names such as `runc`, missing paths, runtime arguments,
custom runtime types and alternate runtime options are refused. The adapter does
not guess a PATH location or change daemon configuration. The exact selected
file is opened with a bounded no-follow descriptor, its ELF64 little-endian
x86-64 header is checked, and its complete hash and filesystem identity are
pinned and rechecked before container creation and final result export. This
proves the selected runtime's native ABI prerequisite without executing it.
The supported trusted local daemon must share the verifier's host filesystem
view. Before every Docker command, the adapter opens only the selected Unix
socket with a bounded timeout and uses kernel SO_PEERCRED to require a direct
root-owned native `dockerd` peer. It binds the socket inode, daemon PID/start
time, running executable inode/hash, mount namespace and filesystem root
identity. The daemon executable is root-owned under a root-owned non-shared-
writable ancestor chain, ELF64 x86-64, and capped at 256 MiB; runc is capped at
64 MiB. The daemon's mount namespace and root device/inode must match the
verifier's, so the runtime path names the same host file. Proxy sockets, remote
forwarders, systemd-owned listeners, different mount/root views, deleted or
changed executables, and unreadable peer metadata fail closed. No API request
is sent over this identity-check connection. The actual container host-process
checks still apply independently.

The caller supplies an absolute trusted
Docker executable and an absolute Unix socket path. Neither may traverse a
symlink. For example, a distribution with `/var/run` linked to `/run` needs the
actual `/run` path. The socket and executable filesystem identities are checked
before every command. No Docker context, remote endpoint, SSH transport,
registry credentials, inherited proxy settings or automatic image pull is used.

The verifier must already be permitted to use that socket and read the
root daemon's identity and namespace metadata and the container process's
`/proc` namespace links, command line, status and mount
information, plus its cgroup resource files. The container uses the verifier's
existing non-root effective UID/GID. If the verifier was already root, the
container instead uses `65532:65532`. This selection adds no host permission or
elevation. Rootless Docker, user-namespace mappings that hide the selected UID,
cgroup v1 and unreadable host process observations fail closed. The helper does
not change host security, install software, start Docker or repair permissions.

## Explicitly trusted local image

Supply an existing local repository digest such as
`example.invalid/reviewed-runner@sha256:<64 lowercase hexadecimal characters>`.
This is a syntax example, not a published image or a download instruction. The
selected exact `RepoDigests` entry, image ID and `linux/amd64` platform must match
Docker's image inspection. Creation uses the immutable image ID and
`--pull=never` and explicitly selects the supplied `--runtime` alias; daemon
inspection must report that exact alias.

The operator-reviewed image must provide:

- `/usr/bin/python3`, Python 3.11 through 3.13, with the standard library including
  `tomllib`; the exact patch version and contents are pinned by the image digest
- `/usr/lib/postgresql/19/bin/{initdb,postgres,psql,pg_dump,pg_restore}` from the
  same PostgreSQL 19 minor release
- Preinstalled `vector`, `pgcrypto` and `pg_trgm` extension files compatible with
  that PostgreSQL installation; available-extension metadata is checked against
  the newly owned cluster before application migrations
- A passwd entry for the selected container UID with its selected primary GID,
  which `initdb` needs; no user or group is created at runtime
- `/bin/bash` for the shipped maintenance launcher smoke, `/usr/bin/uname` or
  `/bin/uname`, and the dynamic loader/libraries needed by the
  selected Linux candidate and PostgreSQL tools
- No declared volumes, exposed ports or secrets. Image environment keys must be
  limited to `PATH`, `HOME`, `TMPDIR`, `LC_ALL`, `LANG` and
  `PYTHONDONTWRITEBYTECODE`; all are overwritten with fixed private values

The image's entrypoint, command, working directory, hostname and health check
are overridden. Missing tools, incompatible versions, unavailable extensions or
candidate loader failures are prerequisites/failures, never instructions to
install, build or download anything.

## Admission before payload release

The host creates one random-named container bearing an exact random ownership
label and a versioned contract label. It has:

- `--network none`, no published ports and private PID, IPC, UTS, mount and
  cgroup namespaces
- Non-root UID/GID, no supplementary groups beyond the selected primary group,
  all Linux capabilities dropped, no-new-privileges and the exact pinned
  default-deny seccomp profile described below
- Read-only root filesystem and a private executable `/work` tmpfs with
  `nosuid,nodev`, mode `0700`, selected UID/GID and a 3 GiB maximum
- 4 GiB cgroup memory limit, no additional swap, two CPU-equivalents, at most
  256 processes, and 64 MiB shared memory
- No bind mounts, volume mounts, socket mounts, devices, inherited volumes,
  shared host PID/network/IPC/UTS/mount/cgroup namespaces, privilege mode,
  auto-remove, restart policy or log export. The rootful user namespace is
  intentionally shared; zero capabilities and non-root identity still apply

Before starting it, the adapter checks actual Docker inspection against the
complete contract. Initially it starts only an image-Python bootstrap that waits
for a host-pinned payload digest. Before any candidate bytes are copied, the
host independently checks the live process's exact command and stable start
time, distinct namespace identities, loopback-only interfaces, zero effective,
permitted, inheritable, ambient and bounding capabilities, no-new-privileges,
seccomp, root/work mount properties and the actual cgroup memory/swap/CPU/PID
limits. Daemon support flags alone do not grant admission. Missing observations
prevent payload release. These observations are repeated before result export.

The inner process performs additional boundary checks before starting tools or
listeners. Those checks cannot replace the host's admission evidence. No
caller-provided boolean, environment marker or inner report grants isolation.

## Exact syscall and transport contract

The adapter never treats the presence of Docker's changing default seccomp
filter as proof that non-IP transports are blocked. It writes a private,
SHA-256-pinned JSON profile from `journey_reliability_seccomp.py` and supplies it
explicitly to Docker. Before startup and again after the workload, actual
`HostConfig.SecurityOpt` must contain that exact decoded JSON, alongside
no-new-privileges. A path, `builtin`, `unconfined`, missing profile, changed rule,
extra option or duplicate profile is refused. Live host observations also
require seccomp filter mode to be active.

The profile is default-deny, with an explicit ordinary native syscall allowlist.
Only the `SCMP_ARCH_X86_64` architecture is enabled; x86/compat and x32 paths are
not added. This relies on the supported native runc/libseccomp runtime beginning
with the same native x86-64 architecture, bound to the daemon-selected runtime
ELF identity; the helper does not inspect generated
BPF or perform denied-call probes. Those runtime security checks remain NOT_RUN.
The only allowed `socket` and `socketpair` domain arguments are
AF_UNIX, AF_INET and AF_INET6. VSOCK, NETLINK, PACKET, ALG, XDP, future families,
`socketcall` and all `io_uring` entrypoints are denied. Namespace-changing clone
flags are masked out; `clone3` returns ENOSYS so compatible libc can use the
inspectable `clone` interface. Namespace/admin calls, `pidfd_getfd`, ptrace,
process-memory access, keyring APIs and other syscall brokers are absent from
the allowlist. There is no fallback to a broader profile if a trusted image or
candidate needs an unsupported syscall.

The socket restrictions apply to all container processes and descendants. UNIX
sockets can reach only the container's private filesystem and abstract network
namespace; IPv4/IPv6 have only the owned loopback interface. They complement the
private namespaces, no user-supplied host mounts, zero capabilities and no-new-
privileges. Actual syscall compatibility and workload execution remain NOT_RUN
until an authorized verifier runs them. Offline JSON tests do not install a
kernel filter or test an escape against a live host.

Primary implementation references used to review this contract:

- [Docker CLI profile-file transport](https://github.com/docker/cli/blob/master/cli/command/container/opts.go):
  the CLI reads the selected file and submits its compact JSON to the daemon
- [libseccomp architecture filtering](https://github.com/seccomp/libseccomp/blob/main/src/gen_bpf.c):
  native-only x86-64 filters exclude the x32 syscall path
- [runc seccomp installation](https://github.com/opencontainers/runc/blob/main/libcontainer/seccomp/seccomp_linux.go):
  only explicitly configured architectures are added
- [Docker seccomp documentation](https://docs.docker.com/engine/security/seccomp/):
  default-profile argument filtering is not sufficient to constrain compat
  `socketcall`, which this runner therefore does not permit

## Bounded candidate and tooling transfer

Candidate archives, their checksums, the manifest and the transitive local
Python tooling closure are copied as a tar stream through Docker's archive copy
protocol. There are no host mounts. Payload names are fixed under
`/work/payload/{candidate,tooling}`; tar entries are only private regular files
and directories owned by the selected UID/GID.

The combined transferred files are capped at 512 MiB. Aggregate manifest-declared
uncompressed archive content is independently capped at 512 MiB. Individual
source modules are capped at 4 MiB, the tooling closure at 100 modules, and launch
metadata at 1 MiB. Hashing and tar emission use pinned no-follow descriptors,
bounded reads, metadata/EOF checks and emitted-byte digest validation. The
bootstrap verifies every transferred byte and ownership against the pinned
launch digest before importing transferred code. Complete archive verification,
build identity, smoke commands and extraction occur only inside the admitted
container.

An owned PostgreSQL 19 cluster runs on container loopback with Unix-domain
listening disabled. It uses empty owned PGDATA and a freshly allocated database.
Only the verified candidate applies application migrations. The candidate and
owned synthetic upstream/Finite fixtures share this same isolated namespace.
No host database URL, credentials, filesystem roots or operator projects are
inherited. Normal feature defaults remain unchanged; only reviewed endpoint
relocations are permitted by the workload.

## Evidence and cleanup

The bootstrap stays alive while the result is copied out, because stopping a
container destroys its tmpfs. Result export admits one regular `result.json`
tar member capped at 1 MiB. The host validates candidate binding, rejects release
or full-case qualification, and projects only allowlisted counts, booleans,
states, hashes and version identifiers. Raw logs, request content, database files,
paths, arbitrary prose and the container filesystem are not exported. The full
inner report is identified by a digest. Synthetic service latency, unexercised
workers, browser responsiveness and real-provider performance are not inferred
from the measured observations.

Cleanup re-inspects the exact container ID, random name, ownership label,
contract label and image ID, then removes only that owned ID. A create timeout
may trigger read-only lookup of only that run's random name to recover its exact
ID. There is no label sweep, prune, broad stop or removal by name. If ownership
cannot be re-established or removal cannot be verified, evidence reports failure
and the retained ID/name so an authorized operator can investigate. A cleanup
failure cannot yield successful helper evidence.

Offline contract suites:

```
PYTHONPATH=scripts/release python3 -m unittest \
  test_journey_reliability_runner_contract test_journey_reliability_enforcement \
  test_journey_reliability_payload test_journey_reliability_runner \
  test_journey_reliability_seccomp test_journey_reliability_native_runtime \
  test_journey_reliability_peer
```

These suites replace socket and subprocess APIs with hard failures. Fake Docker
results exercise protocol ordering and refusal paths; they are never recorded
as live Docker, security-boundary or candidate-runtime evidence.
