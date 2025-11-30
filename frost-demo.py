#!/usr/bin/env python3
"""Run the frost demo and print each step with command and output in Markdown."""

from __future__ import annotations

import locale
import os
import selectors
import shlex
import subprocess
import sys
import textwrap
import threading
import time
import uuid
from pathlib import Path
from typing import Optional, Tuple

BOX_PREFIX = "│ "


class PersistentShell:
    """
    Persistent POSIX shell that preserves state across commands and returns
    combined stdout+stderr with accurate exit codes.
    """

    _CTRL_FD = 9
    _DEBUG_FD = 5
    _RS = b"\x1e"  # ASCII Record Separator to minimize collision in user output

    def __init__(
        self,
        cwd: Optional[str] = None,
        env: Optional[dict] = None,
        *,
        shell_path: str | None = None,
        login: bool = True,
        encoding: Optional[str] = None,
        read_chunk: int = 65536,
        debug: bool = False
    ):
        """Initialize the persistent shell."""
        if os.name != "posix":
            raise OSError("PersistentShell requires a POSIX system.")

        self._encoding = encoding or locale.getpreferredencoding(False)
        self._read_chunk = int(read_chunk)
        self._lock = threading.RLock()
        self._residual = bytearray()

        ctrl_r, ctrl_w = os.pipe()
        self._ctrl_r = ctrl_r
        self._ctrl_w = ctrl_w

        self._shell_path = (
            shell_path
            or os.environ.get("FROST_DEMO_SHELL")
            or os.environ.get("SHELL")
            or "zsh"
        )

        debug_fd = self._DEBUG_FD
        bootstrap = f"""\
# PersistentShell bootstrap (executed via: $SHELL -lc '<this script>')
# stdin is already /dev/null from the parent; do not touch FD 0 here.

# ── Debug channel on FD {debug_fd} (default: silent) ───────────────────────────
if [[ -n "${{PSH_DEBUG_FILE:-}}" ]]; then
  exec {debug_fd}>>"${{PSH_DEBUG_FILE}}" || {{ echo "PSH: cannot open ${{PSH_DEBUG_FILE}}" >&2; exit 95; }}
elif [[ -n "${{PSH_DEBUG:-}}" ]]; then
  exec {debug_fd}>/dev/stderr
else
  exec {debug_fd}>/dev/null
fi

# ── Control FD: duplicate the inherited FD to 9 and close the original ─────────
exec 9<&{ctrl_r} || {{ echo "PSH: dup {ctrl_r} -> 9 failed" >&200; exit 97; }}
exec {ctrl_r}<&- || true

# Sanitize prompts/hooks; keep normal shell semantics (no `set -e`)
PS1=; PS2=; PROMPT_COMMAND=

# If running under zsh, adopt reasonable defaults so scripts behave like POSIX sh
if [[ -n "${{ZSH_VERSION:-}}" ]]; then
  setopt SH_WORD_SPLIT
  unsetopt NOMATCH
fi

# Optional xtrace routed via FD 200
if [[ -n "${{PSH_DEBUG:-}}" ]]; then
  exec 2>&{debug_fd}
  set -x
fi

# Helpful traps to see unexpected exits/signals in debug mode
trap 'rc=$?; echo "PSH: bootstrap exiting rc=$rc" >&200' EXIT
trap 'echo "PSH: got signal" >&200' HUP INT TERM

# ── Main loop: read two NUL‑terminated fields (token, command) from FD 9 ───────
while IFS= read -r -d $'\\0' -u 9 __psh_token; do
  if ! IFS= read -r -d $'\\0' -u 9 __psh_cmd; then
    printf '\\x1ePSHEXIT:%s:%d\\x1e\\n' "$__psh_token" 98
    continue
  fi

  builtin eval -- "$__psh_cmd"
  __psh_status=$?

  builtin printf '\\x1ePSHEXIT:%s:%d\\x1e\\n' "$__psh_token" "$__psh_status"
done

exit 0
"""

        argv = [self._shell_path]
        if login:
            argv.append("-l")
        argv += ["-c", bootstrap]

        shell_env = env.copy() if env else os.environ.copy()
        if debug:
            shell_env["PSH_DEBUG"] = "1"

        self._proc = subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            cwd=cwd,
            env=shell_env,
            bufsize=0,
            close_fds=True,
            pass_fds=(self._ctrl_r,),
            text=False
        )

        self._sel = selectors.DefaultSelector()
        if self._proc.stdout is None:
            raise RuntimeError("Failed to create pipes for persistent shell.")
        self._sel.register(self._proc.stdout, selectors.EVENT_READ)

        self._ctrl_wf = os.fdopen(self._ctrl_w, "wb", buffering=0)

    def _assert_alive(self):
        if self._proc.poll() is not None:
            raise RuntimeError(f"Persistent shell exited with code {self._proc.returncode}.")

    def _write_frame(self, token: str, command: str):
        try:
            self._ctrl_wf.write(token.encode("utf-8") + b"\x00" +
                                command.encode("utf-8") + b"\x00")
            self._ctrl_wf.flush()
        except BrokenPipeError:
            raise RuntimeError("Persistent shell control channel closed.")

    def _read_until_sentinel(self, token: str, timeout: Optional[float]) -> Tuple[bytes, int]:
        self._assert_alive()

        token_b = token.encode("utf-8")
        prefix = self._RS + b"PSHEXIT:" + token_b + b":"
        suffix = self._RS + b"\n"

        buf = bytearray()
        if self._residual:
            buf += self._residual
            self._residual = bytearray()

        end_time = (time.monotonic() + timeout) if timeout else None

        def time_left():
            if end_time is None:
                return None
            return max(0.0, end_time - time.monotonic())

        while True:
            idx = buf.find(prefix)
            if idx != -1:
                after = buf[idx + len(prefix):]
                j = after.find(suffix)
                if j != -1:
                    exit_bytes = after[:j]
                    try:
                        exit_code = int(exit_bytes.decode("ascii", "strict"))
                    except Exception:
                        raise RuntimeError("Malformed sentinel from persistent shell.")
                    before = bytes(buf[:idx])
                    remaining = bytes(after[j + len(suffix):])
                    self._residual.extend(remaining)
                    return before, exit_code

            self._assert_alive()
            timeout_this = time_left()
            events = self._sel.select(timeout_this)
            if not events:
                if end_time is not None and time.monotonic() >= end_time:
                    raise TimeoutError("Timed out waiting for command to complete.")
                continue

            for key, _ in events:
                if self._proc.stdout:
                    chunk = self._proc.stdout.read(self._read_chunk)
                    if chunk is None:
                        continue
                    if chunk == b"":
                        raise RuntimeError("Shell terminated unexpectedly while reading output.")
                    buf.extend(chunk)

    def run_command(self, command: str, *, timeout: Optional[float] = None) -> Tuple[str, int]:
        """Execute a command in the persistent shell and return (combined_output, exit_code)."""
        if "\x00" in command:
            raise ValueError("Command may not contain NUL characters.")

        with self._lock:
            self._assert_alive()
            token = uuid.uuid4().hex
            self._write_frame(token, command)
            out_bytes, exit_code = self._read_until_sentinel(token, timeout)
            output = out_bytes.decode(self._encoding, errors="replace")
            return output, exit_code

    def close(self):
        """Cleanly shut down the shell process."""
        with self._lock:
            try:
                if hasattr(self, "_ctrl_wf") and self._ctrl_wf:
                    self._ctrl_wf.close()
            except Exception:
                pass
            try:
                if self._proc.poll() is None:
                    try:
                        self._proc.wait(timeout=2.0)
                    except subprocess.TimeoutExpired:
                        self._proc.terminate()
                        try:
                            self._proc.wait(timeout=2.0)
                        except subprocess.TimeoutExpired:
                            self._proc.kill()
            finally:
                try:
                    if self._proc.stdout:
                        self._sel.unregister(self._proc.stdout)
                except Exception:
                    pass
                try:
                    if self._proc.stdout:
                        self._proc.stdout.close()
                except Exception:
                    pass
                try:
                    os.close(self._ctrl_r)
                except Exception:
                    pass

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        self.close()


