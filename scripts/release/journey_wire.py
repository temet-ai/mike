"""Small bounded raw HTTP transport. Request bytes are never normalized by a client.

Only explicit loopback TCP ports are contacted. This is a verifier, not a general
HTTP client: upgrades stop at the handshake, and no redirects are followed.
"""
import re
import socket
import time

from journey_process import JourneyError, require

MAX_BODY = 128 * 1024
MAX_HEADERS = 16 * 1024
MAX_WIRE = MAX_BODY + 4 * MAX_HEADERS


class Wire:
    def __init__(self, connection, deadline):
        self.connection = connection
        self.deadline = deadline
        self.buffer = bytearray()
        self.received = 0

    def receive(self):
        remaining = self.deadline - time.monotonic()
        require(remaining > 0, "BOUNDARY_RESPONSE_TIMEOUT")
        self.connection.settimeout(remaining)
        try:
            data = self.connection.recv(16384)
        except TimeoutError as error:
            raise JourneyError("BOUNDARY_RESPONSE_TIMEOUT") from error
        self.received += len(data)
        require(self.received <= MAX_WIRE, "BOUNDARY_WIRE_LIMIT")
        self.buffer.extend(data)
        return bool(data)

    def until(self, delimiter, limit):
        while True:
            position = self.buffer.find(delimiter)
            if position >= 0:
                require(position <= limit, "BOUNDARY_SECTION_LIMIT")
                value = bytes(self.buffer[:position])
                del self.buffer[:position + len(delimiter)]
                return value
            require(len(self.buffer) <= limit, "BOUNDARY_SECTION_LIMIT")
            require(self.receive(), "BOUNDARY_TRUNCATED_RESPONSE")

    def exact(self, length):
        require(0 <= length <= MAX_BODY, "BOUNDARY_BODY_LIMIT")
        while len(self.buffer) < length:
            require(self.receive(), "BOUNDARY_TRUNCATED_RESPONSE")
        value = bytes(self.buffer[:length])
        del self.buffer[:length]
        return value

    def close_delimited(self):
        while self.receive():
            require(len(self.buffer) <= MAX_BODY, "BOUNDARY_BODY_LIMIT")
        require(len(self.buffer) <= MAX_BODY, "BOUNDARY_BODY_LIMIT")
        value = bytes(self.buffer)
        self.buffer.clear()
        return value


def response_head(raw):
    lines = raw.split(b"\r\n")
    match = re.fullmatch(rb"HTTP/1\.[01] ([1-5][0-9]{2})(?: [^\r\n]*)?", lines[0])
    require(match is not None, "BOUNDARY_RESPONSE_STATUS_LINE")
    headers = {}
    for line in lines[1:]:
        name, separator, value = line.partition(b":")
        require(separator and re.fullmatch(rb"[!#$%&'*+.^_`|~0-9A-Za-z-]+", name)
                and b"\r" not in value and b"\n" not in value, "BOUNDARY_RESPONSE_HEADER")
        headers.setdefault(name.decode("ascii").lower(), []).append(value.strip().decode("latin1"))
    return int(match[1]), headers


def chunked_body(wire):
    body = bytearray()
    while True:
        size_line = wire.until(b"\r\n", 1024)
        number = size_line.partition(b";")[0]
        require(re.fullmatch(rb"[0-9A-Fa-f]{1,8}", number), "BOUNDARY_CHUNK_SIZE")
        length = int(number, 16)
        if not length:
            # Do not discard uninspected trailer fields which could hide a leak.
            require(not wire.until(b"\r\n", MAX_HEADERS), "BOUNDARY_UNEXPECTED_RESPONSE_TRAILERS")
            return bytes(body)
        require(len(body) + length <= MAX_BODY, "BOUNDARY_BODY_LIMIT")
        body.extend(wire.exact(length))
        require(wire.exact(2) == b"\r\n", "BOUNDARY_CHUNK_TERMINATOR")


def response_body(wire, status, headers):
    lengths = headers.get("content-length", [])
    encodings = headers.get("transfer-encoding", [])
    require(len(lengths) <= 1 and len(encodings) <= 1 and not (lengths and encodings),
            "BOUNDARY_AMBIGUOUS_RESPONSE_FRAMING")
    if status == 101:
        require(not lengths and not encodings, "BOUNDARY_UPGRADE_BODY_FRAMING")
        return b""
    require(status >= 200, "BOUNDARY_UNEXPECTED_INTERIM_RESPONSE")
    if encodings:
        require(encodings == ["chunked"], "BOUNDARY_RESPONSE_ENCODING")
        return chunked_body(wire)
    if lengths:
        require(re.fullmatch(r"[0-9]{1,9}", lengths[0]), "BOUNDARY_RESPONSE_LENGTH")
        return wire.exact(int(lengths[0]))
    return wire.close_delimited()


def exchange(port, request, timeout=5):
    require(type(port) is int and 0 < port < 65536, "BOUNDARY_LOOPBACK_PORT")
    require(isinstance(request, bytes) and 0 < len(request) <= MAX_HEADERS, "BOUNDARY_REQUEST_LIMIT")
    deadline = time.monotonic() + timeout
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=timeout) as connection:
            remaining = deadline - time.monotonic()
            require(remaining > 0, "BOUNDARY_RESPONSE_TIMEOUT")
            connection.settimeout(remaining)
            connection.sendall(request)
            wire = Wire(connection, deadline)
            status, headers = response_head(wire.until(b"\r\n\r\n", MAX_HEADERS))
            body = response_body(wire, status, headers)
            require(status == 101 or not wire.buffer, "BOUNDARY_TRAILING_RESPONSE_BYTES")
            return status, headers, body
    except TimeoutError as error:
        raise JourneyError("BOUNDARY_RESPONSE_TIMEOUT") from error
