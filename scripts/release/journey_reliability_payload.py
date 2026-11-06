"""Bounded tar transfer, source closure and hash-bound launch inputs; no execution."""
import ast
from contextlib import ExitStack
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tarfile

from journey_process import JourneyError, require
from journey_reliability_runner_contract import GID, UID

MAX_PAYLOAD_BYTES = 512 * 1024 ** 2
MAX_TOOL_BYTES = 4 * 1024 ** 2
MAX_RESULT_BYTES = 1024 ** 2
ROOT_MODULES = ("journey_reliability_container", "journey_reliability_normal")
NAMESPACES = ("net", "pid", "mnt", "ipc", "uts", "cgroup")

# This first process is image Python only. No transferred module is imported until
# every byte in the private payload is matched to the host-pinned launch digest.
BOOTSTRAP = '''import hashlib,json,os,pathlib,stat,sys,time
root=pathlib.Path('/work/payload')
end=time.monotonic()+180
while True:
 try:
  raw=(root/'launch.json').read_bytes()
  if len(raw)<=1048576 and hashlib.sha256(raw).hexdigest()==sys.argv[1]: break
 except FileNotFoundError: pass
 if time.monotonic()>end: raise SystemExit(71)
 time.sleep(0.1)
launch=json.loads(raw)
for item in launch['files']:
 path=root/item['path']
 if not path.is_relative_to(root) or any(p.is_symlink() for p in (path,*path.parents)): raise SystemExit(72)
 info=path.stat()
 if not stat.S_ISREG(info.st_mode) or info.st_uid!=65532 or info.st_gid!=65532 or info.st_mode&0o077 or info.st_size!=item['size']: raise SystemExit(73)
 digest=hashlib.sha256()
 with path.open('rb') as stream:
  for block in iter(lambda:stream.read(1048576),b''): digest.update(block)
 if digest.hexdigest()!=item['sha256']: raise SystemExit(74)
sys.path.insert(0,str(root/'tooling'))
from journey_reliability_container import main
main(launch)
while True: time.sleep(60)
'''.replace('info.st_uid!=65532', 'info.st_uid!=' + str(UID)).replace('info.st_gid!=65532', 'info.st_gid!=' + str(GID))
WAIT_RESULT = '''import pathlib,sys,time
end=time.monotonic()+float(sys.argv[1])
while not pathlib.Path('/work/result.json').is_file():
 if time.monotonic()>end: raise SystemExit(75)
 time.sleep(1)
print('ready')
'''


def local_namespaces():
    result = {key: os.readlink('/proc/self/ns/' + key) for key in NAMESPACES}
    require(all(re.fullmatch(key + r":\[[0-9]+\]", value) for key, value in result.items()),
            "ISOLATED_HOST_NAMESPACE_IDENTITY")
    return result


def bounded_file(path, limit):
    require(not any(part.is_symlink() for part in (path, *path.parents)), "ISOLATED_PAYLOAD_SYMLINK")
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    stream = os.fdopen(descriptor, "rb")
    value = os.fstat(stream.fileno())
    if not stat.S_ISREG(value.st_mode) or not 0 <= value.st_size <= limit:
        stream.close()
        raise JourneyError("ISOLATED_PAYLOAD_FILE_LIMIT")
    return stream, value.st_size


def source_closure(root):
    pending, seen = list(ROOT_MODULES), {}
    while pending:
        name = pending.pop()
        if name in seen:
            continue
        path = root / (name + ".py")
        with bounded_file(path, MAX_TOOL_BYTES)[0] as stream:
            source = stream.read(MAX_TOOL_BYTES + 1)
        require(len(source) <= MAX_TOOL_BYTES, "ISOLATED_TOOLING_LIMIT")
        seen[name] = path
        for node in ast.walk(ast.parse(source)):
            names = ([alias.name.split('.')[0] for alias in node.names] if isinstance(node, ast.Import)
                     else [node.module.split('.')[0]] if isinstance(node, ast.ImportFrom) and node.module else [])
            for dependency in names:
                if (root / (dependency + ".py")).exists():
                    pending.append(dependency)
        require(len(seen) <= 100, "ISOLATED_TOOLING_COUNT")
    return [seen[name] for name in sorted(seen)]


