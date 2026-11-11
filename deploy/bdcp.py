#!/usr/bin/env python3
"""
BDCP Python SDK — Blind Data Collaboration Protocol
====================================================
Single-file wrapper around the daemon IPC sockets.

Usage:
    from bdcp import Alice, Bob

    alice = Alice(socket_path="/run/bdcp/alice.sock")
    result = alice.submit([10, 20, 30])
    # → [11, 22, 33]

    bob = Bob(socket_path="/run/bdcp/bob.sock")
    bob.submit([1, 2, 3])   # Bob only submits, doesn't read result

All communication happens over local Unix Domain Sockets.
Zero external dependencies — uses only Python stdlib.
"""

import json
import os
import socket
import struct
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Union


# ── Core IPC client (raw socket communication) ──────────────────────


class BdcpError(Exception):
    """Raised when the daemon returns an error or the IPC fails."""


@dataclass
class BdcpResponse:
    """Parsed response from a daemon IPC call."""

    raw: str
    lines: List[str]
    status: str  # "ok" | "error"
    results: Optional[List[int]] = None


def _ipc_call(socket_path: str, command: str, timeout: int = 120) -> BdcpResponse:
    """
    Send a command to a BDCP daemon over a Unix socket and read the response.

    Uses `socat` under the hood (must be installed). The socket is a
    stream-based Unix Domain Socket; we send a single line and read
    all lines the daemon writes back before closing.
    """
    if not os.path.exists(socket_path):
        raise BdcpError(f"Socket not found: {socket_path} — is the daemon running?")

    # socat connects, sends stdin, reads response, handles half-close
    cmd = [
        "socat",
        "-t", str(timeout),
        "-",
        f"UNIX-CONNECT:{socket_path}",
    ]

    try:
        proc = subprocess.run(
            cmd,
            input=command.encode(),
            capture_output=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        raise BdcpError(f"IPC timed out after {timeout}s — daemon may be busy")

    stdout = proc.stdout.decode().strip()
    stderr = proc.stderr.decode().strip()

    if proc.returncode != 0 and not stdout:
        raise BdcpError(f"socat error (exit={proc.returncode}): {stderr or '(no output)'}")

    lines = stdout.split("\n") if stdout else []
    response = BdcpResponse(raw=stdout, lines=lines, status="error")

    for line in lines:
        if "ERR" in line:
            response.status = "error"
            break
        if line.startswith("OK results:"):
            response.status = "ok"
            # Parse: "OK results: [11, 22, 33]"
            raw_results = line.split("OK results:")[1].strip()
            # Remove brackets and split
            if raw_results.startswith("[") and raw_results.endswith("]"):
                raw_results = raw_results[1:-1]
            if raw_results.strip():
                response.results = [int(x.strip()) for x in raw_results.split(",")]
            else:
                response.results = []
            break
        if line == "OK submitted":
            response.status = "ok"
            response.results = []

    return response


# ── Public API ──────────────────────────────────────────────────────


class Alice:
    """
    Alice — the job creator and result receiver.

    Alice generates the FHE keys, encrypts her values, submits them
    to the relay, and decrypts the final result. Only Alice sees the
    plaintext result.
    """

    def __init__(self, socket_path: str = "/run/bdcp/alice.sock"):
        self.socket_path = socket_path
        self._check_socket()

    def _check_socket(self) -> None:
        if not os.path.exists(self.socket_path):
            raise BdcpError(
                f"BDCP Alice socket not found at {self.socket_path}.\n"
                f"Start the daemon first:\n"
                f"  docker compose up -d alice\n"
                f"  # or: bdcp-node --config alice.toml"
            )

    def submit(self, values: List[Union[int, float]]) -> List[int]:
        """
        Encrypt and submit values to the relay, then return decrypted results.

        Args:
            values: List of integer values to compute on (e.g. [10, 20, 30]).

        Returns:
            Decrypted result vector (e.g. [11, 22, 33]).

        Raises:
            BdcpError: If the IPC call fails or the daemon returns an error.
        """
        # Convert to comma-separated integers
        val_str = ",".join(str(int(v)) for v in values)
        command = f"submit {val_str}\n"

        response = _ipc_call(self.socket_path, command)

        if response.status == "error":
            error_msg = response.raw or "unknown error"
            raise BdcpError(f"Alice daemon returned error: {error_msg}")

        if response.results is None:
            raise BdcpError(
                f"Alice daemon did not return results.\n"
                f"Raw output: {response.raw}"
            )

        return response.results


class Bob:
    """
    Bob — the data contributor.

    Bob receives Alice's public key (via the relay), encrypts his values,
    and submits them. Bob never sees the decrypted result (only Alice can
    decrypt).
    """

    def __init__(self, socket_path: str = "/run/bdcp/bob.sock"):
        self.socket_path = socket_path
        self._check_socket()

    def _check_socket(self) -> None:
        if not os.path.exists(self.socket_path):
            raise BdcpError(
                f"BDCP Bob socket not found at {self.socket_path}.\n"
                f"Start the daemon first:\n"
                f"  docker compose up -d bob\n"
                f"  # or: bdcp-node --config bob.toml"
            )

    def submit(self, values: List[Union[int, float]]) -> None:
        """
        Encrypt and submit values to the relay.

        This blocks until the daemon confirms submission. The relay
        will combine these values with Alice's values once both have
        submitted.

        Args:
            values: List of integer values to encrypt and submit.

        Raises:
            BdcpError: If the IPC call fails.
        """
        val_str = ",".join(str(int(v)) for v in values)
        command = f"submit {val_str}\n"

        response = _ipc_call(self.socket_path, command)

        if response.status == "error":
            error_msg = response.raw or "unknown error"
            raise BdcpError(f"Bob daemon returned error: {error_msg}")


# ── High-level orchestration ────────────────────────────────────────


def run_computation(
    alice_values: List[int],
    bob_values: List[int],
    alice_socket: str = "/run/bdcp/alice.sock",
    bob_socket: str = "/run/bdcp/bob.sock",
    timeout: int = 120,
) -> List[int]:
    """
    Run a complete two-party computation.

    1. Submits Alice's values (blocks until keys are generated and data is sent)
    2. Submits Bob's values (relay needs both before it can compute)
    3. Returns the decrypted result from Alice

    Args:
        alice_values: Alice's input values.
        bob_values: Bob's input values.
        alice_socket: Path to Alice's IPC socket.
        bob_socket: Path to Bob's IPC socket.
        timeout: Maximum time in seconds to wait for the computation.

    Returns:
        Decrypted result vector.

    Example:
        >>> result = run_computation([10, 20, 30], [1, 2, 3])
        >>> print(result)
        [11, 22, 33]
    """
    alice = Alice(alice_socket)
    bob = Bob(bob_socket)

    # TODO: in a future version, these could run in parallel threads
    # but for now, sequential submission is simpler and avoids race
    # conditions in the relay's acceptance logic.

    # Start both submissions with a small overlap (relay needs both)
    result_holder = {"result": None, "error": None}

    def _alice_worker():
        try:
            result_holder["result"] = alice.submit(alice_values)
        except Exception as e:
            result_holder["error"] = e

    def _bob_worker():
        try:
            bob.submit(bob_values)
        except Exception as e:
            if result_holder["error"] is None:
                result_holder["error"] = e

    alice_thread = threading.Thread(target=_alice_worker, daemon=True)
    bob_thread = threading.Thread(target=_bob_worker, daemon=True)

    alice_thread.start()
    time.sleep(0.3)  # Let Alice start keygen, then start Bob
    bob_thread.start()

    alice_thread.join(timeout=timeout)
    bob_thread.join(timeout=timeout)

    if result_holder["error"]:
        raise BdcpError(str(result_holder["error"])) from result_holder["error"]

    if result_holder["result"] is None:
        raise BdcpError("Computation timed out or produced no result")

    return result_holder["result"]


# ── CLI entry point for quick testing ────────────────────────────────


def main():
    """Quick CLI for testing the SDK."""
    import argparse

    parser = argparse.ArgumentParser(description="BDCP Python SDK — quick test")
    parser.add_argument("--alice-socket", default="/tmp/bdcp_alice.sock")
    parser.add_argument("--bob-socket", default="/tmp/bdcp_bob.sock")
    parser.add_argument("--alice-values", default="10,20,30")
    parser.add_argument("--bob-values", default="1,2,3")
    args = parser.parse_args()

    alice_vals = [int(x) for x in args.alice_values.split(",")]
    bob_vals = [int(x) for x in args.bob_values.split(",")]

    print(f"🤝 BDCP Computation")
    print(f"   Alice socket: {args.alice_socket}")
    print(f"   Bob socket:   {args.bob_socket}")
    print(f"   Alice values: {alice_vals}")
    print(f"   Bob values:   {bob_vals}")
    print()

    try:
        result = run_computation(
            alice_vals,
            bob_vals,
            alice_socket=args.alice_socket,
            bob_socket=args.bob_socket,
        )
        print(f"✅ Secure computation complete!")
        print(f"   Result: {result}")
    except BdcpError as e:
        print(f"❌ Error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
