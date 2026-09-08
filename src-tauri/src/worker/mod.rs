//! The shared stateless-worker supervisor (ADR-0013).
//!
//! One `WorkerSupervisor` owns one Python worker process. It spawns
//! `python <script>` under a Windows Job Object with the ADR-0015 lockdown env,
//! performs the [`WorkerHello`] handshake (protocol-version + kind check), and
//! multiplexes request/response lines keyed by [`WorkerJobId`]. A dead child is
//! restarted with bounded backoff; after the limit the supervisor is `Failed`
//! and every `request` returns [`AppError::BackendUnavailable`].
//!
//! Reused by STT (18), TTS (19), the face-embedder (27).

pub mod env;
pub mod layout;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::contracts::ids::WorkerJobId;
use crate::contracts::worker::{
    WorkerHello, WorkerKind, WorkerRequest, WorkerResponse, WorkerResult,
};
use crate::contracts::WORKER_PROTOCOL_VERSION;
use crate::ipc::{AppError, AppResult};
use crate::job::JobObject;
pub use layout::WorkerLayout;

/// Bounded-restart policy for a crashed worker.
#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    /// How many times a dead child is respawned before the supervisor is
    /// `Failed`.
    pub max_restarts: u32,
    /// Fixed delay before each respawn.
    pub backoff: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 3,
            backoff: Duration::from_millis(500),
        }
    }
}

/// A progress frame forwarded from the worker mid-job.
pub type ProgressSink = mpsc::UnboundedSender<(f32, Option<String>)>;

/// Supervises one worker process. Cheap to hold; the child is spawned lazily on
/// the first `request`.
pub struct WorkerSupervisor {
    layout: WorkerLayout,
    kind: WorkerKind,
    policy: RestartPolicy,
    /// Extra env for the child, on top of the ADR-0015 lockdown (e.g. the STT
    /// worker's model dir). Never a network knob.
    extra_env: Vec<(String, String)>,
    inner: Mutex<Inner>,
}

struct Inner {
    running: Option<Running>,
    /// Restarts spent since the last clean start.
    restarts: u32,
    /// Set once `restarts` exceeds the policy — every `request` fails fast.
    failed: bool,
}

type Pending = Arc<Mutex<HashMap<WorkerJobId, mpsc::UnboundedSender<WorkerResult>>>>;

struct Running {
    stdin: ChildStdin,
    pending: Pending,
    /// Cleared by the response-reader task when the child's stdout closes.
    alive: Arc<AtomicBool>,
    /// Kills the child on drop; also the PID source for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    child_guard: ChildGuard,
    /// Held for the lifetime of the process (kill-on-close job).
    _job: JobObject,
}

/// Kills the child on drop (belt to the Job Object's braces).
struct ChildGuard(tokio::process::Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}

impl WorkerSupervisor {
    /// A supervisor for `kind`, using `layout` to find the interpreter + script.
    /// Does not spawn anything yet.
    #[must_use]
    pub fn new(layout: WorkerLayout, kind: WorkerKind) -> Self {
        Self::with_policy(layout, kind, RestartPolicy::default())
    }

    /// As [`Self::new`] with an explicit restart policy.
    #[must_use]
    pub fn with_policy(layout: WorkerLayout, kind: WorkerKind, policy: RestartPolicy) -> Self {
        Self {
            layout,
            kind,
            policy,
            extra_env: Vec::new(),
            inner: Mutex::new(Inner {
                running: None,
                restarts: 0,
                failed: false,
            }),
        }
    }

    /// Add child environment variables (applied after the ADR-0015 lockdown).
    #[must_use]
    pub fn with_env(mut self, vars: impl IntoIterator<Item = (String, String)>) -> Self {
        self.extra_env.extend(vars);
        self
    }

    /// Spawn the worker (and load its model) if it isn't running. Used to
    /// overlap Chatterbox / faster-whisper load with the user's first utterance.
    pub async fn warm(&self) -> AppResult<()> {
        let mut inner = self.inner.lock().await;
        self.ensure_running(&mut inner).await
    }