def metadata(stream):
    value = os.fstat(stream.fileno())
    return (value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns, value.st_ctime_ns)


def stream_digest(stream, size):
    before = metadata(stream)
    require(before[2] == size, "ISOLATED_PAYLOAD_CHANGED")
    stream.seek(0)
    digest, remaining = hashlib.sha256(), size
    while remaining:
        block = stream.read(min(1024 ** 2, remaining))
        require(bool(block), "ISOLATED_PAYLOAD_CHANGED")
        digest.update(block)
        remaining -= len(block)
    require(not stream.read(1) and metadata(stream) == before, "ISOLATED_PAYLOAD_CHANGED")
    stream.seek(0)
    return digest.hexdigest()


def file_digest(path, limit):
    stream, size = bounded_file(path, limit)
    with stream:
        return stream_digest(stream, size)


def candidate_files(snapshot, manifest, manifest_digest):
    require(manifest.get("target") == "x86_64-unknown-linux-gnu", "ISOLATED_CANDIDATE_PLATFORM")
    require(re.fullmatch(r"[0-9a-f]{64}", manifest_digest or ""), "ISOLATED_MANIFEST_DIGEST")
    with bounded_file(snapshot / "manifest.json", 8 * 1024 ** 2)[0] as source:
        raw_manifest = source.read(8 * 1024 ** 2 + 1)
    require(hashlib.sha256(raw_manifest).hexdigest() == manifest_digest
            and json.loads(raw_manifest) == manifest, "ISOLATED_MANIFEST_CHANGED")
    version = manifest.get("version")
    require(isinstance(version, str) and re.fullmatch(r"[0-9A-Za-z.+-]+", version), "ISOLATED_CANDIDATE_VERSION")
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, list) and len(artifacts) == 2
            and {item.get("binary") for item in artifacts} == {"mike", "mike-mcp"}, "ISOLATED_CANDIDATE_ARCHIVES")
    result = [(snapshot / "manifest.json", manifest_digest)]
    expanded_size = 0
    for item in artifacts:
        name = f"{item['binary']}-v{version}-x86_64-unknown-linux-gnu.tar.gz"
        require(item.get("archive") == name and re.fullmatch(r"[0-9a-f]{64}", item.get("sha256", "")),
                "ISOLATED_ARCHIVE_IDENTITY")
        inventory = item.get("contents")
        require(isinstance(inventory, list) and len(inventory) <= 16, "ISOLATED_ARCHIVE_INVENTORY")
        for member in inventory:
            require(isinstance(member, dict) and type(member.get("size")) is int
                    and 0 <= member["size"] <= MAX_PAYLOAD_BYTES, "ISOLATED_ARCHIVE_EXPANDED_LIMIT")
            expanded_size += member["size"]
        require(expanded_size <= MAX_PAYLOAD_BYTES, "ISOLATED_ARCHIVE_EXPANDED_LIMIT")
        checksum = (item['sha256'] + "  " + name + "\n").encode()
        result.extend(((snapshot / name, item['sha256']),
                       (snapshot / (name + '.sha256'), hashlib.sha256(checksum).hexdigest())))
    require({path.name for path in snapshot.iterdir()} == {path.name for path, _ in result},
            "ISOLATED_CANDIDATE_EXTRA_FILES")
    return result


class CheckedReader:
    def __init__(self, stream, size):
        self.stream, self.remaining, self.digest = stream, size, hashlib.sha256()
        self.before = metadata(stream)
        stream.seek(0)

    def read(self, count):
        require(0 <= count <= self.remaining, "ISOLATED_TAR_READ_LIMIT")
        value = self.stream.read(count)
        self.remaining -= len(value)
        self.digest.update(value)
        return value

    def finish(self, expected):
        require(self.remaining == 0 and not self.stream.read(1) and metadata(self.stream) == self.before
                and self.digest.hexdigest() == expected, "ISOLATED_PAYLOAD_CHANGED")


