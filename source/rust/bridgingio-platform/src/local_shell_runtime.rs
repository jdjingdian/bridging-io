use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
#[cfg(unix)]
use std::os::raw::{c_char, c_int, c_void};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    decode_with_platform_defaults, CapabilityStatus, HostPlatform, InteractiveShellApiLayering,
    InteractiveShellDiagnostics, InteractiveShellSemantics, LocalShellOneShotOutput,
    LocalShellRuntimeError, LocalShellRuntimeWriteOutcome, ShellLaunchMode, ShellLaunchSpec,
};

const PIPE_DEGRADED_HINT: &str =
    "[runtime] degraded mode active: pipe backend in use; tty/prompt fidelity may be reduced";
const STATE_PROBE_DEGRADED_HINT: &str =
    "[runtime] state probe unavailable; using last-known cwd/env snapshot";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InteractiveShellBackendKind {
    Pipe,
    #[cfg(unix)]
    Pty,
}

impl InteractiveShellBackendKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            InteractiveShellBackendKind::Pipe => "pipe",
            #[cfg(unix)]
            InteractiveShellBackendKind::Pty => "pty",
        }
    }

    fn degraded_mode(self) -> bool {
        self == InteractiveShellBackendKind::Pipe
    }
}

#[derive(Clone, Debug)]
pub(crate) struct InteractiveShellRuntimeSnapshot {
    pub(crate) shell_id: String,
    pub(crate) target_kind: String,
    pub(crate) prompt: String,
    pub(crate) cwd: String,
    pub(crate) env: HashMap<String, String>,
    pub(crate) transcript: Vec<String>,
    pub(crate) interrupted: bool,
    pub(crate) closed: bool,
    pub(crate) running: bool,
    pub(crate) inflight_marker: Option<String>,
    pub(crate) backend: InteractiveShellBackendKind,
}

pub(crate) struct BaselineLocalShellRuntime {
    host_platform: HostPlatform,
    shell_label: &'static str,
    sessions: Mutex<HashMap<String, InteractiveShellRuntimeState>>,
}

