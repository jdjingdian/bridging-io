#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
from contextlib import contextmanager
import datetime as dt
import fnmatch
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import tomllib
import zipfile


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG_PATH = REPO_ROOT / "tmp" / "local-preflight.toml"
DEFAULT_RUNS_DIR = REPO_ROOT / "tmp" / "local-preflight-runs"
SUPPORTED_MODES = {"compile-only", "unit", "extended"}
DEFAULT_EXCLUDES = [
    ".git/**",
    "**/target/**",
    "**/__pycache__/**",
    "*.pyc",
    "tmp/local-preflight-runs/**",
    "tmp/local-preflight.toml",
]
DEFAULT_COMMANDS = {
    "compile-only": "cargo test --workspace --no-run",
    "unit": "cargo test --workspace -- --test-threads=1",
    "extended": "cargo test --workspace -- --test-threads=1",
}
DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_COMMAND = (
    "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass "
    "-File ..\\..\\scripts\\testing\\run-windows-ssh-broker-contract.ps1"
)
DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_MODES = ["compile-only", "extended"]


class PreflightError(RuntimeError):
    pass


def now_stamp() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d-%H%M%S")


def info(message: str) -> None:
    print(f"[preflight] {message}")


def load_toml(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def normalize_path(path: str) -> str:
    return path.replace(os.sep, "/")


def should_exclude(rel_path: str, patterns: list[str]) -> bool:
    rel_path = normalize_path(rel_path).lstrip("./")
    if rel_path == "":
        return False
    if rel_path == ".git" or rel_path.startswith(".git/"):
        return True
    if rel_path.endswith("/target") or "/target/" in f"/{rel_path}/":
        return True
    for pattern in patterns:
        if fnmatch.fnmatch(rel_path, pattern):
            return True
    return False


def make_snapshot_archive(
    repo_root: Path,
    archive_path: Path,
    archive_kind: str,
    excludes: list[str],
) -> int:
    file_count = 0
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    if archive_kind == "zip":
        with zipfile.ZipFile(
            archive_path, mode="w", compression=zipfile.ZIP_DEFLATED
        ) as zf:
            for root, dirs, files in os.walk(repo_root):
                root_path = Path(root)
                rel_root = os.path.relpath(root_path, repo_root)
                rel_root = "" if rel_root == "." else normalize_path(rel_root)
                kept_dirs = []
                for dirname in dirs:
                    rel_dir = f"{rel_root}/{dirname}" if rel_root else dirname
                    if should_exclude(rel_dir, excludes):
                        continue
                    kept_dirs.append(dirname)
                dirs[:] = kept_dirs
                for filename in files:
                    rel_file = f"{rel_root}/{filename}" if rel_root else filename
                    if should_exclude(rel_file, excludes):
                        continue
                    zf.write(root_path / filename, rel_file)
                    file_count += 1
    elif archive_kind == "tar.gz":
        with tarfile.open(archive_path, mode="w:gz") as tf:
            for root, dirs, files in os.walk(repo_root):
                root_path = Path(root)
                rel_root = os.path.relpath(root_path, repo_root)
                rel_root = "" if rel_root == "." else normalize_path(rel_root)
                kept_dirs = []
                for dirname in dirs:
                    rel_dir = f"{rel_root}/{dirname}" if rel_root else dirname
                    if should_exclude(rel_dir, excludes):
                        continue
                    kept_dirs.append(dirname)
                dirs[:] = kept_dirs
                for filename in files:
                    rel_file = f"{rel_root}/{filename}" if rel_root else filename
                    if should_exclude(rel_file, excludes):
                        continue
                    tf.add(root_path / filename, arcname=rel_file, recursive=False)
                    file_count += 1
    else:
        raise PreflightError(f"unsupported archive kind: {archive_kind}")
    return file_count


def build_ssh_base(host_cfg: dict) -> list[str]:
    base = ["ssh", "-p", str(host_cfg["port"])]
    if host_cfg.get("identity_file"):
        base.extend(["-i", str(host_cfg["identity_file"])])
    base.append(f"{host_cfg['user']}@{host_cfg['host']}")
    return base


def build_scp_base(host_cfg: dict) -> list[str]:
    base = ["scp", "-P", str(host_cfg["port"])]
    if host_cfg.get("identity_file"):
        base.extend(["-i", str(host_cfg["identity_file"])])
    return base


def auth_mode_for_host(host_cfg: dict) -> str:
    if host_cfg.get("identity_file"):
        return "identity-file"
    if host_cfg.get("password"):
        return "password"
    return "interactive"


@contextmanager
def password_auth_context(host_cfg: dict):
    auth_mode = auth_mode_for_host(host_cfg)
    if auth_mode != "password":
        yield auth_mode, None, False
        return
    password = host_cfg.get("password", "")
    fd, askpass_path = tempfile.mkstemp(prefix="bridgingio-preflight-askpass-")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write("#!/bin/sh\n")
            fh.write('printf "%s\\n" "${BRIDGINGIO_PREFLIGHT_SSH_PASSWORD:-}"\n')
        os.chmod(askpass_path, 0o700)
        env = os.environ.copy()
        env["SSH_ASKPASS"] = askpass_path
        env["SSH_ASKPASS_REQUIRE"] = "force"
        env["BRIDGINGIO_PREFLIGHT_SSH_PASSWORD"] = password
        # Some OpenSSH builds require DISPLAY to invoke SSH_ASKPASS.
        env["DISPLAY"] = env.get("DISPLAY") or "bridgingio-preflight"
        yield auth_mode, env, True
    finally:
        try:
            Path(askpass_path).unlink()
        except OSError:
            pass


def run_and_log(
    cmd: list[str],
    log_path: Path,
    *,
    env: dict | None = None,
    non_interactive: bool = False,
    auth_mode: str = "interactive",
) -> subprocess.CompletedProcess:
    proc = subprocess.run(
        cmd,
        text=True,
        env=env,
        stdin=subprocess.DEVNULL if non_interactive else None,
    )
    with log_path.open("a", encoding="utf-8") as fh:
        fh.write(f"$ {' '.join(shlex.quote(part) for part in cmd)}\n")
        fh.write(f"[auth] {auth_mode}\n")
        if non_interactive:
            fh.write("[output] streamed to console (non-interactive SSH_ASKPASS mode)\n")
        else:
            fh.write("[output] streamed to console (interactive mode)\n")
        fh.write(f"[exit] {proc.returncode}\n\n")
    return proc


def encode_ps(script: str) -> str:
    return base64.b64encode(script.encode("utf-16le")).decode("ascii")


def parse_mode_overrides(raw_modes: object, default_modes: list[str]) -> list[str]:
    values: list[str] = []
    if isinstance(raw_modes, str):
        values = [raw_modes.strip()]
    elif isinstance(raw_modes, list):
        for value in raw_modes:
            if isinstance(value, str):
                values.append(value.strip())
    normalized: list[str] = []
    for mode in values:
        if mode in SUPPORTED_MODES and mode not in normalized:
            normalized.append(mode)
    if normalized:
        return normalized
    return list(default_modes)


def parse_host_config(raw: dict, name: str) -> dict:
    enabled = bool(raw.get("enabled", False))
    raw_password = raw.get("password", "")
    if raw_password is None:
        raw_password = ""
    if not isinstance(raw_password, str):
        raise PreflightError(f"[{name}] password must be a string")
    cfg = {
        "name": name,
        "enabled": enabled,
        "host": str(raw.get("host", "")).strip(),
        "user": str(raw.get("user", "")).strip(),
        "port": int(raw.get("port", 22)),
        "remote_base": str(raw.get("remote_base", "")).strip(),
        "identity_file": str(raw.get("identity_file", "")).strip(),
        "password": raw_password,
        "commands": {},
    }
    raw_cmd = raw.get("commands", {})
    if isinstance(raw_cmd, dict):
        for mode in SUPPORTED_MODES:
            value = raw_cmd.get(mode)
            if isinstance(value, str) and value.strip():
                cfg["commands"][mode] = value.strip()
    if name == "windows":
        git_bash = raw.get("git_bash", {})
        if not isinstance(git_bash, dict):
            git_bash = {}
        cfg["git_bash_enabled"] = bool(git_bash.get("enabled", False))
        cfg["git_bash_extra_check"] = str(
            git_bash.get("extra_check", "cargo test --workspace --no-run")
        ).strip()

        contracts = raw.get("contracts", {})
        if not isinstance(contracts, dict):
            contracts = {}
        cfg["named_pipe_contract_enabled"] = bool(contracts.get("enabled", True))
        named_pipe_contract_command = str(
            contracts.get("command", DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_COMMAND)
        ).strip()
        cfg["named_pipe_contract_command"] = (
            named_pipe_contract_command
            if named_pipe_contract_command
            else DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_COMMAND
        )
        cfg["named_pipe_contract_modes"] = parse_mode_overrides(
            contracts.get("modes", DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_MODES),
            DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_MODES,
        )
    if cfg["identity_file"]:
        cfg["identity_file"] = str(Path(cfg["identity_file"]).expanduser())
    return cfg


def load_config(config_path: Path) -> dict | None:
    if not config_path.exists():
        return None
    data = load_toml(config_path)
    preflight = data.get("preflight", {})
    if not isinstance(preflight, dict):
        raise PreflightError("[preflight] section must be a table")
    mode = str(preflight.get("mode", "compile-only")).strip()
    if mode not in SUPPORTED_MODES:
        raise PreflightError(
            f"unsupported mode `{mode}`; expected one of: {', '.join(sorted(SUPPORTED_MODES))}"
        )
    runs_dir_raw = str(preflight.get("runs_dir", "tmp/local-preflight-runs")).strip()
    runs_dir = Path(runs_dir_raw)
    if not runs_dir.is_absolute():
        runs_dir = REPO_ROOT / runs_dir
    excludes = list(DEFAULT_EXCLUDES)
    extra_excludes = preflight.get("exclude", [])
    if isinstance(extra_excludes, list):
        for item in extra_excludes:
            if isinstance(item, str) and item.strip():
                excludes.append(item.strip())
    cleanup_remote = bool(preflight.get("cleanup_remote_on_success", True))

    linux_cfg = parse_host_config(data.get("linux", {}), "linux")
    windows_cfg = parse_host_config(data.get("windows", {}), "windows")
    return {
        "mode": mode,
        "runs_dir": runs_dir,
        "excludes": excludes,
        "cleanup_remote_on_success": cleanup_remote,
        "linux": linux_cfg,
        "windows": windows_cfg,
    }


def ensure_tools(required: list[str]) -> None:
    missing = [tool for tool in required if shutil_which(tool) is None]
    if missing:
        raise PreflightError(f"missing required tools: {', '.join(missing)}")


def shutil_which(name: str) -> str | None:
    for path_dir in os.environ.get("PATH", "").split(os.pathsep):
        candidate = Path(path_dir) / name
        if candidate.exists() and os.access(candidate, os.X_OK):
            return str(candidate)
        if sys.platform.startswith("win"):
            exe = candidate.with_suffix(".exe")
            if exe.exists() and os.access(exe, os.X_OK):
                return str(exe)
    return None


def host_command(host_cfg: dict, mode: str) -> str:
    return host_cfg["commands"].get(mode) or DEFAULT_COMMANDS[mode]


def run_linux_host(
    host_cfg: dict,
    mode: str,
    run_id: str,
    archive_path: Path,
    run_log: Path,
    cleanup_remote_on_success: bool,
    dry_run: bool,
) -> dict:
    primary_command = host_command(host_cfg, mode)
    auth_mode = auth_mode_for_host(host_cfg)
    ssh_base = build_ssh_base(host_cfg)
    scp_base = build_scp_base(host_cfg)
    remote_archive = f"bridgingio-preflight-{run_id}.tar.gz"
    remote_archive_path = f"~/{remote_archive}"
    remote_base = host_cfg["remote_base"] or "$HOME/bridgingio-local-preflight"
    remote_run_dir = f"{remote_base}/runs/{run_id}"
    remote_script = textwrap.dedent(
        f"""\
        set -euo pipefail
        REMOTE_BASE={shlex.quote(remote_base)}
        RUN_DIR={shlex.quote(remote_run_dir)}
        ARCHIVE={shlex.quote(remote_archive_path)}
        PRIMARY_COMMAND={shlex.quote(primary_command)}
        CLEANUP_ON_SUCCESS={"1" if cleanup_remote_on_success else "0"}
        mkdir -p "$RUN_DIR"
        tar -xzf "$ARCHIVE" -C "$RUN_DIR"
        cd "$RUN_DIR/source/rust"
        bash -lc "$PRIMARY_COMMAND"
        if [ "$CLEANUP_ON_SUCCESS" = "1" ]; then
          rm -rf "$RUN_DIR"
        fi
        rm -f "$ARCHIVE"
        """
    )
    scp_cmd = scp_base + [str(archive_path), f"{host_cfg['user']}@{host_cfg['host']}:{remote_archive_path}"]
    ssh_cmd = ssh_base + ["bash", "-lc", remote_script]
    if dry_run:
        with run_log.open("a", encoding="utf-8") as fh:
            fh.write(f"[dry-run] auth mode: {auth_mode}\n\n")
            fh.write("[dry-run] linux transfer command\n")
            fh.write(" ".join(shlex.quote(p) for p in scp_cmd) + "\n\n")
            fh.write("[dry-run] linux exec command\n")
            fh.write(" ".join(shlex.quote(p) for p in ssh_cmd) + "\n\n")
        return {
            "status": "dry-run",
            "mode": mode,
            "platform": "linux",
            "auth_mode": auth_mode,
            "primary_command": primary_command,
            "remote_run_dir": remote_run_dir,
            "log_path": str(run_log),
        }
    with password_auth_context(host_cfg) as (resolved_auth_mode, env, non_interactive):
        transfer = run_and_log(
            scp_cmd,
            run_log,
            env=env,
            non_interactive=non_interactive,
            auth_mode=resolved_auth_mode,
        )
        if transfer.returncode != 0:
            return {
                "status": "failed",
                "mode": mode,
                "platform": "linux",
                "auth_mode": resolved_auth_mode,
                "primary_command": primary_command,
                "remote_run_dir": remote_run_dir,
                "log_path": str(run_log),
                "error": "scp transfer failed",
            }
        execute = run_and_log(
            ssh_cmd,
            run_log,
            env=env,
            non_interactive=non_interactive,
            auth_mode=resolved_auth_mode,
        )
    status = "passed" if execute.returncode == 0 else "failed"
    result = {
        "status": status,
        "mode": mode,
        "platform": "linux",
        "auth_mode": auth_mode,
        "primary_command": primary_command,
        "remote_run_dir": remote_run_dir,
        "log_path": str(run_log),
    }
    if status == "failed":
        result["error"] = "remote command failed"
    return result


def run_windows_host(
    host_cfg: dict,
    mode: str,
    run_id: str,
    archive_path: Path,
    run_log: Path,
    cleanup_remote_on_success: bool,
    dry_run: bool,
) -> dict:
    primary_command = host_command(host_cfg, mode)
    auth_mode = auth_mode_for_host(host_cfg)
    ssh_base = build_ssh_base(host_cfg)
    scp_base = build_scp_base(host_cfg)
    remote_archive = f"bridgingio-preflight-{run_id}.zip"
    remote_base = host_cfg["remote_base"]
    git_bash_enabled = bool(host_cfg.get("git_bash_enabled", False))
    git_bash_extra = host_cfg.get("git_bash_extra_check", "cargo test --workspace --no-run")
    named_pipe_contract_enabled = bool(host_cfg.get("named_pipe_contract_enabled", True))
    named_pipe_contract_command = host_cfg.get(
        "named_pipe_contract_command", DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_COMMAND
    )
    named_pipe_contract_modes = host_cfg.get(
        "named_pipe_contract_modes", DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_MODES
    )
    if not isinstance(named_pipe_contract_modes, list):
        named_pipe_contract_modes = list(DEFAULT_WINDOWS_NAMED_PIPE_CONTRACT_MODES)
    run_named_pipe_contract = (
        named_pipe_contract_enabled and mode in set(named_pipe_contract_modes)
    )
    ps_script = textwrap.dedent(
        f"""\
        $ErrorActionPreference = "Stop"
        $ProgressPreference = "SilentlyContinue"
        $RunId = "{run_id}"
        $ArchivePath = Join-Path $HOME "{remote_archive}"
        $ConfiguredRemoteBase = "{remote_base.replace('\\', '\\\\').replace('"', '`"')}"
        if ([string]::IsNullOrWhiteSpace($ConfiguredRemoteBase)) {{
          $RemoteBase = Join-Path $env:USERPROFILE "bridgingio-local-preflight"
        }} else {{
          $RemoteBase = $ConfiguredRemoteBase
        }}
        $RunDir = Join-Path $RemoteBase ("runs\\\\" + $RunId)
        $WorkDir = Join-Path $RunDir "source\\\\rust"
        $PrimaryCommand = @'
{primary_command}
'@
        $EnableGitBashExtra = {"$true" if git_bash_enabled else "$false"}
        $GitBashExtra = @'
{git_bash_extra}
'@
        $RunNamedPipeContract = {"$true" if run_named_pipe_contract else "$false"}
        $NamedPipeContractCommand = @'
{named_pipe_contract_command}
'@
        $CleanupOnSuccess = {"$true" if cleanup_remote_on_success else "$false"}

        function Convert-ToGitBashPath([string]$Path) {{
          $Full = [System.IO.Path]::GetFullPath($Path)
          $Drive = $Full.Substring(0, 1).ToLowerInvariant()
          $Tail = $Full.Substring(2).Replace("\\\\", "/")
          return "/$Drive$Tail"
        }}

        if (Test-Path $RunDir) {{
          Remove-Item -LiteralPath $RunDir -Recurse -Force
        }}
        New-Item -ItemType Directory -Path $RunDir -Force | Out-Null
        Expand-Archive -Path $ArchivePath -DestinationPath $RunDir -Force

        Push-Location $WorkDir
        try {{
          if ($RunNamedPipeContract) {{
            cmd.exe /d /s /c $NamedPipeContractCommand
            if ($LASTEXITCODE -ne 0) {{
              throw "windows named-pipe runtime contract command failed with exit code $LASTEXITCODE"
            }}
          }}
          cmd.exe /d /s /c $PrimaryCommand
          if ($LASTEXITCODE -ne 0) {{
            throw "primary command failed with exit code $LASTEXITCODE"
          }}
          if ($EnableGitBashExtra) {{
            $BashCommand = Get-Command bash -ErrorAction SilentlyContinue
            if ($null -eq $BashCommand) {{
              Write-Output "[preflight] git bash not detected; skip optional extra check."
            }} else {{
              $WorkDirPosix = Convert-ToGitBashPath $WorkDir
              $BashScript = "cd '$WorkDirPosix' && $GitBashExtra"
              & bash -lc $BashScript
              if ($LASTEXITCODE -ne 0) {{
                throw "git bash extra check failed with exit code $LASTEXITCODE"
              }}
            }}
          }}
        }} finally {{
          Pop-Location
        }}

        if ($CleanupOnSuccess) {{
          Remove-Item -LiteralPath $RunDir -Recurse -Force
        }}
        Remove-Item -LiteralPath $ArchivePath -Force
        """
    )
    encoded_ps = encode_ps(ps_script)
    scp_cmd = scp_base + [str(archive_path), f"{host_cfg['user']}@{host_cfg['host']}:~/{remote_archive}"]
    ssh_cmd = ssh_base + [
        "powershell",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        encoded_ps,
    ]
    remote_run_dir = (
        f"{remote_base}\\runs\\{run_id}"
        if remote_base
        else f"$env:USERPROFILE\\bridgingio-local-preflight\\runs\\{run_id}"
    )
    if dry_run:
        with run_log.open("a", encoding="utf-8") as fh:
            fh.write(f"[dry-run] auth mode: {auth_mode}\n\n")
            fh.write("[dry-run] windows transfer command\n")
            fh.write(" ".join(shlex.quote(p) for p in scp_cmd) + "\n\n")
            fh.write("[dry-run] windows exec command\n")
            fh.write(" ".join(shlex.quote(p) for p in ssh_cmd) + "\n\n")
            fh.write(
                f"[dry-run] windows named-pipe runtime contract enabled: {run_named_pipe_contract}\n"
            )
            if run_named_pipe_contract:
                fh.write("[dry-run] windows named-pipe runtime contract command\n")
                fh.write(named_pipe_contract_command + "\n\n")
        return {
            "status": "dry-run",
            "mode": mode,
            "platform": "windows",
            "auth_mode": auth_mode,
            "primary_command": primary_command,
            "git_bash_enabled": git_bash_enabled,
            "git_bash_extra_check": git_bash_extra,
            "named_pipe_contract_enabled": run_named_pipe_contract,
            "named_pipe_contract_command": named_pipe_contract_command,
            "remote_run_dir": remote_run_dir,
            "log_path": str(run_log),
        }
    with password_auth_context(host_cfg) as (resolved_auth_mode, env, non_interactive):
        transfer = run_and_log(
            scp_cmd,
            run_log,
            env=env,
            non_interactive=non_interactive,
            auth_mode=resolved_auth_mode,
        )
        if transfer.returncode != 0:
            return {
                "status": "failed",
                "mode": mode,
                "platform": "windows",
                "auth_mode": resolved_auth_mode,
                "primary_command": primary_command,
                "git_bash_enabled": git_bash_enabled,
                "git_bash_extra_check": git_bash_extra,
                "named_pipe_contract_enabled": run_named_pipe_contract,
                "named_pipe_contract_command": named_pipe_contract_command,
                "remote_run_dir": remote_run_dir,
                "log_path": str(run_log),
                "error": "scp transfer failed",
            }
        execute = run_and_log(
            ssh_cmd,
            run_log,
            env=env,
            non_interactive=non_interactive,
            auth_mode=resolved_auth_mode,
        )
    status = "passed" if execute.returncode == 0 else "failed"
    result = {
        "status": status,
        "mode": mode,
        "platform": "windows",
        "auth_mode": auth_mode,
        "primary_command": primary_command,
        "git_bash_enabled": git_bash_enabled,
        "git_bash_extra_check": git_bash_extra,
        "named_pipe_contract_enabled": run_named_pipe_contract,
        "named_pipe_contract_command": named_pipe_contract_command,
        "remote_run_dir": remote_run_dir,
        "log_path": str(run_log),
    }
    if status == "failed":
        result["error"] = "remote command failed"
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Optional local cross-platform preflight using SSH/SCP."
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=DEFAULT_CONFIG_PATH,
        help="Path to local gitignored preflight TOML config.",
    )
    parser.add_argument(
        "--mode",
        choices=sorted(SUPPORTED_MODES),
        help="Override mode from config (`compile-only`, `unit`, `extended`).",
    )
    parser.add_argument(
        "--target",
        choices=["all", "linux", "windows"],
        default="all",
        help="Run subset of configured hosts.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Generate archives and commands without remote execution.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    run_id = now_stamp()
    summary: dict = {
        "run_id": run_id,
        "config_path": str(args.config),
        "started_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "dry_run": args.dry_run,
        "status": "unknown",
        "hosts": {},
    }

    run_base_dir = DEFAULT_RUNS_DIR
    config = load_config(args.config)
    if config is not None:
        run_base_dir = config["runs_dir"]
    run_root = run_base_dir / run_id
    run_root.mkdir(parents=True, exist_ok=True)
    summary_path = run_root / "summary.json"
    summary["runs_dir"] = str(run_base_dir)

    if config is None:
        summary["status"] = "skipped"
        summary["reason"] = "not-configured"
        summary["message"] = (
            f"local config not found at {args.config}; create one from "
            "scripts/testing/local-preflight.example.toml"
        )
        summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
        info(f"skipped: local config not found at {args.config}")
        info(
            "create your local config from scripts/testing/local-preflight.example.toml"
        )
        info(f"summary: {summary_path}")
        return 0

    mode = args.mode or config["mode"]
    if mode not in SUPPORTED_MODES:
        raise PreflightError(f"invalid mode: {mode}")
    summary["mode"] = mode

    linux_enabled = config["linux"]["enabled"] and args.target in {"all", "linux"}
    windows_enabled = config["windows"]["enabled"] and args.target in {"all", "windows"}
    if not linux_enabled and not windows_enabled:
        summary["status"] = "skipped"
        summary["reason"] = "no-enabled-hosts"
        summary["message"] = "no enabled hosts matched target filter"
        summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
        info("skipped: no enabled hosts matched current --target filter")
        info(f"summary: {summary_path}")
        return 0

    ensure_tools(["ssh", "scp"])
    excludes = config["excludes"]
    local_archives = {}
    if linux_enabled:
        linux_archive = run_root / "snapshot-linux.tar.gz"
        files = make_snapshot_archive(REPO_ROOT, linux_archive, "tar.gz", excludes)
        local_archives["linux"] = linux_archive
        info(f"built linux snapshot ({files} files): {linux_archive}")
    if windows_enabled:
        windows_archive = run_root / "snapshot-windows.zip"
        files = make_snapshot_archive(REPO_ROOT, windows_archive, "zip", excludes)
        local_archives["windows"] = windows_archive
        info(f"built windows snapshot ({files} files): {windows_archive}")

    failure_count = 0
    for host_name in ["linux", "windows"]:
        host_cfg = config[host_name]
        should_run = host_cfg["enabled"] and args.target in {"all", host_name}
        if not should_run:
            continue
        if not host_cfg["host"] or not host_cfg["user"]:
            summary["hosts"][host_name] = {
                "status": "failed",
                "error": "host/user is required for enabled host",
            }
            failure_count += 1
            continue
        log_path = run_root / f"{host_name}.log"
        info(f"running {host_name} preflight in `{mode}` mode")
        if host_name == "linux":
            result = run_linux_host(
                host_cfg,
                mode,
                run_id,
                local_archives["linux"],
                log_path,
                config["cleanup_remote_on_success"],
                args.dry_run,
            )
        else:
            result = run_windows_host(
                host_cfg,
                mode,
                run_id,
                local_archives["windows"],
                log_path,
                config["cleanup_remote_on_success"],
                args.dry_run,
            )
        summary["hosts"][host_name] = result
        if result["status"] == "failed":
            failure_count += 1
            info(
                f"{host_name} failed; check log {result['log_path']} and remote run dir {result['remote_run_dir']}"
            )
        elif result["status"] == "passed":
            info(f"{host_name} passed")
        else:
            info(f"{host_name} dry-run complete")

    if failure_count > 0:
        summary["status"] = "failed"
    else:
        if args.dry_run:
            summary["status"] = "dry-run"
        else:
            summary["status"] = "passed"
    summary["finished_at_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    info(f"summary: {summary_path}")
    return 1 if summary["status"] == "failed" else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except PreflightError as exc:
        print(f"[preflight] error: {exc}", file=sys.stderr)
        raise SystemExit(2)
