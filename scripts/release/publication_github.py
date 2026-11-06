"""Explicit GitHub.com transport using an already authorized gh installation.

No login, token creation, repository/tap creation, ref update, deletion, rebuild,
clobber, automatic retry, or general URL is supported. Tests replace the runner.
"""

import hashlib
import json
import os
import resource
from pathlib import Path
import subprocess
import tempfile
from urllib.parse import quote

from common import sha256
from publication_engine import RemoteFailure
from publication_inputs import COMMIT, REPOSITORY, canonical, require


class GitHub:
    def __init__(self, repository, runner=subprocess.run):
        require(isinstance(repository, str) and REPOSITORY.fullmatch(repository), "invalid_repository")
        self.repository_name = repository
        self.base = "repos/" + repository
        self.runner = runner

    def command(self, method, endpoint, *, input_file=None, binary=False):
        args = ["gh", "api", "--hostname", "github.com", "--method", method, endpoint,
                "-H", "X-GitHub-Api-Version: 2022-11-28",
                "-H", "Accept: application/octet-stream" if binary else "Accept: application/vnd.github+json"]
        if not binary:
            args.append("--include")
        if input_file is not None:
            args.extend(["--input", str(input_file)])
            args.extend(["-H", "Content-Type: application/octet-stream" if endpoint.startswith("https://uploads.github.com/")
                         else "Content-Type: application/json"])
        return args

    def execute(self, args, output, maximum=16 * 1024 * 1024):
        # Existing gh auth may come from its config or existing environment; no
        # setup, persistence or credential inspection occurs. Never print stderr.
        env = {key: value for key, value in os.environ.items()
               if key in ("PATH", "HOME", "GH_CONFIG_DIR", "XDG_CONFIG_HOME", "GH_TOKEN", "GITHUB_TOKEN",
                          "SSL_CERT_FILE", "SSL_CERT_DIR", "SYSTEMROOT")}
        env.update(GH_PROMPT_DISABLED="1", GH_NO_UPDATE_NOTIFIER="1", GH_PAGER="cat")
        def output_limit():
            _, hard = resource.getrlimit(resource.RLIMIT_FSIZE)
            bound = maximum if hard == resource.RLIM_INFINITY else min(maximum, hard)
            resource.setrlimit(resource.RLIMIT_FSIZE, (bound, bound))
        try:
            result = self.runner(args, stdout=output, stderr=subprocess.PIPE, env=env,
                                 timeout=600, check=False, preexec_fn=output_limit)
        except (OSError, subprocess.SubprocessError) as exc:
            raise RemoteFailure("GitHub request outcome unknown; inspect and explicitly resume the same plan") from exc
        return result.returncode

    def request(self, method, endpoint, payload=None, input_file=None, allow_missing=False):
        with tempfile.TemporaryDirectory(prefix="mike-github-request-") as temporary:
            root = Path(temporary)
            if payload is not None:
                input_file = root / "request.json"
                input_file.write_bytes(canonical(payload))
                input_file.chmod(0o600)
            with (root / "response").open("w+b") as output:
                code = self.execute(self.command(method, endpoint, input_file=input_file), output)
                require(output.tell() <= 16 * 1024 * 1024, "oversized_github_response")
                output.seek(0)
                raw = output.read().replace(b"\r\n", b"\n")
            header, separator, body = raw.partition(b"\n\n")
            try:
                status = int(header.splitlines()[0].split()[1])
            except (IndexError, ValueError):
                raise RemoteFailure("GitHub returned no valid HTTP status; no automatic retry")
            if allow_missing and status == 404:
                return None
            if code or status < 200 or status >= 300 or not separator:
                raise RemoteFailure(f"GitHub HTTP {status}; stopped without retry or permission fallback")
            try:
                return json.loads(body)
            except (ValueError, UnicodeError) as exc:
                raise RemoteFailure("GitHub response invalid; inspect and explicitly resume the same plan") from exc

    def repository(self):
        return self.request("GET", self.base)

    def tag_commit(self, tag):
        ref = self.request("GET", self.base + "/git/ref/tags/" + quote(tag, safe=""), allow_missing=True)
        if ref is None:
            return None
        require(ref.get("ref") == "refs/tags/" + tag, "remote_tag_name_mismatch")
        obj = ref.get("object", {})
        for _ in range(8):
            if obj.get("type") == "commit":
                return obj.get("sha")
            require(obj.get("type") == "tag", "unexpected_remote_tag_type")
            require(isinstance(obj.get("sha"), str) and COMMIT.fullmatch(obj["sha"]), "invalid_remote_tag_object")
            obj = self.request("GET", self.base + "/git/tags/" + obj["sha"]).get("object", {})
        raise RemoteFailure("nested tag depth refused; no tag will be changed")

    def release(self, tag):
        # The tag endpoint omits drafts for some token contexts. List releases,
        # including drafts visible to this already-authorized identity, instead.
        found = []
        for page in range(1, 101):
            batch = self.request("GET", self.base + f"/releases?per_page=100&page={page}")
            require(isinstance(batch, list), "invalid_release_list")
            found.extend(item for item in batch if item.get("tag_name") == tag)
            if len(batch) < 100:
                require(len(found) <= 1, "ambiguous_release_for_tag")
                return found[0] if found else None
        raise RemoteFailure("release listing incomplete; refuse publication")

    def assets(self, release_id):
        result = []
        for page in range(1, 101):
            batch = self.request("GET", self.base + f"/releases/{release_id}/assets?per_page=100&page={page}")
            require(isinstance(batch, list), "invalid_asset_list")
            result.extend(batch)
            if len(batch) < 100:
                return result
        raise RemoteFailure("asset listing incomplete; refuse publication")

    def asset_sha256(self, asset_id, expected_size):
        # Stream to private disk; never trust metadata's digest or download URL.
        with tempfile.TemporaryFile() as output:
            code = self.execute(self.command("GET", self.base + f"/releases/assets/{asset_id}", binary=True),
                                output, expected_size + 1)
            if code:
                raise RemoteFailure("asset download denied or failed; no automatic retry")
            require(output.tell() == expected_size, "remote_download_size_conflict")
            output.seek(0)
            digest = hashlib.sha256()
            for block in iter(lambda: output.read(1024 * 1024), b""):
                digest.update(block)
            return digest.hexdigest()

    def create_draft(self, spec):
        self.request("POST", self.base + "/releases", spec)

    def upload(self, release_id, name, path, expected_sha256):
        require(sha256(path) == expected_sha256, "snapshot_changed_before_upload")
        endpoint = f"https://uploads.github.com/{self.base}/releases/{release_id}/assets?name=" + quote(name, safe="")
        self.request("POST", endpoint, input_file=path)

    def publish(self, release_id):
        self.request("PATCH", self.base + f"/releases/{release_id}", {"draft": False, "make_latest": "false"})