def add_member(archive, name, source, size, *, directory=False):
    info = tarfile.TarInfo(name)
    info.uid, info.gid = UID, GID
    info.mode = 0o700 if directory else 0o600
    info.type = tarfile.DIRTYPE if directory else tarfile.REGTYPE
    info.size = 0 if directory else size
    archive.addfile(info, source)


def prepare_payload(snapshot, manifest, manifest_digest, destination, request, *, host_namespaces=None):
    files = [("candidate/" + path.name, path, expected) for path, expected in candidate_files(snapshot, manifest, manifest_digest)]
    files.extend(("tooling/" + path.name, path, None) for path in source_closure(Path(__file__).resolve().parent))
    records, streams, total = [], [], 0
    with ExitStack() as stack:
        for name, path, expected in files:
            limit = (MAX_TOOL_BYTES if name.startswith("tooling/") else
                     8 * 1024 ** 2 if path.name == "manifest.json" else
                     1024 if path.name.endswith(".sha256") else MAX_PAYLOAD_BYTES)
            source, size = bounded_file(path, limit)
            stack.enter_context(source)
            total += size
            require(total <= MAX_PAYLOAD_BYTES, "ISOLATED_PAYLOAD_TOTAL_LIMIT")
            digest = stream_digest(source, size)
            require(expected is None or digest == expected, "ISOLATED_ARCHIVE_DIGEST")
            records.append({"path": name, "size": size, "sha256": digest})
            streams.append(source)
        launch = {"schema_version": 1, "manifest_sha256": manifest_digest,
                  "host_namespaces": host_namespaces if host_namespaces is not None else local_namespaces(),
                  "request": request, "files": records}
        encoded = json.dumps(launch, sort_keys=True, separators=(",", ":")).encode()
        require(len(encoded) <= MAX_RESULT_BYTES, "ISOLATED_LAUNCH_LIMIT")
        with destination.open("xb") as output, tarfile.open(fileobj=output, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for directory in ("payload", "payload/candidate", "payload/tooling"):
                add_member(archive, directory, None, 0, directory=True)
            for source, record in zip(streams, records):
                reader = CheckedReader(source, record['size'])
                add_member(archive, "payload/" + record['path'], reader, record["size"])
                reader.finish(record['sha256'])
            add_member(archive, "payload/launch.json", io.BytesIO(encoded), len(encoded))
    destination.chmod(0o600)
    require(destination.stat().st_size <= MAX_PAYLOAD_BYTES + 2 * MAX_RESULT_BYTES, "ISOLATED_TAR_LIMIT")
    return {"launch_sha256": hashlib.sha256(encoded).hexdigest(),
            "transfer_sha256": file_digest(destination, MAX_PAYLOAD_BYTES + 2 * MAX_RESULT_BYTES),
            "file_count": len(records), "bytes": total}


def result_from_tar(raw):
    require(len(raw) <= MAX_RESULT_BYTES + 65536, "ISOLATED_RESULT_TRANSFER_LIMIT")
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        member = archive.next()
        require(member is not None and member.name == "result.json" and member.isreg()
                and not member.linkname and 0 < member.size <= MAX_RESULT_BYTES,
                "ISOLATED_RESULT_MEMBER")
        stream = archive.extractfile(member)
        data = stream.read(MAX_RESULT_BYTES + 1)
        require(len(data) == member.size and archive.next() is None, "ISOLATED_RESULT_EXTRA_MEMBERS")
    try:
        value = json.loads(data)
    except (ValueError, UnicodeError) as error:
        raise JourneyError("ISOLATED_RESULT_JSON") from error
    require(isinstance(value, dict), "ISOLATED_RESULT_SHAPE")
    return value, hashlib.sha256(data).hexdigest()
