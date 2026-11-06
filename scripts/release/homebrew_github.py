"""Fixed-host tap Git adapter; no repository, branch, tag or credential creation."""

import base64
import binascii
from urllib.parse import quote

from homebrew_proposal import git_blob

from publication_github import GitHub
from publication_inputs import COMMIT, require


class TapGitHub(GitHub):
    def branch(self, name):
        result = self.request("GET", self.base + "/git/ref/heads/" + quote(name, safe=""), allow_missing=True)
        if result is None:
            return None
        require(result.get("ref") == "refs/heads/" + name
                and result.get("object", {}).get("type") == "commit", "tap_ref_identity_mismatch")
        return result["object"].get("sha")

    def commit(self, sha):
        require(isinstance(sha, str) and COMMIT.fullmatch(sha), "invalid_tap_commit_sha")
        return self.request("GET", self.base + "/git/commits/" + sha)

    def tree(self, sha):
        require(isinstance(sha, str) and COMMIT.fullmatch(sha), "invalid_tap_tree_sha")
        return self.request("GET", self.base + "/git/trees/" + sha + "?recursive=1")

    def blob(self, sha):
        require(isinstance(sha, str) and COMMIT.fullmatch(sha), "invalid_tap_blob_sha")
        result = self.request("GET", self.base + "/git/blobs/" + sha)
        require(isinstance(result, dict) and result.get("sha") == sha
                and result.get("encoding") == "base64" and type(result.get("size")) is int
                and 0 <= result["size"] <= 65536 and isinstance(result.get("content"), str)
                and len(result["content"]) <= 131072, "invalid_tap_blob_response")
        try:
            data = base64.b64decode(result["content"].replace("\n", ""), validate=True)
        except (ValueError, binascii.Error):
            require(False, "invalid_tap_blob_encoding")
        require(len(data) == result["size"] and git_blob(data) == sha, "tap_blob_identity_mismatch")
        return data

    def create_tree(self, base_tree, entries):
        require(isinstance(base_tree, str) and COMMIT.fullmatch(base_tree), "invalid_tap_base_tree")
        require(isinstance(entries, list) and len(entries) == 2
                and {e.get("path") for e in entries} == {"Formula/mike.rb", "Formula/mike-mcp.rb"}
                and all(set(e) == {"path", "mode", "type", "content"} and e["mode"] == "100644"
                        and e["type"] == "blob" and isinstance(e["content"], str)
                        and len(e["content"].encode()) <= 65536 for e in entries), "exact_formula_pair_required")
        return self.request("POST", self.base + "/git/trees", {"base_tree": base_tree, "tree": entries}).get("sha")

    def create_commit(self, tree, parent, message):
        require(all(isinstance(s, str) and COMMIT.fullmatch(s) for s in (tree, parent)), "invalid_tap_commit_inputs")
        return self.request("POST", self.base + "/git/commits",
                            {"tree": tree, "parents": [parent], "message": message}).get("sha")

    def advance_branch(self, branch, sha):
        require(isinstance(sha, str) and COMMIT.fullmatch(sha), "invalid_tap_target_commit")
        self.request("PATCH", self.base + "/git/refs/heads/" + quote(branch, safe=""), {"sha": sha, "force": False})
