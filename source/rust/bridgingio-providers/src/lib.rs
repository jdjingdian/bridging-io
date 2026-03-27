use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
#[cfg(unix)]
use std::os::raw::{c_char, c_int, c_void};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use bridgingio_artifacts::{ArtifactRefineMode, InMemoryArtifactStore};
use bridgingio_domain::{ArtifactRecord, CapabilitySummary, PolicyProfile};
use bridgingio_policy::{evaluate, OperationKind, PolicyDecision};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub message: String,
}

pub fn terminal_provider_capability() -> CapabilitySummary {
    CapabilitySummary {
        id: "terminal.exec".into(),
        label: "terminal command execution".into(),
        supports_streaming: true,
        supports_file_transfer: false,
        requires_approval: true,
        typed_entrypoints: vec![
            "terminal.exec".into(),
            "artifacts.read".into(),
            "artifacts.refine".into(),
        ],
        raw_fallback: true,
    }
}

#[derive(Default)]
pub struct TerminalProvider {
    pub artifacts: InMemoryArtifactStore,
    interactive_shells: HashMap<String, InteractiveShellState>,
    interactive_processes: HashMap<String, InteractiveShellProcess>,
    next_shell_seq: u64,
    next_command_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellState {
    pub shell_id: String,
    pub logical_session_id: String,
    pub channel_id: String,
    pub transport_session_id: Option<String>,
    pub target_kind: String,
    pub prompt: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub transcript: Vec<String>,
    pub interrupted: bool,
    pub closed: bool,
    pub running: bool,
    pub inflight_marker: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveShellWriteResult {
    pub shell_id: String,
    pub output: String,
    pub prompt: String,
    pub cwd: String,
    pub artifact_id: String,
    pub running: bool,
}

struct InteractiveShellProcess {
    child: Child,
    writer: InteractiveShellWriter,
    backend: InteractiveShellBackend,
    output_lines: Arc<Mutex<Vec<String>>>,
    harvested_index: usize,
}

enum InteractiveShellWriter {
    Pipe(ChildStdin),
    #[cfg(unix)]
    Pty(File),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractiveShellBackend {
    Pipe,
    #[cfg(unix)]
    Pty,
}

impl TerminalProvider {
    pub fn exec_local(
        &mut self,
        logical_session_id: &str,
        channel_id: Option<&str>,
        transport_session_id: Option<&str>,
        command: &str,
        artifact_id: &str,
        policy: &PolicyProfile,
    ) -> Result<ArtifactRecord, ProviderError> {
        if matches!(
            evaluate(policy, OperationKind::Write),
            PolicyDecision::RequireApproval
        ) && command.contains("rm ")
        {
            return Err(ProviderError {
                message: "command requires approval".into(),
            });
        }

        let output = run_shell_command(command)?;

        let created = self.artifacts.create_raw(
            artifact_id.to_string(),
            logical_session_id.to_string(),
            channel_id.map(|v| v.to_string()),
            transport_session_id.map(|v| v.to_string()),
            command.to_string(),
            "terminal command output",
            SystemTime::now(),
        );

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            self.artifacts.append_chunk(&created.id, line.to_string());
        }
        for line in String::from_utf8_lossy(&output.stderr).lines() {
            self.artifacts
                .append_chunk(&created.id, format!("stderr: {line}"));
        }

        Ok(created)
    }

    pub fn refine_keyword(
        &mut self,
        source_artifact_id: &str,
        derived_artifact_id: &str,
        keyword: &str,
    ) -> Option<ArtifactRecord> {
        self.artifacts.derive_with_keyword(
            derived_artifact_id.to_string(),
            source_artifact_id.to_string(),
            keyword.to_string(),
            SystemTime::now(),
        )
    }

    pub fn refine_with_filter(
        &mut self,
        source_artifact_id: &str,
        derived_artifact_id: &str,
        pattern: &str,
        mode_label: &str,
        ignore_case: bool,
    ) -> Result<ArtifactRecord, ProviderError> {
        let mode = parse_artifact_refine_mode(mode_label)?;
        let record = self
            .artifacts
            .derive_with_pattern(
                derived_artifact_id.to_string(),
                source_artifact_id.to_string(),
                pattern.to_string(),
                mode,
                ignore_case,
                SystemTime::now(),
            )
            .map_err(|message| ProviderError { message })?
            .ok_or_else(|| ProviderError {
                message: "source artifact not found".into(),
            })?;
        Ok(record)
    }

    pub fn open_interactive_shell(
        &mut self,
        logical_session_id: &str,
        channel_id: &str,
        transport_session_id: Option<&str>,
        target_kind: &str,
    ) -> InteractiveShellState {
        self.open_interactive_shell_with_command(
            logical_session_id,
            channel_id,
            transport_session_id,
            target_kind,
            None,
        )
    }

    pub fn open_interactive_shell_with_command(
        &mut self,
        logical_session_id: &str,
        channel_id: &str,
        transport_session_id: Option<&str>,
        target_kind: &str,
        launch_command: Option<&str>,
    ) -> InteractiveShellState {
        self.next_shell_seq += 1;
        let shell_id = format!("shell-{:06}", self.next_shell_seq);
        let prompt = default_prompt_for(target_kind);
        let mut transcript = Vec::new();
        transcript.push(prompt.clone());
        let mut state = InteractiveShellState {
            shell_id: shell_id.clone(),
            logical_session_id: logical_session_id.to_string(),
            channel_id: channel_id.to_string(),
            transport_session_id: transport_session_id.map(ToString::to_string),
            target_kind: target_kind.to_string(),
            prompt,
            cwd: "/".to_string(),
            env: HashMap::new(),
            transcript,
            interrupted: false,
            closed: false,
            running: false,
            inflight_marker: None,
        };

        if let Ok(process) = spawn_interactive_process(launch_command) {
            if process.backend == InteractiveShellBackend::Pipe {
                state
                    .transcript
                    .push("[runtime] PTY unavailable, falling back to pipe backend".into());
            }
            self.interactive_processes.insert(shell_id.clone(), process);
        }
        self.interactive_shells
            .insert(shell_id.clone(), state.clone());
        state
    }

    pub fn write_interactive_shell(
        &mut self,
        shell_id: &str,
        command: &str,
        artifact_id: &str,
        policy: &PolicyProfile,
    ) -> Result<InteractiveShellWriteResult, ProviderError> {
        let Some(mut state) = self.interactive_shells.remove(shell_id) else {
            return Err(ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            });
        };
        if state.closed {
            self.interactive_shells
                .insert(shell_id.to_string(), state.clone());
            return Err(ProviderError {
                message: "interactive shell already closed".into(),
            });
        }

        let command = command.trim();
        if let Some(process) = self.interactive_processes.get_mut(shell_id) {
            let _ = harvest_interactive_output(process, &mut state, Duration::from_millis(0));
        }
        if state.running {
            self.interactive_shells
                .insert(shell_id.to_string(), state.clone());
            return Err(ProviderError {
                message: "interactive shell command still running; read or interrupt first".into(),
            });
        }

        state.transcript.push(format!("$ {command}"));
        let output_lines = if let Some(process) = self.interactive_processes.get_mut(shell_id) {
            self.next_command_seq += 1;
            state.inflight_marker = Some(format!(
                "__BRIDGINGIO_DONE_{}_{}__",
                shell_id, self.next_command_seq
            ));
            state.running = true;
            execute_on_interactive_process(process, command, &state.inflight_marker)?;
            harvest_interactive_output(process, &mut state, Duration::from_millis(300))
        } else {
            let output = execute_with_shell_state(&mut state, command, policy)?;
            output.lines().map(ToString::to_string).collect::<Vec<_>>()
        };

        for line in &output_lines {
            state.transcript.push(line.to_string());
        }
        if !state.running {
            state.transcript.push(state.prompt.clone());
        }

        let created = self.artifacts.create_raw(
            artifact_id.to_string(),
            state.logical_session_id.clone(),
            Some(state.channel_id.clone()),
            state.transport_session_id.clone(),
            command.to_string(),
            "interactive shell output",
            SystemTime::now(),
        );
        for line in &output_lines {
            self.artifacts.append_chunk(&created.id, line.to_string());
        }

        apply_shell_state_mutation(
            &mut state,
            command,
            output_lines.first().map(String::as_str),
        );
        let output = output_lines.join("\n");
        let running = state.running;
        let prompt = state.prompt.clone();
        let cwd = state.cwd.clone();
        self.interactive_shells
            .insert(shell_id.to_string(), state.clone());

        Ok(InteractiveShellWriteResult {
            shell_id: shell_id.to_string(),
            output,
            prompt,
            cwd,
            artifact_id: created.id,
            running,
        })
    }

    pub fn read_interactive_transcript(
        &mut self,
        shell_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, ProviderError> {
        let Some(mut state) = self.interactive_shells.remove(shell_id) else {
            return Err(ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            });
        };
        if let Some(process) = self.interactive_processes.get_mut(shell_id) {
            let lines = harvest_interactive_output(process, &mut state, Duration::from_millis(100));
            for line in lines {
                state.transcript.push(line);
            }
        }
        let result = state
            .transcript
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        self.interactive_shells
            .insert(shell_id.to_string(), state.clone());
        Ok(result)
    }

    pub fn get_interactive_shell(
        &self,
        shell_id: &str,
    ) -> Result<InteractiveShellState, ProviderError> {
        self.interactive_shells
            .get(shell_id)
            .cloned()
            .ok_or_else(|| ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            })
    }