impl BaselineLocalShellRuntime {
    pub(crate) fn new(host_platform: HostPlatform, shell_label: &'static str) -> Self {
        Self {
            host_platform,
            shell_label,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn status(&self) -> CapabilityStatus {
        match self.host_platform {
            HostPlatform::Unknown => CapabilityStatus::Unsupported,
            _ => CapabilityStatus::Ready,
        }
    }

    pub(crate) fn default_shell_label(&self) -> &'static str {
        self.shell_label
    }

    pub(crate) fn launch_spec(&self, mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec {
        match self.host_platform {
            HostPlatform::Windows => windows_launch_spec(mode, command),
            _ => unix_launch_spec(mode, command),
        }
    }

    pub(crate) fn run_one_shot(
        &self,
        command: &str,
    ) -> Result<LocalShellOneShotOutput, LocalShellRuntimeError> {
        let spec = self.launch_spec(ShellLaunchMode::OneShot, Some(command));
        let mut shell = Command::new(&spec.program);
        shell.args(spec.args);
        let output = shell.output().map_err(|err| LocalShellRuntimeError {
            message: format!("failed to run shell command: {err}"),
        })?;
        Ok(LocalShellOneShotOutput {
            stdout_lines: decode_lines(&output.stdout),
            stderr_lines: decode_lines(&output.stderr),
        })
    }

    pub(crate) fn open_interactive_shell(
        &self,
        shell_id: &str,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> Result<InteractiveShellRuntimeSnapshot, LocalShellRuntimeError> {
        let backend_process = spawn_interactive_process(self, launch_command)?;
        let mut transcript = vec![default_prompt_for(target_kind)];
        let mut state = InteractiveShellRuntimeState {
            shell_id: shell_id.to_string(),
            target_kind: target_kind.to_string(),
            prompt: default_prompt_for(target_kind),
            cwd: initial_cwd(),
            env: HashMap::new(),
            transcript: Vec::new(),
            interrupted: false,
            closed: false,
            running: false,
            inflight_marker: None,
            process: Some(backend_process),
        };

        if state.backend() == InteractiveShellBackendKind::Pipe {
            transcript.push(PIPE_DEGRADED_HINT.to_string());
        }

        match refresh_runtime_shell_state(self.shell_label, &mut state) {
            Ok(passthrough) if !passthrough.is_empty() => transcript.extend(passthrough),
            Ok(_) => {}
            Err(err) => {
                if launch_command.is_some() {
                    terminate_interactive_process(&mut state);
                    return Err(LocalShellRuntimeError {
                        message: format!(
                            "structured interactive launch readiness probe failed: {}",
                            err.message
                        ),
                    });
                }
                push_probe_degraded_note(&mut transcript);
            }
        }
        state.transcript = transcript;

        let snapshot = state.snapshot();
        let mut guard = self.sessions.lock().map_err(|_| LocalShellRuntimeError {
            message: "failed to lock local shell runtime sessions".to_string(),
        })?;
        guard.insert(shell_id.to_string(), state);
        Ok(snapshot)
    }

    pub(crate) fn write_interactive_shell(
        &self,
        shell_id: &str,
        command: &str,
        completion_marker: String,
    ) -> Result<LocalShellRuntimeWriteOutcome, LocalShellRuntimeError> {
        let command = command.trim();
        let mut guard = self.sessions.lock().map_err(|_| LocalShellRuntimeError {
            message: "failed to lock local shell runtime sessions".to_string(),
        })?;
        let state = guard
            .get_mut(shell_id)
            .ok_or_else(|| LocalShellRuntimeError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;
        if state.closed {
            return Err(LocalShellRuntimeError {
                message: "interactive shell already closed".to_string(),
            });
        }

        if let Some(mut process) = state.process.take() {
            let _ = harvest_interactive_output(
                &mut process,
                &mut state.inflight_marker,
                &mut state.running,
                Duration::from_millis(0),
            );
            state.process = Some(process);
        }
        if state.running {
            return Err(LocalShellRuntimeError {
                message: "interactive shell command still running; read or interrupt first"
                    .to_string(),
            });
        }

        state.transcript.push(format!("$ {command}"));
        let output_lines = if let Some(mut process) = state.process.take() {
            let run = (|| -> Result<Vec<String>, LocalShellRuntimeError> {
                state.inflight_marker = Some(completion_marker);
                state.running = true;
                execute_on_interactive_process(
                    self.shell_label,
                    &mut process,
                    command,
                    &state.inflight_marker,
                )?;
                Ok(harvest_interactive_output(
                    &mut process,
                    &mut state.inflight_marker,
                    &mut state.running,
                    Duration::from_millis(300),
                ))
            })();
            state.process = Some(process);
            run?
        } else {
            execute_without_live_process(self, state, command)?
        };

        for line in &output_lines {
            state.transcript.push(line.clone());
        }

        if !state.running {
            match refresh_runtime_shell_state(self.shell_label, state) {
                Ok(passthrough) => {
                    for line in passthrough {
                        state.transcript.push(line);
                    }
                }
                Err(_) => push_probe_degraded_note(&mut state.transcript),
            }
            state.transcript.push(state.prompt.clone());
        }

        Ok(LocalShellRuntimeWriteOutcome {
            output_lines,
            snapshot: snapshot_to_public(state.snapshot()),
        })
    }

    pub(crate) fn read_interactive_transcript(
        &self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, LocalShellRuntimeError> {
        let mut guard = self.sessions.lock().map_err(|_| LocalShellRuntimeError {
            message: "failed to lock local shell runtime sessions".to_string(),
        })?;
        let state = guard
            .get_mut(shell_id)
            .ok_or_else(|| LocalShellRuntimeError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;

        if let Some(mut process) = state.process.take() {
            let lines = harvest_interactive_output(
                &mut process,
                &mut state.inflight_marker,
                &mut state.running,
                Duration::from_millis(100),
            );
            for line in lines {
                state.transcript.push(line);
            }
            state.process = Some(process);
            if !state.running {
                match refresh_runtime_shell_state(self.shell_label, state) {
                    Ok(passthrough) => {
                        for line in passthrough {
                            state.transcript.push(line);
                        }
                    }
                    Err(_) => push_probe_degraded_note(&mut state.transcript),
                }
            }
        }

        Ok(state
            .transcript
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect())
    }

    pub(crate) fn interactive_shell_state(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellRuntimeSnapshot, LocalShellRuntimeError> {
        let mut guard = self.sessions.lock().map_err(|_| LocalShellRuntimeError {
            message: "failed to lock local shell runtime sessions".to_string(),
        })?;
        let state = guard
            .get_mut(shell_id)
            .ok_or_else(|| LocalShellRuntimeError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;

        if let Some(mut process) = state.process.take() {
            let lines = harvest_interactive_output(
                &mut process,
                &mut state.inflight_marker,
                &mut state.running,
                Duration::from_millis(0),
            );
            for line in lines {
                state.transcript.push(line);
            }
            state.process = Some(process);
            if !state.running {
                match refresh_runtime_shell_state(self.shell_label, state) {
                    Ok(passthrough) => {
                        for line in passthrough {
                            state.transcript.push(line);
                        }
                    }
                    Err(_) => push_probe_degraded_note(&mut state.transcript),
                }
            }
        }

        Ok(state.snapshot())
    }

    pub(crate) fn interrupt_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellRuntimeSnapshot, LocalShellRuntimeError> {
        let mut guard = self.sessions.lock().map_err(|_| LocalShellRuntimeError {
            message: "failed to lock local shell runtime sessions".to_string(),
        })?;
        let state = guard
            .get_mut(shell_id)
            .ok_or_else(|| LocalShellRuntimeError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;
        if state.closed {
            return Err(LocalShellRuntimeError {
                message: "interactive shell already closed".to_string(),
            });
        }

        if let Some(mut process) = state.process.take() {
            let interrupt = (|| -> Result<Vec<String>, LocalShellRuntimeError> {
                send_interrupt_signal(&mut process)?;
                Ok(harvest_interactive_output(
                    &mut process,
                    &mut state.inflight_marker,
                    &mut state.running,
                    Duration::from_millis(150),
                ))
            })();
            state.process = Some(process);
            let lines = interrupt?;
            for line in lines {
                state.transcript.push(line);
            }
        }

        state.interrupted = true;
        state.running = false;
        state.inflight_marker = None;
        state
            .transcript
            .push("[signal] interrupt requested".to_string());
        state.transcript.push(state.prompt.clone());
        Ok(state.snapshot())
    }

    pub(crate) fn close_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellRuntimeSnapshot, LocalShellRuntimeError> {
        let mut guard = self.sessions.lock().map_err(|_| LocalShellRuntimeError {
            message: "failed to lock local shell runtime sessions".to_string(),
        })?;
        let state = guard
            .get_mut(shell_id)
            .ok_or_else(|| LocalShellRuntimeError {
                message: format!("interactive shell not found: {shell_id}"),
            })?;

        if let Some(mut process) = state.process.take() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }

        if !state.closed {
            state.closed = true;
            state.running = false;
            state.inflight_marker = None;
            state.transcript.push("[shell] closed".to_string());
        }

        Ok(state.snapshot())
    }

    pub(crate) fn interactive_shell_diagnostics(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellDiagnostics, LocalShellRuntimeError> {
        let snapshot = self.interactive_shell_state(shell_id)?;
        Ok(build_diagnostics(self.host_platform, self.shell_label, &snapshot))
    }

    pub(crate) fn semantics(&self) -> InteractiveShellSemantics {
        semantics_for(self.host_platform, self.shell_label)
    }
}

struct InteractiveShellRuntimeState {
    shell_id: String,
    target_kind: String,
    prompt: String,
    cwd: String,
    env: HashMap<String, String>,
    transcript: Vec<String>,
    interrupted: bool,
    closed: bool,
    running: bool,
    inflight_marker: Option<String>,
    process: Option<InteractiveShellProcess>,
}

impl InteractiveShellRuntimeState {
    fn snapshot(&self) -> InteractiveShellRuntimeSnapshot {
        InteractiveShellRuntimeSnapshot {
            shell_id: self.shell_id.clone(),
            target_kind: self.target_kind.clone(),
            prompt: self.prompt.clone(),
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            transcript: self.transcript.clone(),
            interrupted: self.interrupted,
            closed: self.closed,
            running: self.running,
            inflight_marker: self.inflight_marker.clone(),
            backend: self.backend(),
        }
    }

    fn backend(&self) -> InteractiveShellBackendKind {
        self.process
            .as_ref()
            .map(|p| p.backend)
            .unwrap_or(InteractiveShellBackendKind::Pipe)
    }
}

struct InteractiveShellProcess {
    child: Child,
    writer: InteractiveShellWriter,
    backend: InteractiveShellBackendKind,
    output_lines: Arc<Mutex<Vec<String>>>,
    harvested_index: usize,
}

enum InteractiveShellWriter {
    Pipe(ChildStdin),
    #[cfg(unix)]
    Pty(File),
}

fn spawn_interactive_process(
    runtime: &BaselineLocalShellRuntime,
    launch_command: Option<&str>,
) -> Result<InteractiveShellProcess, LocalShellRuntimeError> {
    if launch_command.is_some() {
        return spawn_interactive_pipe_process(runtime, launch_command);
    }
    #[cfg(unix)]
    if let Ok(process) = spawn_interactive_pty_process(runtime, launch_command) {
        return Ok(process);
    }
    spawn_interactive_pipe_process(runtime, launch_command)
}

fn spawn_interactive_pipe_process(
    runtime: &BaselineLocalShellRuntime,
    launch_command: Option<&str>,
) -> Result<InteractiveShellProcess, LocalShellRuntimeError> {
    let mut command = command_from_spec(runtime.launch_spec(ShellLaunchMode::Interactive, launch_command));
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| LocalShellRuntimeError {
            message: format!("failed to spawn interactive shell: {err}"),
        })?;

    let stdin = child.stdin.take().ok_or_else(|| LocalShellRuntimeError {
        message: "failed to capture interactive shell stdin".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| LocalShellRuntimeError {
        message: "failed to capture interactive shell stdout".to_string(),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| LocalShellRuntimeError {
        message: "failed to capture interactive shell stderr".to_string(),
    })?;

    let output_lines = Arc::new(Mutex::new(Vec::new()));
    spawn_output_reader(stdout, Arc::clone(&output_lines), "");
    spawn_output_reader(stderr, Arc::clone(&output_lines), "stderr: ");

    Ok(InteractiveShellProcess {
        child,
        writer: InteractiveShellWriter::Pipe(stdin),
        backend: InteractiveShellBackendKind::Pipe,
        output_lines,
        harvested_index: 0,
    })
}

#[cfg(unix)]
fn spawn_interactive_pty_process(
    runtime: &BaselineLocalShellRuntime,
    launch_command: Option<&str>,
) -> Result<InteractiveShellProcess, LocalShellRuntimeError> {
    let (master_fd, slave_fd) = openpty_pair()?;
    let master = unsafe { File::from_raw_fd(master_fd) };
    let master_reader = master.try_clone().map_err(|err| LocalShellRuntimeError {
        message: format!("failed to clone PTY master fd: {err}"),
    })?;
    let slave = unsafe { File::from_raw_fd(slave_fd) };
    let child_stdin = slave.try_clone().map_err(|err| LocalShellRuntimeError {
        message: format!("failed to clone PTY slave fd for stdin: {err}"),
    })?;
    let child_stdout = slave.try_clone().map_err(|err| LocalShellRuntimeError {
        message: format!("failed to clone PTY slave fd for stdout: {err}"),
    })?;
    let child_stderr = slave.try_clone().map_err(|err| LocalShellRuntimeError {
        message: format!("failed to clone PTY slave fd for stderr: {err}"),
    })?;

    let mut command =
        command_from_spec(runtime.launch_spec(ShellLaunchMode::Interactive, launch_command));
    let child = command
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        .env("TERM", "xterm-256color")
        .spawn()
        .map_err(|err| LocalShellRuntimeError {
            message: format!("failed to spawn PTY interactive shell: {err}"),
        })?;
    drop(slave);

    let output_lines = Arc::new(Mutex::new(Vec::new()));
    spawn_output_reader(master_reader, Arc::clone(&output_lines), "");

    let mut process = InteractiveShellProcess {
        child,
        writer: InteractiveShellWriter::Pty(master),
        backend: InteractiveShellBackendKind::Pty,
        output_lines,
        harvested_index: 0,
    };
    initialize_pty_session(&mut process)?;
    Ok(process)
}

fn command_from_spec(spec: ShellLaunchSpec) -> Command {
    let mut command = Command::new(spec.program);
    command.args(spec.args);
    command
}

fn execute_on_interactive_process(
    shell_label: &str,
    process: &mut InteractiveShellProcess,
    command: &str,
    marker: &Option<String>,
) -> Result<(), LocalShellRuntimeError> {
    write_bytes_to_interactive_process(process, format!("{command}\n").as_bytes())?;
    if let Some(marker) = marker {
        let marker_command = marker_command_for_shell(shell_label, marker);
        write_bytes_to_interactive_process(process, marker_command.as_bytes())?;
        write_bytes_to_interactive_process(process, b"\n")?;
    }
    flush_interactive_process_writer(process)
}

fn marker_command_for_shell(shell_label: &str, marker: &str) -> String {
    if shell_label.eq_ignore_ascii_case("cmd") {
        format!("echo {marker}")
    } else {
        format!("printf '%s\\n' {}", shell_single_quote(marker))
    }
}

fn harvest_interactive_output(
    process: &mut InteractiveShellProcess,
    inflight_marker: &mut Option<String>,
    running: &mut bool,
    wait_timeout: Duration,
) -> Vec<String> {
    let start = Instant::now();
    let mut harvested = Vec::new();

    loop {
        let mut found_new = false;
        let mut found_marker = false;

        if let Ok(lines) = process.output_lines.lock() {
            while process.harvested_index < lines.len() {
                found_new = true;
                let line = lines[process.harvested_index].clone();
                process.harvested_index += 1;
                if let Some(marker) = inflight_marker.as_ref() {
                    if let Some(index) = line.find(marker) {
                        let before = line[..index].trim().to_string();
                        if !before.is_empty() {
                            harvested.push(before);
                        }
                        let after = line[index + marker.len()..].trim().to_string();
                        if !after.is_empty() {
                            harvested.push(after);
                        }
                        *running = false;
                        *inflight_marker = None;
                        found_marker = true;
                        continue;
                    }
                }
                harvested.push(line);
            }
        }

        if found_marker || start.elapsed() >= wait_timeout {
            break;
        }
        if !found_new {
            thread::sleep(Duration::from_millis(10));
        }
    }

    harvested
}

fn refresh_runtime_shell_state(
    shell_label: &str,
    state: &mut InteractiveShellRuntimeState,
) -> Result<Vec<String>, LocalShellRuntimeError> {
    let Some(mut process) = state.process.take() else {
        return Ok(Vec::new());
    };

    let probe = build_probe(shell_label);
    let probe_result = (|| -> Result<Vec<String>, LocalShellRuntimeError> {
        execute_on_interactive_process(
            shell_label,
            &mut process,
            &probe.command,
            &Some(probe.done_marker.clone()),
        )?;
        state.running = true;
        state.inflight_marker = Some(probe.done_marker.clone());
        Ok(harvest_interactive_output(
            &mut process,
            &mut state.inflight_marker,
            &mut state.running,
            Duration::from_millis(500),
        ))
    })();
    state.process = Some(process);
    let lines = match probe_result {
        Ok(lines) => lines,
        Err(err) => {
            state.running = false;
            state.inflight_marker = None;
            return Err(err);
        }
    };
    if state.running {
        state.running = false;
        state.inflight_marker = None;
        return Err(LocalShellRuntimeError {
            message: "interactive state probe timed out before completion marker".to_string(),
        });
    }

    let mut passthrough = Vec::new();
    let mut in_cwd = false;
    let mut in_env = false;
    let mut cwd = None;
    let mut env = HashMap::new();

    for raw in lines {
        let line = raw.trim().to_string();
        if line == probe.cwd_start {
            in_cwd = true;
            continue;
        }
        if line == probe.cwd_end {
            in_cwd = false;
            continue;
        }
        if line == probe.env_start {
            in_env = true;
            continue;
        }
        if line == probe.env_end {
            in_env = false;
            continue;
        }

        if in_cwd {
            if !line.is_empty() {
                cwd = Some(line);
            }
            continue;
        }

        if in_env {
            if let Some((key, value)) = line.split_once('=') {
                env.insert(key.trim().to_string(), value.to_string());
            }
            continue;
        }

        if !line.is_empty() {
            passthrough.push(line);
        }
    }

    if let Some(cwd) = cwd {
        state.cwd = cwd;
    }
    if !env.is_empty() {
        state.env = env;
    }

    Ok(passthrough)
}

fn snapshot_to_public(snapshot: InteractiveShellRuntimeSnapshot) -> crate::LocalShellRuntimeSnapshot {
    crate::LocalShellRuntimeSnapshot {
        shell_id: snapshot.shell_id,
        target_kind: snapshot.target_kind,
        prompt: snapshot.prompt,
        cwd: snapshot.cwd,
        env: snapshot.env,
        transcript: snapshot.transcript,
        interrupted: snapshot.interrupted,
        closed: snapshot.closed,
        running: snapshot.running,
        completion_marker: snapshot.inflight_marker,
        backend: snapshot.backend.as_str().to_string(),
        degraded_mode: snapshot.backend.degraded_mode(),
    }
}

fn push_probe_degraded_note(transcript: &mut Vec<String>) {
    if transcript
        .iter()
        .any(|line| line == STATE_PROBE_DEGRADED_HINT)
    {
        return;
    }
    transcript.push(STATE_PROBE_DEGRADED_HINT.to_string());
}

struct RuntimeStateProbe {
    command: String,
    cwd_start: String,
    cwd_end: String,
    env_start: String,
    env_end: String,
    done_marker: String,
}

fn build_probe(shell_label: &str) -> RuntimeStateProbe {
    let cwd_start = "__BRIDGINGIO_STATE_CWD_BEGIN__".to_string();
    let cwd_end = "__BRIDGINGIO_STATE_CWD_END__".to_string();
    let env_start = "__BRIDGINGIO_STATE_ENV_BEGIN__".to_string();
    let env_end = "__BRIDGINGIO_STATE_ENV_END__".to_string();
    let done_marker = "__BRIDGINGIO_STATE_DONE__".to_string();

    let command = if shell_label.eq_ignore_ascii_case("cmd") {
        format!(
            "echo {cwd_start} & cd & echo {cwd_end} & echo {env_start} & set & echo {env_end}"
        )
    } else {
        format!(
            "printf '%s\\n' {} ; pwd ; printf '%s\\n' {} ; printf '%s\\n' {} ; env ; printf '%s\\n' {}",
            shell_single_quote(&cwd_start),
            shell_single_quote(&cwd_end),
            shell_single_quote(&env_start),
            shell_single_quote(&env_end)
        )
    };

    RuntimeStateProbe {
        command,
        cwd_start,
        cwd_end,
        env_start,
        env_end,
        done_marker,
    }
}

fn execute_without_live_process(
    runtime: &BaselineLocalShellRuntime,
    state: &mut InteractiveShellRuntimeState,
    command: &str,
) -> Result<Vec<String>, LocalShellRuntimeError> {
    if command.is_empty() {
        return Ok(Vec::new());
    }

    let script = if runtime.shell_label.eq_ignore_ascii_case("cmd") {
        let mut script = String::new();
        if !state.cwd.is_empty() {
            script.push_str("cd /d ");
            script.push_str(&shell_double_quote(&state.cwd));
            script.push('\n');
        }
        for (key, value) in &state.env {
            script.push_str("set ");
            script.push_str(key);
            script.push('=');
            script.push_str(value);
            script.push('\n');
        }
        script.push_str(command);
        script
    } else {
        let mut script = String::new();
        script.push_str("set -e\n");
        script.push_str(&format!("cd {}\n", shell_single_quote(&state.cwd)));
        for (key, value) in &state.env {
            script.push_str(&format!("export {}={}\n", key, shell_single_quote(value)));
        }
        script.push_str(command);
        script
    };

    let output = runtime.run_one_shot(&script)?;
    let mut lines = output.stdout_lines;
    lines.extend(
        output
            .stderr_lines
            .into_iter()
            .map(|line| format!("stderr: {line}")),
    );
    Ok(lines)
}

fn write_bytes_to_interactive_process(
    process: &mut InteractiveShellProcess,
    bytes: &[u8],
) -> Result<(), LocalShellRuntimeError> {
    match &mut process.writer {
        InteractiveShellWriter::Pipe(stdin) => stdin.write_all(bytes).map_err(|err| {
            LocalShellRuntimeError {
                message: format!("failed to write command to interactive shell: {err}"),
            }
        }),
        #[cfg(unix)]
        InteractiveShellWriter::Pty(master) => master.write_all(bytes).map_err(|err| {
            LocalShellRuntimeError {
                message: format!("failed to write command to PTY interactive shell: {err}"),
            }
        }),
    }
}

fn flush_interactive_process_writer(
    process: &mut InteractiveShellProcess,
) -> Result<(), LocalShellRuntimeError> {
    match &mut process.writer {
        InteractiveShellWriter::Pipe(stdin) => stdin.flush().map_err(|err| LocalShellRuntimeError {
            message: format!("failed to flush interactive shell stdin: {err}"),
        }),
        #[cfg(unix)]
        InteractiveShellWriter::Pty(master) => master.flush().map_err(|err| {
            LocalShellRuntimeError {
                message: format!("failed to flush PTY interactive shell writer: {err}"),
            }
        }),
    }
}

fn spawn_output_reader<R: Read + Send + 'static>(
    mut reader: R,
    output_lines: Arc<Mutex<Vec<String>>>,
    prefix: &'static str,
) {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut pending = String::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let decoded = decode_with_platform_defaults(&buffer[..count]);
                    pending.push_str(&decoded.text);
                    let has_trailing_newline = pending.ends_with('\n');
                    let mut parts = pending
                        .split('\n')
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    if has_trailing_newline {
                        pending.clear();
                    } else {
                        pending = parts.pop().unwrap_or_default();
                    }

                    for part in parts {
                        push_output_line(&output_lines, prefix, part);
                    }

                    if pending.len() > 1024 {
                        let partial = pending.clone();
                        pending.clear();
                        push_output_line(&output_lines, prefix, partial);
                    }
                }
                Err(_) => break,
            }
        }

        if !pending.is_empty() {
            push_output_line(&output_lines, prefix, pending);
        }
    });
}

fn push_output_line(output_lines: &Arc<Mutex<Vec<String>>>, prefix: &str, line: String) {
    if let Ok(mut lines) = output_lines.lock() {
        if prefix.is_empty() {
            lines.push(line);
        } else {
            lines.push(format!("{prefix}{line}"));
        }
    }
}

#[cfg(unix)]
fn initialize_pty_session(process: &mut InteractiveShellProcess) -> Result<(), LocalShellRuntimeError> {
    write_bytes_to_interactive_process(process, b"stty -echo 2>/dev/null || true\n")?;
    flush_interactive_process_writer(process)?;
    write_bytes_to_interactive_process(process, b"export PS1=''\n")?;
    flush_interactive_process_writer(process)?;
    thread::sleep(Duration::from_millis(40));
    if let Ok(lines) = process.output_lines.lock() {
        process.harvested_index = lines.len();
    }
    Ok(())
}

#[cfg(unix)]
fn openpty_pair() -> Result<(RawFd, RawFd), LocalShellRuntimeError> {
    let mut master: c_int = -1;
    let mut slave: c_int = -1;
    let rc = unsafe {
        openpty(
            &mut master as *mut c_int,
            &mut slave as *mut c_int,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if rc != 0 {
        return Err(LocalShellRuntimeError {
            message: format!(
                "failed to allocate PTY pair for interactive shell: {}",
                std::io::Error::last_os_error()
            ),
        });
    }
    Ok((master as RawFd, slave as RawFd))
}

#[cfg(unix)]
#[cfg_attr(target_os = "linux", link(name = "util"))]
extern "C" {
    fn openpty(
        amaster: *mut c_int,
        aslave: *mut c_int,
        name: *mut c_char,
        termp: *const c_void,
        winp: *const c_void,
    ) -> c_int;
}

#[cfg(unix)]
fn send_interrupt_signal(process: &mut InteractiveShellProcess) -> Result<(), LocalShellRuntimeError> {
    if process.backend == InteractiveShellBackendKind::Pty {
        write_bytes_to_interactive_process(process, &[0x03])?;
        flush_interactive_process_writer(process)?;
        return Ok(());
    }

    let status = Command::new("kill")
        .arg("-INT")
        .arg(process.child.id().to_string())
        .status()
        .map_err(|err| LocalShellRuntimeError {
            message: format!("failed to send interrupt signal: {err}"),
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(LocalShellRuntimeError {
            message: "interrupt signal command failed".to_string(),
        })
    }
}

#[cfg(not(unix))]
fn send_interrupt_signal(process: &mut InteractiveShellProcess) -> Result<(), LocalShellRuntimeError> {
    process.child.kill().map_err(|err| LocalShellRuntimeError {
        message: format!("failed to send interrupt signal: {err}"),
    })
}

fn unix_launch_spec(mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec {
    match mode {
        ShellLaunchMode::OneShot => ShellLaunchSpec {
            program: "/bin/sh".to_string(),
            args: vec!["-lc".to_string(), command.unwrap_or_default().to_string()],
        },
        ShellLaunchMode::Interactive => {
            if let Some(command) = command {
                ShellLaunchSpec {
                    program: "/bin/sh".to_string(),
                    args: vec!["-lc".to_string(), command.to_string()],
                }
            } else {
                ShellLaunchSpec {
                    program: "/bin/sh".to_string(),
                    args: vec!["-s".to_string()],
                }
            }
        }
    }
}

fn windows_launch_spec(mode: ShellLaunchMode, command: Option<&str>) -> ShellLaunchSpec {
    match mode {
        ShellLaunchMode::OneShot => ShellLaunchSpec {
            program: "cmd".to_string(),
            args: vec!["/C".to_string(), command.unwrap_or_default().to_string()],
        },
        ShellLaunchMode::Interactive => {
            if let Some(command) = command {
                ShellLaunchSpec {
                    program: "cmd".to_string(),
                    args: vec!["/C".to_string(), command.to_string()],
                }
            } else {
                ShellLaunchSpec {
                    program: "cmd".to_string(),
                    args: vec!["/Q".to_string(), "/K".to_string()],
                }
            }
        }
    }
}

fn default_prompt_for(target_kind: &str) -> String {
    match target_kind {
        "adb" => "emulator:/ $".to_string(),
        "ssh" => "ssh:$".to_string(),
        _ => "shell:$".to_string(),
    }
}

fn initial_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/".to_string())
}

fn decode_lines(bytes: &[u8]) -> Vec<String> {
    decode_with_platform_defaults(bytes)
        .text
        .lines()
        .map(ToString::to_string)
        .collect()
}

fn semantics_for(host_platform: HostPlatform, shell_label: &str) -> InteractiveShellSemantics {
    let shell = shell_label.to_ascii_lowercase();
    match host_platform {
        HostPlatform::Windows => InteractiveShellSemantics {
            host_shell_default: "cmd",
            cwd_semantics: "cwd is refreshed from runtime probe output",
            env_semantics: "environment is refreshed from runtime probe output",
            interrupt_semantics:
                "interrupt is best-effort: process kill fallback in pipe mode, ctrl-break equivalent reserved for future native backend",
            close_semantics: "close requests process termination and transcript finalization",
            prompt_semantics: "prompt is maintained by runtime channel state model",
            degraded_mode_semantics:
                "pipe backend is allowed degraded mode; running/closed/interrupted/completion marker states must remain visible",
            supports_tty_backend: false,
            supports_pipe_backend: true,
            default_completion_marker: "runtime-provided completion marker",
            host_shell_label: shell,
        },
        _ => InteractiveShellSemantics {
            host_shell_default: "sh",
            cwd_semantics: "cwd is refreshed from runtime probe output",
            env_semantics: "environment is refreshed from runtime probe output",
            interrupt_semantics:
                "interrupt sends ctrl-c on pty and SIGINT on pipe backend",
            close_semantics: "close requests process termination and transcript finalization",
            prompt_semantics: "prompt is maintained by runtime channel state model",
            degraded_mode_semantics:
                "pipe backend is allowed degraded mode; running/closed/interrupted/completion marker states must remain visible",
            supports_tty_backend: true,
            supports_pipe_backend: true,
            default_completion_marker: "runtime-provided completion marker",
            host_shell_label: shell,
        },
    }
}

fn build_diagnostics(
    host_platform: HostPlatform,
    shell_label: &str,
    snapshot: &InteractiveShellRuntimeSnapshot,
) -> InteractiveShellDiagnostics {
    InteractiveShellDiagnostics {
        shell_id: snapshot.shell_id.clone(),
        backend: snapshot.backend.as_str().to_string(),
        degraded_mode: snapshot.backend.degraded_mode(),
        running: snapshot.running,
        closed: snapshot.closed,
        interrupted: snapshot.interrupted,
        completion_marker: snapshot.inflight_marker.clone(),
        required_visible_states: vec![
            "running".to_string(),
            "closed".to_string(),
            "interrupted".to_string(),
            "completion_marker".to_string(),
            "backend".to_string(),
            "cwd".to_string(),
            "prompt".to_string(),
        ],
        allowed_degraded_capabilities: vec![
            "tty_program_fidelity".to_string(),
            "prompt_echo_stability".to_string(),
            "signal_semantics_on_pipe".to_string(),
        ],
        semantics: semantics_for(host_platform, shell_label),
        api_layering: InteractiveShellApiLayering {
            startup: "open_interactive_shell",
            write: "write_interactive_shell",
            read: "read_interactive_transcript",
            state_query: "interactive_shell_state",
            interrupt: "interrupt_interactive_shell",
            close: "close_interactive_shell",
            diagnostics: "interactive_shell_diagnostics",
        },
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn terminate_interactive_process(state: &mut InteractiveShellRuntimeState) {
    if let Some(mut process) = state.process.take() {
        let _ = process.child.kill();
        let _ = process.child.wait();
    }
}

fn shell_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
