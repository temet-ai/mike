"""Bounded JSON-lines MCP client against the exact extracted companion binary."""
import json
import math
import os
import selectors
import time
from journey_process import captured_run, owned_child, require, runtime_environment


def notification(message):
    require(isinstance(message, dict) and message.get("jsonrpc") == "2.0", "MCP_NON_PROTOCOL_STDOUT")
    require(isinstance(message.get("method"), str) and bool(message["method"])
            and not any(key in message for key in ("id", "result", "error"))
            and ("params" not in message or isinstance(message["params"], (dict, list))),
            "MCP_INVALID_NOTIFICATION")


class Wire:
    def __init__(self, child):
        self.child = child
        self.buffer = bytearray()
        self.sequence = 0
        self.frames = 0
        self.selector = selectors.DefaultSelector()
        self.selector.register(child.stdout, selectors.EVENT_READ)
    def send(self, value):
        self.child.stdin.write(json.dumps(value, separators=(",", ":")).encode() + b"\n")
        self.child.stdin.flush()
    def call(self, method, params, *, deadline=None):
        if deadline is not None:
            require(type(deadline) in (int, float) and math.isfinite(deadline), "MCP_DEADLINE_INVALID")
            require(time.monotonic() < deadline, "MCP_FRAME_TIMEOUT")
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        return self.receive(self.sequence, deadline=deadline)
    def line(self, deadline):
        while b"\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            require(remaining > 0 and self.selector.select(remaining), "MCP_FRAME_TIMEOUT")
            data = os.read(self.child.stdout.fileno(), 65536)
            require(data, "MCP_STDOUT_CLOSED")
            self.buffer.extend(data)
            require(len(self.buffer) <= 8 * 1024 * 1024, "MCP_FRAME_LIMIT")
        line, _, rest = self.buffer.partition(b"\n")
        self.buffer = bytearray(rest)
        return json.loads(line)
    def receive(self, request_id, *, deadline=None):
        if deadline is not None:
            require(type(deadline) in (int, float) and math.isfinite(deadline), "MCP_DEADLINE_INVALID")
        deadline = min(time.monotonic() + 5, deadline) if deadline is not None else time.monotonic() + 5
        while True:
            require(time.monotonic() < deadline, "MCP_FRAME_TIMEOUT")
            message = self.line(deadline)
            require(time.monotonic() < deadline, "MCP_FRAME_TIMEOUT")
            self.frames += 1
            require(isinstance(message, dict) and message.get("jsonrpc") == "2.0", "MCP_NON_PROTOCOL_STDOUT")
            if "id" in message:
                require(type(message["id"]) is int and message["id"] == request_id, "MCP_UNEXPECTED_RESPONSE")
                require("result" in message and "error" not in message, "MCP_RPC_FAILURE")
                require("method" not in message and isinstance(message["result"], dict), "MCP_INVALID_RESPONSE")
                return message["result"]
            notification(message)
    def finish(self):
        try:
            self.child.stdin.close()
            deadline = time.monotonic() + 3
            while True:
                remaining = deadline - time.monotonic()
                require(remaining > 0 and self.selector.select(remaining), "MCP_SHUTDOWN_TIMEOUT")
                data = os.read(self.child.stdout.fileno(), 65536)
                if not data:
                    break
                self.buffer.extend(data)
                require(len(self.buffer) <= 8 * 1024 * 1024, "MCP_FRAME_LIMIT")
            while self.buffer:
                require(b"\n" in self.buffer, "MCP_TRAILING_NON_PROTOCOL_STDOUT")
                line, _, rest = self.buffer.partition(b"\n")
                self.buffer = bytearray(rest)
                value = json.loads(line)
                notification(value)
            self.child.wait(timeout=1)
            require(self.child.returncode == 0, "MCP_EXIT_FAILURE")
        finally:
            self.selector.close()

def content_json(result):
    require(not result.get("isError", False), "MCP_TOOL_FAILURE")
    content = result.get("content", [])
    values = [json.loads(item["text"]) for item in content if item.get("type") == "text"]
    require(len(values) == 1 and isinstance(values[0], dict), "MCP_TOOL_CONTENT")
    return values[0]

def run_mcp(binary, inspect_port, request_id, session, version, home, cwd):
    environment = runtime_environment(home)
    environment["MIKE_INSPECT_URL"] = f"http://127.0.0.1:{inspect_port}"
    expected = json.loads(captured_run([str(binary), "list-tools", "--json", "--core"], cwd, environment))
    expected_names = sorted(tool["name"] for tool in expected)
    with owned_child([str(binary), "serve", "--core"], cwd, environment, protocol=True) as child:
        wire = Wire(child)
        try:
            initialized = wire.call("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "mike-archive-journey", "version": "1"}})
            require(initialized.get("serverInfo", {}).get("name") == "mike-mcp"
                    and initialized["serverInfo"].get("version") == version
                    and initialized.get("protocolVersion") == "2024-11-05", "MCP_IDENTITY")
            require(isinstance(initialized.get("capabilities", {}).get("tools"), dict), "MCP_TOOLS_CAPABILITY")
            wire.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            catalogue = wire.call("tools/list", {})
            names = sorted(tool["name"] for tool in catalogue.get("tools", []))
            require(names == expected_names and "get_request" in names, "MCP_PROFILE_MISMATCH")
            result = content_json(wire.call("tools/call", {"name": "get_request", "arguments": {"id": request_id}}))
            request = result.get("request", {})
            require(request.get("id") == request_id and request.get("session_id") == session
                    and request.get("status") == "Complete", "MCP_CAPTURE_MISMATCH")
            return {"protocol": "2024-11-05", "profile": "core", "tool_count": len(names),
                    "read_tool": "get_request", "matching_capture": True, "protocol_frames": wire.frames}
        finally:
            wire.finish()