    pub fn interrupt_interactive_shell(&mut self, shell_id: &str) -> Result<(), ProviderError> {
        let Some(mut state) = self.interactive_shells.remove(shell_id) else {
            return Err(ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            });
        };
        if state.closed {
            self.interactive_shells
                .insert(shell_id.to_string(), state.clone());
            return Err(ProviderError {
                message: "interactive shell already closed".into(),
            });
        }
        if let Some(process) = self.interactive_processes.get_mut(shell_id) {
            send_interrupt_signal(process)?;
            let lines = harvest_interactive_output(process, &mut state, Duration::from_millis(150));
            for line in lines {
                state.transcript.push(line);
            }
        }
        state.interrupted = true;
        state.running = false;
        state.inflight_marker = None;
        state.transcript.push("[signal] interrupt requested".into());
        state.transcript.push(state.prompt.clone());
        self.interactive_shells
            .insert(shell_id.to_string(), state.clone());
        Ok(())
    }

    pub fn close_interactive_shell(&mut self, shell_id: &str) -> Result<(), ProviderError> {
        let Some(mut state) = self.interactive_shells.remove(shell_id) else {
            return Err(ProviderError {
                message: format!("interactive shell not found: {shell_id}"),
            });
        };
        if let Some(mut process) = self.interactive_processes.remove(shell_id) {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
        if !state.closed {
            state.closed = true;
            state.running = false;
            state.inflight_marker = None;
            state.transcript.push("[shell] closed".into());
        }
        self.interactive_shells
            .insert(shell_id.to_string(), state.clone());
        Ok(())
    }
}