    /// Send one job to the worker and await its terminal result.
    ///
    /// `cancel` abandons the wait (the worker keeps running; its late response is
    /// dropped). `progress` receives every `Progress` frame before the terminal
    /// one.
    ///
    /// # Errors
    /// - [`AppError::BackendUnavailable`] — worker missing, won't start, past the
    ///   restart limit, or died mid-job.
    /// - Whatever [`AppError`] the worker returned in `WorkerResult::Err`.
    /// - [`AppError::Timeout`] is *not* raised here — callers apply their own
    ///   deadline via `cancel`.
    pub async fn request(
        &self,
        payload: Value,
        cancel: &CancellationToken,
        progress: Option<ProgressSink>,
    ) -> AppResult<Value> {
        let job_id = WorkerJobId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let (tx, mut rx) = mpsc::unbounded_channel::<WorkerResult>();

        {
            let mut inner = self.inner.lock().await;
            self.ensure_running(&mut inner).await?;
            let running = inner
                .running
                .as_mut()
                .expect("ensure_running set it or errored");
            running.pending.lock().await.insert(job_id.clone(), tx);

            let line = serde_json::to_string(&WorkerRequest {
                id: job_id.clone(),
                kind: self.kind,
                payload,
            })
            .map_err(|e| AppError::internal("serialize worker request", e))?;
            if let Err(e) = write_line(&mut running.stdin, &line).await {
                running.pending.lock().await.remove(&job_id);
                self.mark_dead(&mut inner);
                return Err(AppError::BackendUnavailable(format!(
                    "worker stdin write failed: {e}"
                )));
            }
        }

        let result = loop {
            tokio::select! {
                () = cancel.cancelled() => break Err(AppError::Cancelled),
                frame = rx.recv() => match frame {
                    Some(WorkerResult::Progress { progress: p, detail }) => {
                        if let Some(sink) = &progress {
                            let _ = sink.send((p, detail));
                        }
                    }
                    Some(WorkerResult::Ok { data }) => break Ok(data),
                    Some(WorkerResult::Err { error }) => break Err(error),
                    None => break Err(AppError::BackendUnavailable(
                        "worker exited before answering".to_owned(),
                    )),
                },
            }
        };

        // Unregister; drop the child if its reader task has since seen EOF.
        {
            let mut inner = self.inner.lock().await;
            if let Some(running) = inner.running.as_ref() {
                running.pending.lock().await.remove(&job_id);
                if !running.alive.load(Ordering::Relaxed) {
                    inner.running = None;
                }
            }
        }
        result
    }