def run_step(
    shell: PersistentShell,
    title: str,
    commands: list[str] | tuple[str, ...] | str,
    commentary: str | None = None,
    *,
    stop_on_success: bool = False,
) -> list[str]:
    """Execute commands using persistent shell and render the result in Markdown."""

    if isinstance(commands, str):
        command_list = [textwrap.dedent(commands).strip()]
    else:
        command_list = [textwrap.dedent(cmd).strip() for cmd in commands]

    outputs: list[str] = []
    aggregated_lines: list[str] = []
    success = False
    last_error: subprocess.CalledProcessError | None = None
    failure_output: str = ""

    print(f"## {title}\n")
    if commentary:
        print(f"{commentary}\n")

    print("```")
    for index, command in enumerate(command_list):
        if not command:
            continue

        display_command = sanitize_command(command)
        print(display_command)

        try:
            output, exit_code = shell.run_command(command)
            if exit_code != 0:
                raise subprocess.CalledProcessError(exit_code, command, output=output)
            success = True
        except subprocess.CalledProcessError as error:
            output = error.output if hasattr(error, 'output') else ""
            last_error = error
            if not stop_on_success:
                if output:
                    print("")
                    for line in output.splitlines():
                        print(f"{BOX_PREFIX}{line}")
                print("```")
                print("")
                raise SystemExit(error.returncode) from error
            failure_output = output
            continue
        except Exception as error:
            output = str(error)
            last_error = subprocess.CalledProcessError(1, command, output=output)
            if not stop_on_success:
                if output:
                    print("")
                    for line in output.splitlines():
                        print(f"{BOX_PREFIX}{line}")
                print("```")
                print("")
                raise SystemExit(1) from error
            failure_output = output
            continue

        outputs.append(output)
        if output:
            aggregated_lines.extend(output.splitlines())

        if stop_on_success and success:
            break

    if stop_on_success and not success and last_error is not None:
        if failure_output:
            print("")
            for line in failure_output.splitlines():
                print(f"{BOX_PREFIX}{line}")
        print("```")
        print("")
        raise SystemExit(last_error.returncode) from last_error

    if aggregated_lines:
        print("")
        print("\n".join(f"{BOX_PREFIX}{line}" for line in aggregated_lines))
    print("```")
    print("")

    return outputs