fn default_prompt_for(target_kind: &str) -> String {
    match target_kind {
        "adb" => "emulator:/ $".to_string(),
        "ssh" => "ssh:$".to_string(),
        _ => "shell:$".to_string(),
    }
}

fn spawn_interactive_process(
    launch_command: Option<&str>,
) -> Result<InteractiveShellProcess, ProviderError> {
    if launch_command.is_some() {
        return spawn_interactive_pipe_process(launch_command);
    }
    #[cfg(unix)]
    if let Ok(process) = spawn_interactive_pty_process(launch_command) {
        return Ok(process);
    }
    spawn_interactive_pipe_process(launch_command)
}

fn spawn_interactive_pipe_process(
    launch_command: Option<&str>,
) -> Result<InteractiveShellProcess, ProviderError> {
    let mut command = platform_interactive_shell_command(launch_command);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| ProviderError {
            message: format!("failed to spawn interactive shell: {err}"),
        })?;
    let stdin = child.stdin.take().ok_or_else(|| ProviderError {
        message: "failed to capture interactive shell stdin".into(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| ProviderError {
        message: "failed to capture interactive shell stdout".into(),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| ProviderError {
        message: "failed to capture interactive shell stderr".into(),
    })?;

    let output_lines = Arc::new(Mutex::new(Vec::<String>::new()));
    spawn_output_reader(stdout, Arc::clone(&output_lines), "");
    spawn_output_reader(stderr, Arc::clone(&output_lines), "stderr: ");

    Ok(InteractiveShellProcess {
        child,
        writer: InteractiveShellWriter::Pipe(stdin),
        backend: InteractiveShellBackend::Pipe,
        output_lines,
        harvested_index: 0,
    })
}

#[cfg(unix)]
fn spawn_interactive_pty_process(
    launch_command: Option<&str>,
) -> Result<InteractiveShellProcess, ProviderError> {
    let (master_fd, slave_fd) = openpty_pair()?;
    let master = unsafe { File::from_raw_fd(master_fd) };
    let master_reader = master.try_clone().map_err(|err| ProviderError {
        message: format!("failed to clone PTY master fd: {err}"),
    })?;
    let slave = unsafe { File::from_raw_fd(slave_fd) };
    let child_stdin = slave.try_clone().map_err(|err| ProviderError {
        message: format!("failed to clone PTY slave fd for stdin: {err}"),
    })?;
    let child_stdout = slave.try_clone().map_err(|err| ProviderError {
        message: format!("failed to clone PTY slave fd for stdout: {err}"),
    })?;
    let child_stderr = slave.try_clone().map_err(|err| ProviderError {
        message: format!("failed to clone PTY slave fd for stderr: {err}"),
    })?;

    let mut command = Command::new("/bin/sh");
    if let Some(launch_command) = launch_command {
        command.arg("-lc").arg(launch_command);
    } else {
        command.arg("-s");
    }
    let child = command
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        .env("TERM", "xterm-256color")
        .spawn()
        .map_err(|err| ProviderError {
            message: format!("failed to spawn PTY interactive shell: {err}"),
        })?;
    drop(slave);

    let output_lines = Arc::new(Mutex::new(Vec::<String>::new()));
    spawn_output_reader(master_reader, Arc::clone(&output_lines), "");

    let mut process = InteractiveShellProcess {
        child,
        writer: InteractiveShellWriter::Pty(master),
        backend: InteractiveShellBackend::Pty,
        output_lines,
        harvested_index: 0,
    };
    initialize_pty_session(&mut process)?;
    Ok(process)
}

fn execute_on_interactive_process(
    process: &mut InteractiveShellProcess,
    command: &str,
    marker: &Option<String>,
) -> Result<(), ProviderError> {
    write_bytes_to_interactive_process(process, format!("{command}\n").as_bytes())?;
    if let Some(marker) = marker {
        let marker_command = marker_command_for_shell(marker);
        write_bytes_to_interactive_process(process, marker_command.as_bytes())?;
        write_bytes_to_interactive_process(process, b"\n")?;
    }
    flush_interactive_process_writer(process)?;
    Ok(())
}

fn marker_command_for_shell(marker: &str) -> String {
    #[cfg(windows)]
    {
        format!("echo {marker}")
    }
    #[cfg(not(windows))]
    {
        format!("printf '%s\\n' {}", shell_single_quote(marker))
    }
}

fn harvest_interactive_output(
    process: &mut InteractiveShellProcess,
    state: &mut InteractiveShellState,
    wait_timeout: Duration,
) -> Vec<String> {
    let start = Instant::now();
    let mut harvested = Vec::<String>::new();
    loop {
        let mut found_new = false;
        let mut found_marker = false;
        if let Ok(lines) = process.output_lines.lock() {
            while process.harvested_index < lines.len() {
                found_new = true;
                let line = lines[process.harvested_index].clone();
                process.harvested_index += 1;
                if let Some(marker) = state.inflight_marker.as_ref() {
                    if let Some(index) = line.find(marker) {
                        let before = line[..index].trim().to_string();
                        if !before.is_empty() {
                            harvested.push(before);
                        }
                        let after = line[index + marker.len()..].trim().to_string();
                        if !after.is_empty() {
                            harvested.push(after);
                        }
                        state.running = false;
                        state.inflight_marker = None;
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

#[cfg(unix)]
fn send_interrupt_signal(process: &mut InteractiveShellProcess) -> Result<(), ProviderError> {
    if process.backend == InteractiveShellBackend::Pty {
        write_bytes_to_interactive_process(process, &[0x03])?;
        flush_interactive_process_writer(process)?;
        return Ok(());
    }
    send_posix_signal(process.child.id(), "-INT")
}

#[cfg(unix)]
fn send_posix_signal(pid: u32, signal: &str) -> Result<(), ProviderError> {
    let status = Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
        .map_err(|err| ProviderError {
            message: format!("failed to send interrupt signal: {err}"),
        })?;
    if !status.success() {
        return Err(ProviderError {
            message: "interrupt signal command failed".into(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn send_interrupt_signal(_process: &mut InteractiveShellProcess) -> Result<(), ProviderError> {
    Ok(())
}

fn write_bytes_to_interactive_process(
    process: &mut InteractiveShellProcess,
    bytes: &[u8],
) -> Result<(), ProviderError> {
    match &mut process.writer {
        InteractiveShellWriter::Pipe(stdin) => {
            stdin.write_all(bytes).map_err(|err| ProviderError {
                message: format!("failed to write command to interactive shell: {err}"),
            })
        }
        #[cfg(unix)]
        InteractiveShellWriter::Pty(master) => {
            master.write_all(bytes).map_err(|err| ProviderError {
                message: format!("failed to write command to PTY interactive shell: {err}"),
            })
        }
    }
}

fn flush_interactive_process_writer(
    process: &mut InteractiveShellProcess,
) -> Result<(), ProviderError> {
    match &mut process.writer {
        InteractiveShellWriter::Pipe(stdin) => stdin.flush().map_err(|err| ProviderError {
            message: format!("failed to flush interactive shell stdin: {err}"),
        }),
        #[cfg(unix)]
        InteractiveShellWriter::Pty(master) => master.flush().map_err(|err| ProviderError {
            message: format!("failed to flush PTY interactive shell writer: {err}"),
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
                    let chunk = String::from_utf8_lossy(&buffer[..count])
                        .replace("\r\n", "\n")
                        .replace('\r', "\n");
                    pending.push_str(&chunk);
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
fn initialize_pty_session(process: &mut InteractiveShellProcess) -> Result<(), ProviderError> {
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
fn openpty_pair() -> Result<(RawFd, RawFd), ProviderError> {
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
        return Err(ProviderError {
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

fn apply_shell_state_mutation(
    state: &mut InteractiveShellState,
    command: &str,
    first_output_line: Option<&str>,
) {
    if command == "pwd" {
        if let Some(output) = first_output_line {
            if !output.is_empty() {
                state.cwd = output.to_string();
            }
        }
        return;
    }
    if let Some(rest) = command.strip_prefix("cd ") {
        let new_dir = rest.trim();
        if new_dir.starts_with('/') {
            state.cwd = new_dir.to_string();
        } else if new_dir == "." || new_dir.is_empty() {
        } else if new_dir == ".." {
            if let Some((parent, _)) = state.cwd.rsplit_once('/') {
                state.cwd = if parent.is_empty() {
                    "/".to_string()
                } else {
                    parent.to_string()
                };
            }
        } else if state.cwd == "/" {
            state.cwd = format!("/{new_dir}");
        } else {
            state.cwd = format!("{}/{}", state.cwd, new_dir);
        }
        return;
    }
    if let Some(rest) = command.strip_prefix("export ") {
        if let Some((key, value)) = rest.split_once('=') {
            state
                .env
                .insert(key.trim().to_string(), value.trim().to_string());
        }
        return;
    }
    if let Some(rest) = command.strip_prefix("unset ") {
        state.env.remove(rest.trim());
    }
}

fn execute_with_shell_state(
    state: &mut InteractiveShellState,
    command: &str,
    policy: &PolicyProfile,
) -> Result<String, ProviderError> {
    if command.is_empty() {
        return Ok(String::new());
    }
    if matches!(
        evaluate(policy, OperationKind::Write),
        PolicyDecision::RequireApproval
    ) && command.contains("rm ")
    {
        return Err(ProviderError {
            message: "command requires approval".into(),
        });
    }

    if command == "pwd" {
        return Ok(state.cwd.clone());
    }
    if let Some(rest) = command.strip_prefix("cd ") {
        let new_dir = rest.trim();
        if new_dir.starts_with('/') {
            state.cwd = new_dir.to_string();
        } else if new_dir == "." || new_dir.is_empty() {
        } else if new_dir == ".." {
            if let Some((parent, _)) = state.cwd.rsplit_once('/') {
                state.cwd = if parent.is_empty() {
                    "/".to_string()
                } else {
                    parent.to_string()
                };
            }
        } else if state.cwd == "/" {
            state.cwd = format!("/{new_dir}");
        } else {
            state.cwd = format!("{}/{}", state.cwd, new_dir);
        }
        return Ok(String::new());
    }
    if let Some(rest) = command.strip_prefix("export ") {
        if let Some((key, value)) = rest.split_once('=') {
            state
                .env
                .insert(key.trim().to_string(), value.trim().to_string());
        }
        return Ok(String::new());
    }
    if let Some(rest) = command.strip_prefix("unset ") {
        state.env.remove(rest.trim());
        return Ok(String::new());
    }

    #[cfg(windows)]
    let shell_script = {
        let mut script = String::new();
        if !state.cwd.is_empty() && state.cwd != "/" {
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
    };

    #[cfg(not(windows))]
    let shell_script = {
        let mut script = String::new();
        script.push_str("set -e\n");
        script.push_str(&format!("cd {}\n", shell_single_quote(&state.cwd)));
        for (key, value) in &state.env {
            script.push_str(&format!("export {}={}\n", key, shell_single_quote(value)));
        }
        script.push_str(command);
        script
    };

    let output = run_shell_command(&shell_script)?;

    let mut lines = Vec::<String>::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        lines.push(line.to_string());
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        lines.push(format!("stderr: {line}"));
    }
    Ok(lines.join("\n"))
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(windows)]
fn shell_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn run_shell_command(command: &str) -> Result<Output, ProviderError> {
    #[cfg(windows)]
    let mut shell = {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    };

    #[cfg(not(windows))]
    let mut shell = {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-lc").arg(command);
        cmd
    };

    shell.output().map_err(|e| ProviderError {
        message: format!("failed to run shell command: {e}"),
    })
}

fn platform_interactive_shell_command(launch_command: Option<&str>) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        if let Some(launch_command) = launch_command {
            command.arg("/C").arg(launch_command);
        } else {
            command.arg("/Q").arg("/K");
        }
        command
    }

    #[cfg(not(windows))]
    {
        let mut command = Command::new("/bin/sh");
        if let Some(launch_command) = launch_command {
            command.arg("-lc").arg(launch_command);
        } else {
            command.arg("-s");
        }
        command
    }
}

fn parse_artifact_refine_mode(mode_label: &str) -> Result<ArtifactRefineMode, ProviderError> {
    match mode_label.trim().to_ascii_lowercase().as_str() {
        "keyword" => Ok(ArtifactRefineMode::Keyword),
        "regex" => Ok(ArtifactRefineMode::Regex),
        "auto" | "" => Ok(ArtifactRefineMode::Auto),
        other => Err(ProviderError {
            message: format!("invalid artifact refine mode: {other}"),
        }),
    }
}

pub struct GitProvider {
    repo_path: PathBuf,
}

impl GitProvider {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    pub fn status(&self) -> Result<String, ProviderError> {
        self.run_git(["status", "--short"])
    }

    pub fn diff(&self, reference: &str) -> Result<String, ProviderError> {
        self.run_git(["diff", reference])
    }

    pub fn log(&self, max_count: usize) -> Result<String, ProviderError> {
        self.run_git(["log", "--oneline", "--max-count", &max_count.to_string()])
    }

    fn run_git<const N: usize>(&self, args: [&str; N]) -> Result<String, ProviderError> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(args)
            .output()
            .map_err(|e| ProviderError {
                message: format!("failed to run git: {e}"),
            })?;

        if !output.status.success() {
            return Err(ProviderError {
                message: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::time::SystemTime;

    use bridgingio_domain::PolicyProfile;

    use super::TerminalProvider;

    #[test]
    fn refines_artifact_with_keyword() {
        let mut provider = TerminalProvider::default();
        let now = SystemTime::now();
        let raw = provider
            .artifacts
            .create_raw("a1", "s1", None, None, "echo test", "raw", now);
        provider.artifacts.append_chunk(&raw.id, "alpha");
        provider.artifacts.append_chunk(&raw.id, "beta");
        provider.artifacts.append_chunk(&raw.id, "alpha gamma");

        let derived = provider
            .refine_keyword(&raw.id, "a2", "alpha")
            .expect("derive");
        let chunks = provider.artifacts.read_chunks(&derived.id, 0, 10);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn refines_artifact_with_regex_like_filter() {
        let mut provider = TerminalProvider::default();
        let now = SystemTime::now();
        let raw =
            provider
                .artifacts
                .create_raw("a1", "s1", None, None, "logcat -b all", "raw", now);
        provider
            .artifacts
            .append_chunk(&raw.id, "system_server: ok");
        provider.artifacts.append_chunk(&raw.id, "Netd: ready");
        provider
            .artifacts
            .append_chunk(&raw.id, "surfaceflinger: frame");

        let derived = provider
            .refine_with_filter(&raw.id, "a2", "system_server|netd", "regex", true)
            .expect("derive");
        let chunks = provider.artifacts.read_chunks(&derived.id, 0, 10);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn blocks_dangerous_command_when_policy_requires_approval() {
        let mut provider = TerminalProvider::default();
        let err = provider
            .exec_local(
                "session",
                Some("ch-1"),
                Some("ts-1"),
                "rm -rf /tmp/x",
                "art",
                &PolicyProfile::default(),
            )
            .expect_err("must reject command");
        assert!(err.message.contains("requires approval"));
    }

    #[test]
    fn exec_local_records_basic_command_output() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        #[cfg(windows)]
        let command = "echo provider-ok";
        #[cfg(not(windows))]
        let command = "printf 'provider-ok\\n'";

        let artifact = provider
            .exec_local("session", Some("ch-1"), Some("ts-1"), command, "art", &policy)
            .expect("exec command");
        let chunks = provider.artifacts.read_chunks(&artifact.id, 0, 20);
        assert!(
            chunks.iter().any(|line| line.contains("provider-ok")),
            "artifact chunks: {chunks:?}"
        );
    }

    #[test]
    fn interactive_shell_preserves_env_and_cwd_per_channel() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell_a = provider.open_interactive_shell("ls-1", "ch-a", Some("ts-1"), "ssh");
        let shell_b = provider.open_interactive_shell("ls-1", "ch-b", Some("ts-1"), "ssh");

        provider
            .write_interactive_shell(&shell_a.shell_id, "export DEMO=hello", "a1", &policy)
            .expect("export");
        let output = provider
            .write_interactive_shell(&shell_a.shell_id, "printf \"$DEMO\\n\"", "a2", &policy)
            .expect("print env");
        assert!(output.output.contains("hello"));

        provider
            .write_interactive_shell(&shell_a.shell_id, "cd /tmp", "a3", &policy)
            .expect("cd");
        let pwd = provider
            .write_interactive_shell(&shell_a.shell_id, "pwd", "a4", &policy)
            .expect("pwd");
        assert_eq!(pwd.output.trim(), "/tmp");

        let output_b = provider
            .write_interactive_shell(&shell_b.shell_id, "printf \"$DEMO\\n\"", "b1", &policy)
            .expect("print env b");
        assert!(!output_b.output.contains("hello"));
    }

    #[test]
    fn interactive_shell_supports_interrupt_close_and_read_transcript() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell = provider.open_interactive_shell("ls-2", "ch-1", Some("ts-1"), "adb");
        provider
            .write_interactive_shell(&shell.shell_id, "echo hello", "s1", &policy)
            .expect("write");
        provider
            .interrupt_interactive_shell(&shell.shell_id)
            .expect("interrupt");
        provider
            .close_interactive_shell(&shell.shell_id)
            .expect("close");

        let transcript = provider
            .read_interactive_transcript(&shell.shell_id, 0, 20)
            .expect("read");
        assert!(transcript.iter().any(|line| line.contains("hello")));
        assert!(transcript
            .iter()
            .any(|line| line.contains("interrupt requested")));
        assert!(transcript.iter().any(|line| line.contains("closed")));

        let err = provider
            .write_interactive_shell(&shell.shell_id, "echo after-close", "s2", &policy)
            .expect_err("must reject");
        assert!(err.message.contains("closed"));
    }

    #[test]
    fn interactive_shell_can_mark_running_and_interrupt_long_command() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell = provider.open_interactive_shell("ls-3", "ch-1", Some("ts-1"), "ssh");
        let write = provider
            .write_interactive_shell(&shell.shell_id, "sleep 1; echo done", "l1", &policy)
            .expect("write long command");

        if write.running {
            provider
                .interrupt_interactive_shell(&shell.shell_id)
                .expect("interrupt");
            let state = provider
                .get_interactive_shell(&shell.shell_id)
                .expect("state after interrupt");
            assert!(state.interrupted);
            assert!(!state.running);
        }
    }

    #[test]
    fn platform_interactive_shell_default_command_is_platform_specific() {
        let command = super::platform_interactive_shell_command(None);
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        #[cfg(windows)]
        {
            assert_eq!(command.get_program(), OsStr::new("cmd"));
            assert_eq!(args, vec!["/Q", "/K"]);
        }

        #[cfg(not(windows))]
        {
            assert_eq!(command.get_program(), OsStr::new("/bin/sh"));
            assert_eq!(args, vec!["-s"]);
        }
    }

    #[test]
    fn platform_interactive_shell_launch_command_is_platform_specific() {
        let command = super::platform_interactive_shell_command(Some("echo hello"));
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        #[cfg(windows)]
        {
            assert_eq!(command.get_program(), OsStr::new("cmd"));
            assert_eq!(args, vec!["/C", "echo hello"]);
        }

        #[cfg(not(windows))]
        {
            assert_eq!(command.get_program(), OsStr::new("/bin/sh"));
            assert_eq!(args, vec!["-lc", "echo hello"]);
        }
    }

    #[test]
    fn marker_command_is_platform_specific() {
        let command = super::marker_command_for_shell("BRIDGINGIO_DONE_MARKER");
        #[cfg(windows)]
        assert_eq!(command, "echo BRIDGINGIO_DONE_MARKER");
        #[cfg(not(windows))]
        assert_eq!(command, "printf '%s\\n' 'BRIDGINGIO_DONE_MARKER'");
    }

    #[cfg(unix)]
    #[test]
    fn interactive_shell_exposes_tty_for_terminal_programs() {
        let mut provider = TerminalProvider::default();
        let policy = PolicyProfile::default();
        let shell = provider.open_interactive_shell("ls-4", "ch-1", Some("ts-1"), "adb");
        let tty = provider
            .write_interactive_shell(&shell.shell_id, "tty", "tty-1", &policy)
            .expect("tty");
        let output = tty.output.to_lowercase();
        assert!(!output.contains("not a tty"));
        assert!(!output.contains("inappropriate ioctl"));
    }
}