    /// Stop the worker (if running). Idempotent.
    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        inner.running = None; // drops stdin + ChildGuard (start_kill) + JobObject
        inner.failed = false;
        inner.restarts = 0;
    }

    /// The child PID while it is running (tests).
    #[cfg(test)]
    pub async fn child_pid(&self) -> Option<u32> {
        self.inner
            .lock()
            .await
            .running
            .as_ref()
            .and_then(|r| r.child_guard.0.id())
    }

    async fn ensure_running(&self, inner: &mut Inner) -> AppResult<()> {
        // Drop a child whose reader task has seen stdout close.
        if inner
            .running
            .as_ref()
            .is_some_and(|r| !r.alive.load(Ordering::Relaxed))
        {
            inner.running = None;
        }
        if inner.running.is_some() {
            return Ok(());
        }
        if inner.failed {
            return Err(AppError::BackendUnavailable(format!(
                "{:?} worker failed permanently after {} restarts",
                self.kind, inner.restarts
            )));
        }
        if inner.restarts > 0 {
            tokio::time::sleep(self.policy.backoff).await;
        }
        match self.spawn().await {
            Ok(running) => {
                inner.running = Some(running);
                Ok(())
            }
            Err(e) => {
                inner.restarts += 1;
                if inner.restarts > self.policy.max_restarts {
                    inner.failed = true;
                }
                Err(e)
            }
        }
    }

    fn mark_dead(&self, inner: &mut Inner) {
        inner.running = None;
        inner.restarts += 1;
        if inner.restarts > self.policy.max_restarts {
            inner.failed = true;
        }
    }

    async fn spawn(&self) -> AppResult<Running> {
        let script = self.layout.script_for(self.kind)?;
        let job =
            JobObject::new().map_err(|e| AppError::internal("create worker job object", e))?;

        let mut cmd = Command::new(&self.layout.python);
        cmd.arg(&script)
            .current_dir(&self.layout.workers_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        env::apply(&mut cmd);
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        crate::job::hide_console(&mut cmd);

        let mut child = cmd.spawn().map_err(|e| {
            AppError::BackendUnavailable(format!(
                "failed to start {:?} worker ({}): {e}",
                self.kind,
                self.layout.python.display()
            ))
        })?;

        if let Some(handle) = raw_handle(&child) {
            if let Err(e) = job.assign(handle) {
                let _ = child.start_kill();
                return Err(AppError::internal("assign worker to job object", e));
            }
        }

        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let kind = self.kind;

        // stderr → logs only (ADR-0013).
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "worker", "[{kind:?}] {line}");
            }
        });

        let mut reader = BufReader::new(stdout).lines();
        read_hello(&mut reader, kind).await?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        tokio::spawn(read_responses(
            reader,
            Arc::clone(&pending),
            Arc::clone(&alive),
            kind,
        ));

        Ok(Running {
            stdin,
            pending,
            alive,
            child_guard: ChildGuard(child),
            _job: job,
        })
    }
}

/// Read + validate the worker's first line (`WorkerHello`).
async fn read_hello(
    reader: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    kind: WorkerKind,
) -> AppResult<()> {
    let line = tokio::time::timeout(Duration::from_secs(30), reader.next_line())
        .await
        .map_err(|_| AppError::BackendUnavailable(format!("{kind:?} worker sent no hello in 30s")))?
        .map_err(|e| AppError::BackendUnavailable(format!("worker stdout read: {e}")))?
        .ok_or_else(|| {
            AppError::BackendUnavailable(format!("{kind:?} worker closed before hello"))
        })?;
    let hello: WorkerHello = serde_json::from_str(&line).map_err(|e| {
        AppError::BackendUnavailable(format!("worker hello was not valid JSON: {e}"))
    })?;
    if hello.protocol_version != WORKER_PROTOCOL_VERSION {
        return Err(AppError::BackendUnavailable(format!(
            "{kind:?} worker protocol {} != supported {WORKER_PROTOCOL_VERSION}",
            hello.protocol_version
        )));
    }
    if hello.worker != kind {
        return Err(AppError::BackendUnavailable(format!(
            "worker announced {:?}, expected {kind:?}",
            hello.worker
        )));
    }
    Ok(())
}

/// Dispatch every response line to its job's channel until stdout closes, then
/// mark the worker dead and fail all in-flight jobs.
async fn read_responses(
    mut reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    pending: Pending,
    alive: Arc<AtomicBool>,
    kind: WorkerKind,
) {
    while let Ok(Some(line)) = reader.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<WorkerResponse>(&line) {
            Ok(resp) => {
                if let Some(tx) = pending.lock().await.get(&resp.id) {
                    let _ = tx.send(resp.result);
                }
            }
            Err(e) => {
                tracing::debug!(target: "worker", "[{kind:?}] unparseable response line: {e}");
            }
        }
    }
    alive.store(false, Ordering::Relaxed);
    for (_, tx) in pending.lock().await.drain() {
        let _ = tx.send(WorkerResult::Err {
            error: AppError::BackendUnavailable(format!("{kind:?} worker exited")),
        });
    }
}

async fn write_line(stdin: &mut ChildStdin, line: &str) -> std::io::Result<()> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

#[cfg(windows)]
fn raw_handle(child: &tokio::process::Child) -> Option<isize> {
    child.raw_handle().map(|h| h as isize)
}

#[cfg(not(windows))]
fn raw_handle(_child: &tokio::process::Child) -> Option<isize> {
    None
}