def qp(path: Path) -> str:
    """Shell-quote a filesystem path."""

    return shlex.quote(rel(path))


def rel(path: Path) -> str:
    """Return *path* relative to the script directory when possible."""

    try:
        return str(path.relative_to(SCRIPT_DIR))
    except ValueError:
        return str(path)


def sanitize_command(command: str) -> str:
    display = command
    for abs_path, rel_path in PATH_REPLACEMENTS:
        display = display.replace(abs_path, rel_path)
    return display


def register_path(path: Path) -> Path:
    """Record *path* for later sanitization and return it unchanged."""

    normalized = path if path.is_absolute() else (SCRIPT_DIR / path).resolve()
    normalized = normalized.resolve()
    if normalized in PATH_OBJECTS:
        return normalized

    PATH_OBJECTS.add(normalized)
    abs_path = str(normalized)
    rel_path = rel(normalized)
    PATH_REPLACEMENTS.append((abs_path, rel_path))
    PATH_REPLACEMENTS.append((shlex.quote(abs_path), rel_path))
    PATH_REPLACEMENTS.append((f"@{abs_path}", f"@{rel_path}"))
    PATH_REPLACEMENTS.append((f"@{shlex.quote(abs_path)}", f"@{rel_path}"))
    return normalized


def main() -> None:
    with PersistentShell(cwd=str(SCRIPT_DIR), env=ENV, debug=False) as shell:
        run_step(
            shell,
            "Set zsh options",
            "setopt nobanghist",
            "zsh is the default shell on macOS and many Linux systems. This keeps history markers out of the transcript.",
        )

        run_step(
            shell,
            "Checking prerequisites",
            """
for cmd in frost envelope; do
  $cmd --version
done
""",
            "Verify that the required CLI tools are present and available in $PATH.",
        )

        run_step(
            shell,
            "Configuring storage backend",
            """
STORAGE=server
TIMEOUT=600
""",
            "Set the storage backend for Hubert. Can be 'server', 'dht', 'ipfs', or 'hybrid'.",
        )

        run_step(
            shell,
            "Preparing demo workspace",
            f"rm -rf {qp(DEMO_DIR)} && mkdir -p {qp(DEMO_DIR)}",
            "Start with a clean directory to capture demo artifacts.",
        )

        for name in PARTICIPANTS:
            upper = name.upper()
            title = name.title()
            script = f"""
{upper}_PRVKEYS=$(envelope generate prvkeys)
echo "{upper}_PRVKEYS=${upper}_PRVKEYS"
{upper}_PUBKEYS=$(envelope generate pubkeys "${upper}_PRVKEYS")
echo "{upper}_PUBKEYS=${upper}_PUBKEYS"
{upper}_OWNER_DOC=$(envelope xid new --nickname {shlex.quote(title)} --sign inception "${upper}_PRVKEYS")
echo "{upper}_OWNER_DOC=${upper}_OWNER_DOC"
{upper}_SIGNED_DOC=$(envelope xid new --nickname {shlex.quote(title)} --private omit --sign inception "${upper}_PRVKEYS")
echo "{upper}_SIGNED_DOC=${upper}_SIGNED_DOC"
"""
            run_step(
                shell,
                f"Provisioning XID for {title}",
                script,
                commentary=f"Generate {title}'s key material, a private XID document (for owner use), and a signed public XID document (for participants).",
            )

        for owner in PARTICIPANTS:
            owner_title = owner.title()
            owner_upper = owner.upper()
            registry_var = f"{owner_upper}_REGISTRY"
            registry_path = REGISTRIES[owner]
            participant_lines = []
            for other in PARTICIPANTS:
                if other == owner:
                    continue
                other_upper = other.upper()
                participant_lines.append(
                    f'frost registry participant add --registry "${registry_var}" "${other_upper}_SIGNED_DOC" {other.title()}'
                )
            participant_block = "\n".join(participant_lines)
            cat_registry = (
                f'cat "${{{registry_var}}}"' if owner == "alice" else ""
            )
            script = f"""
{registry_var}={qp(registry_path)}
frost registry owner set --registry "${registry_var}" "${owner_upper}_OWNER_DOC" {owner_title}
{participant_block}
{cat_registry}
"""
            run_step(
                shell,
                f"Building {owner_title}'s registry",
                script,
                commentary=(
                    f"Set {owner_title} as the registry owner using the private XID document, "
                    "then add the other three participants with their signed XID documents."
                ),
            )

        run_step(
            shell,
            "Composing Alice's preview DKG invite",
            f"""
ALICE_INVITE_PREVIEW=$(frost dkg coordinator invite send --registry {qp(REGISTRIES["alice"])} --preview --min-signers 2 --charter "This group will authorize new club editions." Bob Carol Dan)
echo "${{ALICE_INVITE_PREVIEW}}" | envelope format
""",
            commentary=(
                "Create a 2-of-3 DKG invite for Bob, Carol, and Dan (from Alice's registry) "
                "as a preview envelope UR for auditing."
            ),
        )

        run_step(
            shell,
            "Composing Alice's sealed DKG invite",
            f"""
ALICE_INVITE_SEALED=$(frost dkg coordinator invite send --registry {qp(REGISTRIES["alice"])} --min-signers 2 --charter "This group will authorize new club editions." Bob Carol Dan)
echo "${{ALICE_INVITE_SEALED}}" | envelope format
echo "${{ALICE_INVITE_SEALED}}" | envelope info
""",
            commentary=(
                "Seal the 2-of-3 invite for Bob, Carol, and Dan and format the sealed envelope "
                "to view the encrypted recipient entries."
            ),
        )

        run_step(
            shell,
            "Checking Hubert server availability",
            "frost check --verbose --storage $STORAGE",
            "Verify the local Hubert server is responding before publishing the invite.",
        )

        run_step(
            shell,
            "Sending sealed DKG invite to Hubert",
            f"""
ALICE_INVITE_ARID=$(frost dkg coordinator invite send --storage $STORAGE --registry {qp(REGISTRIES["alice"])} --min-signers 2 --charter "This group will authorize new club editions." Bob Carol Dan)
echo "${{ALICE_INVITE_ARID}}"
""",
            commentary=(
                "Seal the invite and store it in Hubert using the default server backend; "
                "the printed ARID (UR) can be shared out-of-band."
            ),
        )

        run_step(
            shell,
            "Receiving invite from Hubert as Bob",
            f"""
BOB_INVITE=$(frost dkg participant invite receive --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{ALICE_INVITE_ARID}}")
frost dkg participant invite receive --info --registry {qp(REGISTRIES["bob"])} "${{BOB_INVITE}}"
""",
            commentary=(
                "Retrieve the invite from Hubert using Bob's registry (capturing the envelope), "
                "then show the invite details using the cached envelope."
            ),
        )

        run_step(
            shell,
            "Composing Bob's preview invite response",
            f"""
BOB_RESPONSE_PREVIEW=$(frost dkg participant invite respond --preview --registry {qp(REGISTRIES["bob"])} "${{BOB_INVITE}}")
echo "${{BOB_RESPONSE_PREVIEW}}" | envelope format
""",
            commentary=(
                "Preview the response envelope structure before posting. "
                "This shows the DKG Round 1 package and group metadata."
            ),
        )

        run_step(
            shell,
            "Composing Bob's sealed invite response",
            f"""
BOB_RESPONSE_SEALED=$(frost dkg participant invite respond --registry {qp(REGISTRIES["bob"])} "${{BOB_INVITE}}")
echo "${{BOB_RESPONSE_SEALED}}" | envelope format
""",
            commentary=(
                "The sealed response is encrypted to a single recipient (Alice, the coordinator)."
            ),
        )

        run_step(
            shell,
            "Bob responds to the invite",
            f"""
frost dkg participant invite respond --verbose --storage $STORAGE --registry {qp(REGISTRIES["bob"])} "${{BOB_INVITE}}"
""",
            commentary=(
                "Post Bob's sealed response to Hubert using the cached invite envelope."
            ),
        )

        run_step(
            shell,
            "Carol and Dan respond to the invite",
            f"""
frost dkg participant invite respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["carol"])} "${{ALICE_INVITE_ARID}}"
frost dkg participant invite respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["dan"])} "${{ALICE_INVITE_ARID}}"
""",
            commentary=(
                "Carol and Dan accept the invite from Hubert using their registries, posting their responses to Hubert."
            ),
        )

        run_step(
            shell,
            "Inspecting Alice's registry after sending invite",
            f"""
jq . {qp(REGISTRIES["alice"])}
""",
            commentary=(
                "Alice's registry now contains the group record with pending_requests "
                "listing the response ARIDs for each participant."
            ),
        )

        run_step(
            shell,
            "Alice collects Round 1 responses and sends Round 2 requests",
            f"""
# Get the group ID from Alice's registry
ALICE_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["alice"])})
echo "Group ID: ${{ALICE_GROUP_ID}}"

# Collect Round 1 responses and dispatch Round 2 (with a preview of one request)
ROUND1_PREVIEW=$(frost dkg coordinator round1 --preview --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["alice"])} "${{ALICE_GROUP_ID}}" | tee /dev/stderr | tail -n1)
echo "${{ROUND1_PREVIEW}}" | envelope format
""",
            commentary=(
                "Alice fetches each participant's sealed response from Hubert, "
                "saves the Round 1 packages, and immediately posts individualized Round 2 requests. "
                "One Round 2 request is previewed while posting."
            ),
        )

        run_step(
            shell,
            "Inspecting collected Round 1 packages",
            f"""
jq . {qp(PARTICIPANT_DIRS["alice"])}/group-state/*/collected_round1.json
""",
            commentary=(
                "The collected Round 1 packages are stored in Alice's group-state directory, "
                "ready for Round 2 processing."
            ),
        )

        # ── DKG Round 2 ─────────────────────────────────────────────────

        run_step(
            shell,
            "Inspecting Alice's registry after Round 2 dispatch",
            f"""
jq '.groups' {qp(REGISTRIES["alice"])}
""",
            commentary=(
                "Alice's registry now has pending_requests with the coordinator's collection ARIDs "
                "for Round 2 responses."
            ),
        )

        run_step(
            shell,
            "Checking Bob's listening ARID for Round 2",
            f"""
jq '.groups[].listening_at_arid' {qp(REGISTRIES["bob"])}
""",
            commentary=(
                "Bob's registry still records the ARID where Alice posted the Round 2 request."
            ),
        )

        # ── Participants respond to Round 2 ─────────────────────────────

        # Test with just Bob first
        run_step(
            shell,
            "Bob responds to Round 2 request",
            f"""
BOB_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["bob"])})
frost dkg participant round2 respond --preview --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_GROUP_ID}}" | envelope format
""",
            commentary=(
                "Bob fetches the Round 2 request, runs FROST DKG part2 "
                "with his Round 1 secret and all Round 1 packages, generates Round 2 packages, "
                "and prints the preview response envelope structure (no post)."
            ),
        )

        run_step(
            shell,
            "Bob posts Round 2 response",
            f"""
BOB_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["bob"])})
frost dkg participant round2 respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_GROUP_ID}}"
""",
            commentary=(
                "Bob posts the sealed Round 2 response to the coordinator (no preview output)."
            ),
        )

        run_step(
            shell,
            "Carol responds to Round 2 request",
            f"""
CAROL_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["carol"])})
frost dkg participant round2 respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["carol"])} "${{CAROL_GROUP_ID}}"
""",
            commentary=(
                "Carol processes the Round 2 request with her Round 1 secret and all Round 1 packages, "
                "generates Round 2 packages, and posts them back to the coordinator."
            ),
        )

        run_step(
            shell,
            "Dan responds to Round 2 request",
            f"""
DAN_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["dan"])})
frost dkg participant round2 respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["dan"])} "${{DAN_GROUP_ID}}"
""",
            commentary=(
                "Dan processes the Round 2 request with his Round 1 secret and all Round 1 packages, "
                "generates Round 2 packages, and posts them back to the coordinator."
            ),
        )

        run_step(
            shell,
            "Alice collects Round 2 responses",
            f"""
ALICE_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["alice"])})
frost dkg coordinator round2 collect --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["alice"])} "${{ALICE_GROUP_ID}}"
""",
            commentary=(
                "Alice fetches Round 2 responses from Hubert, validates them, saves collected packages, "
                "and records each participant's next response ARID for the finalize phase."
            ),
        )

        run_step(
            shell,
            "Inspecting collected Round 2 packages",
            f"""
jq . {qp(PARTICIPANT_DIRS["alice"])}/group-state/*/collected_round2.json
""",
            commentary="Collected Round 2 packages with each sender's next response ARID.",
        )

        # ── DKG Finalize send (distribution of round2 packages) ─────────────

        run_step(
            shell,
            "Alice composes a preview finalize request (for first participant)",
            f"""
FINALIZE_PREVIEW=$(frost dkg coordinator finalize send --preview --registry {qp(REGISTRIES["alice"])} "${{ALICE_GROUP_ID}}")
echo "${{FINALIZE_PREVIEW}}" | envelope format
""",
            commentary=(
                "Preview the finalize request structure that delivers incoming Round 2 packages "
                "to a participant along with their responseArid for finalize respond."
            ),
        )

        run_step(
            shell,
            "Alice sends finalize packages to each participant",
            f"""
frost dkg coordinator finalize send --verbose --storage $STORAGE --registry {qp(REGISTRIES["alice"])} "${{ALICE_GROUP_ID}}"
""",
            commentary=(
                "Alice posts the finalize requests (with each participant's incoming Round 2 packages) "
                "to the ARIDs provided in Round 2 collect."
            ),
        )

        # ── Participants respond to finalize ──────────────────────────────

        run_step(
            shell,
            "Bob previews finalize response",
            f"""
BOB_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["bob"])})
frost dkg participant finalize respond --preview --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_GROUP_ID}}" | envelope format
""",
            commentary=(
                "Bob previews his finalize response structure (key packages) without posting."
            ),
        )

        run_step(
            shell,
            "Bob posts finalize response",
            f"""
BOB_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["bob"])})
frost dkg participant finalize respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_GROUP_ID}}"
""",
            commentary="Bob posts his finalize response with generated key packages.",
        )

        run_step(
            shell,
            "Carol posts finalize response",
            f"""
CAROL_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["carol"])})
frost dkg participant finalize respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["carol"])} "${{CAROL_GROUP_ID}}"
""",
            commentary="Carol posts her finalize response with generated key packages.",
        )

        run_step(
            shell,
            "Dan posts finalize response",
            f"""
DAN_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["dan"])})
frost dkg participant finalize respond --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["dan"])} "${{DAN_GROUP_ID}}"
""",
            commentary="Dan posts his finalize response with generated key packages.",
        )

        run_step(
            shell,
            "Alice collects finalize responses",
            f"""
ALICE_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["alice"])})
frost dkg coordinator finalize collect --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["alice"])} "${{ALICE_GROUP_ID}}"
""",
            commentary=(
                "Alice fetches all finalize responses, validates them, saves collected "
                "key packages, and reports the group verifying key."
            ),
        )

        run_step(
            shell,
            "Inspecting collected finalize responses",
            f"""
jq . {qp(PARTICIPANT_DIRS["alice"])}/group-state/*/collected_finalize.json
""",
            commentary="Collected finalize responses keyed by participant XID.",
        )

        run_step(
            shell,
            "Verifying group key across all participants",
            f"""
for name in {" ".join(PARTICIPANTS)}; do
  echo "$name:"
  jq -r '.groups | to_entries[0].value.verifying_key' {qp(DEMO_DIR)}/$name/registry.json
done
""",
            commentary=(
                "Each registry records the same group verifying key (UR form)."
            ),
        )

        # ── Signing session setup (start) ────────────────────────────────

        run_step(
            shell,
            "Compose target envelope for signing",
            f"""
BASE_ENVELOPE=$(envelope subject type string "FROST signing demo target")
TARGET_ENVELOPE=$(echo "${{BASE_ENVELOPE}}" | envelope assertion add pred-obj string purpose string "threshold signing demo")
WRAPPED_TARGET=$(envelope subject type wrapped "${{TARGET_ENVELOPE}}")
echo "${{WRAPPED_TARGET}}" > {qp(SIGN_TARGET)}
envelope format < {qp(SIGN_TARGET)}
""",
            commentary=(
                "Build a sample target envelope with an assertion, wrap it for signing, "
                "and show its structure."
            ),
        )

        run_step(
            shell,
            "Preview signCommit request (unsealed)",
            f"""
ALICE_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["alice"])})
frost sign coordinator start --preview --target {qp(SIGN_TARGET)} --registry {qp(REGISTRIES["alice"])} "${{ALICE_GROUP_ID}}" | envelope format
""",
            commentary=(
                "Preview the unsealed signCommit GSTP request (initial signing hop)."
            ),
        )

        run_step(
            shell,
            "Post signCommit request to Hubert",
            f"""
ALICE_GROUP_ID=$(jq -r '.groups | keys[0]' {qp(REGISTRIES["alice"])})
ALICE_SIGN_START_ARID=$(frost sign coordinator start --verbose --storage $STORAGE --registry {qp(REGISTRIES["alice"])} --target {qp(SIGN_TARGET)} "${{ALICE_GROUP_ID}}")
echo "${{ALICE_SIGN_START_ARID}}"
""",
            commentary=(
                "Coordinator posts the signCommit request to a single first-hop ARID (printed)."
            ),
        )

        run_step(
            shell,
            "Bob inspects signCommit request",
            f"""
START_PATH=$(ls -t demo/alice/group-state/*/signing/*/start.json | head -n1)
ALICE_SIGN_START_ARID=$(jq -r '.start_arid' "${{START_PATH}}")
BOB_SESSION_ID=$(frost sign participant receive --info --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{ALICE_SIGN_START_ARID}}" | tee /dev/stderr | tail -n1)
""",
            commentary=(
                "Bob fetches and decrypts the signCommit request via Hubert and views the details of the session."
            ),
        )

        run_step(
            shell,
            "Carol inspects signCommit request",
            f"""
START_PATH=$(ls -t demo/alice/group-state/*/signing/*/start.json | head -n1)
ALICE_SIGN_START_ARID=$(jq -r '.start_arid' "${{START_PATH}}")
CAROL_SESSION_ID=$(frost sign participant receive --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["carol"])} "${{ALICE_SIGN_START_ARID}}" | tee /dev/stderr | tail -n1)
""",
            commentary=(
                "Carol fetches and decrypts the signCommit request via Hubert."
            ),
        )

        run_step(
            shell,
            "Dan inspects signCommit request",
            f"""
START_PATH=$(ls -t demo/alice/group-state/*/signing/*/start.json | head -n1)
ALICE_SIGN_START_ARID=$(jq -r '.start_arid' "${{START_PATH}}")
DAN_SESSION_ID=$(frost sign participant receive --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["dan"])} "${{ALICE_SIGN_START_ARID}}" | tee /dev/stderr | tail -n1)
""",
            commentary=(
                "Dan fetches and decrypts the signCommit request via Hubert."
            ),
        )

        run_step(
            shell,
            "Bob previews signCommit response",
            f"""
frost sign participant commit --preview --registry {qp(REGISTRIES["bob"])} "${{BOB_SESSION_ID}}" | envelope format
""",
            commentary=(
                "Bob dry-runs his signCommit response, showing commitments and next-hop response ARID without posting."
            ),
        )

        run_step(
            shell,
            "Bob posts signCommit response",
            f"""
frost sign participant commit --verbose --storage $STORAGE --registry {qp(REGISTRIES["bob"])} "${{BOB_SESSION_ID}}"
""",
            commentary=(
                "Bob posts his signCommit response to the coordinator."
            ),
        )

        run_step(
            shell,
            "Carol posts signCommit response",
            f"""
frost sign participant commit --storage $STORAGE --registry {qp(REGISTRIES["carol"])} "${{CAROL_SESSION_ID}}"
""",
            commentary=(
                "Carol posts her signCommit response to the coordinator."
            ),
        )

        run_step(
            shell,
            "Dan posts signCommit response",
            f"""
frost sign participant commit --storage $STORAGE --registry {qp(REGISTRIES["dan"])} "${{DAN_SESSION_ID}}"
""",
            commentary=(
                "Dan posts his signCommit response to the coordinator."
            ),
        )

        run_step(
            shell,
            "Alice collects commitments and posts signShare requests",
            f"""
START_PATH=$(ls -t {qp(PARTICIPANT_DIRS["alice"])}/group-state/*/signing/*/start.json | head -n1)
SESSION_ID=$(jq -r '.session_id' "${{START_PATH}}")
frost sign coordinator collect --preview-share --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["alice"])} "${{SESSION_ID}}"
""",
            commentary=(
                "Alice gathers the signCommit responses, aggregates commitments, sends per-participant signShare "
                "requests, and tells participants where to post their signature shares (share ARIDs)."
            ),
        )

        run_step(
            shell,
            "Inspecting collected commitments",
            f"""
COMMITMENTS_PATH=$(ls -t {qp(PARTICIPANT_DIRS["alice"])}/group-state/*/signing/*/commitments.json | head -n1)
jq . "${{COMMITMENTS_PATH}}"
""",
            commentary="Commitments and ARIDs keyed by participant XID.",
        )

        run_step(
            shell,
            "Bob previews signShare response",
            f"""
frost sign participant share --preview --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_SESSION_ID}}" | envelope format
""",
            commentary=(
                "Bob fetches the signShare request, validates commitments, "
                "computes his signature share, and previews the response without posting."
            ),
        )

        run_step(
            shell,
            "Bob posts signShare response",
            f"""
frost sign participant share --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_SESSION_ID}}"
""",
            commentary="Bob posts his signature share to the coordinator's share collection ARID.",
        )

        run_step(
            shell,
            "Carol posts signShare response",
            f"""
frost sign participant share --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["carol"])} "${{CAROL_SESSION_ID}}"
""",
            commentary="Carol posts her signature share to the coordinator.",
        )

        run_step(
            shell,
            "Dan posts signShare response",
            f"""
frost sign participant share --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["dan"])} "${{DAN_SESSION_ID}}"
""",
            commentary="Dan posts his signature share to the coordinator.",
        )

        run_step(
            shell,
            "Inspecting Bob's signature share",
            f"""
SHARE_PATH=$(ls -t {qp(PARTICIPANT_DIRS["bob"])}/group-state/*/signing/*/share.json | head -n1)
jq . "${{SHARE_PATH}}"
""",
            commentary="Persisted signature share and signing context for Bob.",
        )

        run_step(
            shell,
            "Alice finalizes and posts finalize packages",
            f"""
START_PATH=$(ls -t {qp(PARTICIPANT_DIRS["alice"])}/group-state/*/signing/*/start.json | head -n1)
SESSION_ID=$(jq -r '.session_id' "${{START_PATH}}")
frost sign coordinator finalize --preview-finalize --verbose --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["alice"])} "${{SESSION_ID}}"
""",
            commentary=(
                "Alice collects signature shares, aggregates the signature, and dispatches finalize packages; "
                "prints a preview of one finalize message."
            ),
        )

        run_step(
            shell,
            "Bob attaches aggregated signature",
            f"""
BOB_ATTACH=$(frost sign participant attach --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["bob"])} "${{BOB_SESSION_ID}}")
echo "${{BOB_ATTACH}}"
BOB_SIGNED=$(echo "${{BOB_ATTACH}}" | tail -n1)
echo "${{BOB_SIGNED}}" | envelope format
""",
            commentary=(
                "Bob fetches his finalize package, rebuilds the aggregated signature locally, "
                "attaches it to the target envelope, and formats the signed result."
            ),
        )

        run_step(
            shell,
            "Carol attaches aggregated signature",
            f"""
frost sign participant attach --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["carol"])} "${{CAROL_SESSION_ID}}"
""",
            commentary=(
                "Carol retrieves her finalize package and attaches the verified group signature."
            ),
        )

        run_step(
            shell,
            "Dan attaches aggregated signature",
            f"""
frost sign participant attach --storage $STORAGE --timeout $TIMEOUT --registry {qp(REGISTRIES["dan"])} "${{DAN_SESSION_ID}}"
""",
            commentary=(
                "Dan rebuilds and verifies the signature from his finalize package."
            ),
        )

        run_step(
            shell,
            "Inspecting Bob's final signature state",
            f"""
FINAL_PATH=$(ls -t {qp(PARTICIPANT_DIRS["bob"])}/group-state/*/signing/*/final.json | head -n1)
jq . "${{FINAL_PATH}}"
""",
            commentary="Persisted aggregated signature, signature shares, and signed target UR for Bob.",
        )


SCRIPT_DIR = Path(__file__).resolve().parent

PATH_OBJECTS: set[Path] = set()
PATH_REPLACEMENTS: list[tuple[str, str]] = []

DEMO_DIR = register_path(SCRIPT_DIR / "demo")
SIGN_TARGET = register_path(DEMO_DIR / "target-envelope.ur")

PARTICIPANTS = ["alice", "bob", "carol", "dan"]

# Each participant has their own directory with registry.json inside
PARTICIPANT_DIRS = {
    name: register_path(DEMO_DIR / name)
    for name in PARTICIPANTS
}

REGISTRIES = {
    name: register_path(PARTICIPANT_DIRS[name] / "registry.json")
    for name in PARTICIPANTS
}

ENV = os.environ.copy()


if __name__ == "__main__":
    try:
        main()
    except SystemExit as exc:
        sys.exit(exc.code)
