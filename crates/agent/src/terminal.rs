use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serverbee_common::constants::{CAP_TERMINAL, MAX_TERMINAL_SESSIONS, has_capability};
use tokio::sync::mpsc;

/// Message sent from terminal sessions back to the reporter.
pub enum TerminalEvent {
    Output { session_id: String, data: String },
    Started { session_id: String },
    Error { session_id: String, error: String },
    Exited { session_id: String },
}

struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    reader_handle: tokio::task::JoinHandle<()>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

/// Manages PTY terminal sessions on the agent.
pub struct TerminalManager {
    sessions: HashMap<String, PtySession>,
    event_tx: mpsc::Sender<TerminalEvent>,
    capabilities: Arc<AtomicU32>,
}

impl TerminalManager {
    pub fn new(event_tx: mpsc::Sender<TerminalEvent>, capabilities: Arc<AtomicU32>) -> Self {
        Self {
            sessions: HashMap::new(),
            event_tx,
            capabilities,
        }
    }

    /// Open a new terminal session with the given dimensions.
    pub fn open(&mut self, session_id: String, rows: u16, cols: u16) {
        let caps = self.capabilities.load(Ordering::SeqCst);
        if !has_capability(caps, CAP_TERMINAL) {
            tracing::warn!("Terminal denied: capability disabled (session={session_id})");
            let tx = self.event_tx.clone();
            let sid = session_id;
            tokio::spawn(async move {
                let _ = tx
                    .send(TerminalEvent::Error {
                        session_id: sid,
                        error: "Terminal capability is disabled".to_string(),
                    })
                    .await;
            });
            return;
        }

        if self.sessions.len() >= MAX_TERMINAL_SESSIONS {
            let tx = self.event_tx.clone();
            let sid = session_id.clone();
            tokio::spawn(async move {
                let _ = tx
                    .send(TerminalEvent::Error {
                        session_id: sid,
                        error: format!("Max terminal sessions ({MAX_TERMINAL_SESSIONS}) reached"),
                    })
                    .await;
            });
            return;
        }

        if self.sessions.contains_key(&session_id) {
            tracing::warn!("Terminal session {session_id} already exists");
            return;
        }

        let pty_system = NativePtySystem::default();
        let pair = match pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) => {
                let tx = self.event_tx.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(TerminalEvent::Error {
                            session_id: sid,
                            error: format!("Failed to open PTY: {e}"),
                        })
                        .await;
                });
                return;
            }
        };

        // Determine the user's shell
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");

        let child = match pair.slave.spawn_command(cmd) {
            Ok(c) => c,
            Err(e) => {
                let tx = self.event_tx.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(TerminalEvent::Error {
                            session_id: sid,
                            error: format!("Failed to spawn shell: {e}"),
                        })
                        .await;
                });
                return;
            }
        };

        let writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => {
                let tx = self.event_tx.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(TerminalEvent::Error {
                            session_id: sid,
                            error: format!("Failed to take PTY writer: {e}"),
                        })
                        .await;
                });
                return;
            }
        };

        let reader = match pair.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => {
                let tx = self.event_tx.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    let _ = tx
                        .send(TerminalEvent::Error {
                            session_id: sid,
                            error: format!("Failed to clone PTY reader: {e}"),
                        })
                        .await;
                });
                return;
            }
        };

        // Spawn blocking reader task
        let tx = self.event_tx.clone();
        let sid = session_id.clone();
        let reader_handle = tokio::task::spawn_blocking(move || {
            read_pty_output(reader, &sid, &tx);
        });

        self.sessions.insert(
            session_id.clone(),
            PtySession {
                writer,
                master: pair.master,
                reader_handle,
                child,
            },
        );

        // Notify that session started
        let tx = self.event_tx.clone();
        let sid = session_id;
        tokio::spawn(async move {
            let _ = tx.send(TerminalEvent::Started { session_id: sid }).await;
        });
    }

    /// Write input data (base64 encoded) to a terminal session.
    pub fn write_input(&mut self, session_id: &str, data_b64: &str) {
        let session = match self.sessions.get_mut(session_id) {
            Some(s) => s,
            None => {
                tracing::warn!("Terminal session {session_id} not found for input");
                return;
            }
        };

        let data = match BASE64.decode(data_b64) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Invalid base64 terminal input: {e}");
                return;
            }
        };

        if let Err(e) = session.writer.write_all(&data) {
            tracing::error!("Failed to write to PTY {session_id}: {e}");
            // Session may be dead, clean up
            self.close(session_id);
        }
    }

    /// Resize a terminal session.
    pub fn resize(&mut self, session_id: &str, rows: u16, cols: u16) {
        let session = match self.sessions.get_mut(session_id) {
            Some(s) => s,
            None => {
                tracing::warn!("Terminal session {session_id} not found for resize");
                return;
            }
        };

        if let Err(e) = session.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            tracing::warn!("Failed to resize PTY {session_id}: {e}");
        }
    }

    /// Close a terminal session.
    pub fn close(&mut self, session_id: &str) {
        if let Some(mut session) = self.sessions.remove(session_id) {
            // Kill the child process and reap it so closed sessions don't
            // leave orphaned shells / zombie processes behind. Dropping the
            // PTY master alone only sends EOF/SIGHUP and is not a reliable
            // way to terminate or wait for the child.
            if let Err(e) = session.child.kill() {
                tracing::debug!("Failed to kill terminal child {session_id}: {e}");
            }
            if let Err(e) = session.child.wait() {
                tracing::debug!("Failed to reap terminal child {session_id}: {e}");
            }
            session.reader_handle.abort();
            tracing::debug!("Closed terminal session {session_id}");
        }
    }

    /// Close all terminal sessions. Returns the ids that were open, so the
    /// caller can notify the server/browser side that they ended (e.g. on
    /// capability revocation, where the PTY is torn down locally rather than
    /// exiting on its own).
    pub fn close_all(&mut self) -> Vec<String> {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in &ids {
            self.close(id);
        }
        ids
    }
}

/// Blocking function that reads PTY output and sends it as events.
fn read_pty_output(
    mut reader: Box<dyn Read + Send>,
    session_id: &str,
    tx: &mpsc::Sender<TerminalEvent>,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // PTY closed (child exited)
                let _ = tx.blocking_send(TerminalEvent::Exited {
                    session_id: session_id.to_string(),
                });
                break;
            }
            Ok(n) => {
                let data = BASE64.encode(&buf[..n]);
                if tx
                    .blocking_send(TerminalEvent::Output {
                        session_id: session_id.to_string(),
                        data,
                    })
                    .is_err()
                {
                    // Channel closed
                    break;
                }
            }
            Err(e) => {
                tracing::debug!("PTY read error for session {session_id}: {e}");
                let _ = tx.blocking_send(TerminalEvent::Exited {
                    session_id: session_id.to_string(),
                });
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::CAP_EXEC;

    fn pid_alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Drain the event channel until an event matching `pred` is received, or
    /// the bounded timeout elapses. Non-matching events (e.g. PTY `Output`
    /// noise from a freshly spawned shell) are skipped so assertions stay
    /// resilient to timing. Panics with `msg` on timeout.
    async fn wait_for_event<F>(
        rx: &mut mpsc::Receiver<TerminalEvent>,
        mut pred: F,
        msg: &str,
    ) -> TerminalEvent
    where
        F: FnMut(&TerminalEvent) -> bool,
    {
        let deadline = std::time::Duration::from_secs(5);
        loop {
            match tokio::time::timeout(deadline, rx.recv()).await {
                Ok(Some(ev)) if pred(&ev) => return ev,
                // Skip non-matching events and keep waiting (within the bound).
                Ok(Some(_)) => {}
                Ok(None) => panic!("event channel closed unexpectedly: {msg}"),
                Err(_) => panic!("timed out waiting for event: {msg}"),
            }
        }
    }

    /// Open denied when CAP_TERMINAL is absent: an `Error` event is emitted and
    /// no session is created.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn open_denied_without_capability_emits_error() {
        let (tx, mut rx) = mpsc::channel(64);
        // No CAP_TERMINAL bit set.
        let caps = Arc::new(AtomicU32::new(0));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "denied".to_string();
        mgr.open(sid.clone(), 24, 80);

        // No session should have been inserted.
        assert!(
            mgr.sessions.is_empty(),
            "no session should be created when capability is denied"
        );

        let ev = wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Error { .. }),
            "capability-denied error",
        )
        .await;
        match ev {
            TerminalEvent::Error { session_id, error } => {
                assert_eq!(session_id, sid);
                assert!(
                    error.contains("disabled"),
                    "error should mention capability disabled, got: {error}"
                );
            }
            _ => unreachable!(),
        }
    }

    /// Opening a session with CAP_TERMINAL spawns the shell and emits `Started`;
    /// the session is tracked by the manager.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn open_emits_started_and_tracks_session() {
        let (tx, mut rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "started".to_string();
        mgr.open(sid.clone(), 24, 80);

        assert!(
            mgr.sessions.contains_key(&sid),
            "session should be tracked after a successful open"
        );

        let ev = wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Started { .. }),
            "started event",
        )
        .await;
        match ev {
            TerminalEvent::Started { session_id } => assert_eq!(session_id, sid),
            _ => unreachable!(),
        }

        // Always reap the child to avoid orphaned shells.
        mgr.close(&sid);
    }

    /// Opening the same session id twice is a no-op for the second call: the
    /// existing session is preserved and no new event is required.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn open_duplicate_session_id_is_noop() {
        let (tx, mut rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "dup".to_string();
        mgr.open(sid.clone(), 24, 80);
        wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Started { .. }),
            "first started event",
        )
        .await;

        let pid_before = mgr
            .sessions
            .get(&sid)
            .expect("session exists")
            .child
            .process_id();

        // Second open with the same id: should hit the `contains_key` guard and
        // return early without replacing the session.
        mgr.open(sid.clone(), 30, 100);

        let pid_after = mgr
            .sessions
            .get(&sid)
            .expect("session still exists")
            .child
            .process_id();
        assert_eq!(
            pid_before, pid_after,
            "duplicate open must not replace the existing session's child"
        );
        assert_eq!(mgr.sessions.len(), 1, "still exactly one session");

        mgr.close(&sid);
    }

    /// Opening more than MAX_TERMINAL_SESSIONS sessions emits an `Error` and
    /// does not create a session beyond the cap.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn open_beyond_max_sessions_emits_error() {
        let (tx, mut rx) = mpsc::channel(128);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        // Fill up to the cap.
        for i in 0..MAX_TERMINAL_SESSIONS {
            let sid = format!("max-{i}");
            mgr.open(sid.clone(), 24, 80);
            wait_for_event(
                &mut rx,
                |ev| matches!(ev, TerminalEvent::Started { .. }),
                "started event while filling sessions",
            )
            .await;
        }
        assert_eq!(mgr.sessions.len(), MAX_TERMINAL_SESSIONS);

        // One more should be rejected with an Error event.
        let overflow_id = "overflow".to_string();
        mgr.open(overflow_id.clone(), 24, 80);
        assert_eq!(
            mgr.sessions.len(),
            MAX_TERMINAL_SESSIONS,
            "session count must not exceed the cap"
        );
        assert!(
            !mgr.sessions.contains_key(&overflow_id),
            "overflow session must not be inserted"
        );

        let ev = wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Error { .. }),
            "max-sessions error",
        )
        .await;
        match ev {
            TerminalEvent::Error { session_id, error } => {
                assert_eq!(session_id, overflow_id);
                assert!(
                    error.contains("Max terminal sessions"),
                    "error should mention the session cap, got: {error}"
                );
            }
            _ => unreachable!(),
        }

        // Clean up all sessions.
        mgr.close_all();
    }

    /// Writing valid base64 input to a live session succeeds and keeps the
    /// session open.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn write_input_valid_keeps_session() {
        let (tx, mut rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "write".to_string();
        mgr.open(sid.clone(), 24, 80);
        wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Started { .. }),
            "started event",
        )
        .await;

        // "echo hi\n" base64-encoded; exercises the decode + write_all path.
        let payload = BASE64.encode(b"echo hi\n");
        mgr.write_input(&sid, &payload);

        assert!(
            mgr.sessions.contains_key(&sid),
            "session should remain open after a successful write"
        );

        mgr.close(&sid);
    }

    /// Writing invalid base64 is a no-op: it must not panic and must not close
    /// the session.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn write_input_invalid_base64_is_noop() {
        let (tx, mut rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "bad-b64".to_string();
        mgr.open(sid.clone(), 24, 80);
        wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Started { .. }),
            "started event",
        )
        .await;

        // "!!!" is not valid base64; should hit the decode error branch.
        mgr.write_input(&sid, "!!!not-base64!!!");

        assert!(
            mgr.sessions.contains_key(&sid),
            "invalid base64 must not close the session"
        );

        mgr.close(&sid);
    }

    /// Writing to an unknown session id must not panic and creates nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn write_input_unknown_session_is_noop() {
        let (tx, _rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        // No session opened: hits the `None` branch of get_mut.
        mgr.write_input("does-not-exist", &BASE64.encode(b"hi"));
        assert!(mgr.sessions.is_empty());
    }

    /// Resizing a live session succeeds (no event, no panic) and keeps it open.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resize_existing_session_succeeds() {
        let (tx, mut rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "resize".to_string();
        mgr.open(sid.clone(), 24, 80);
        wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Started { .. }),
            "started event",
        )
        .await;

        mgr.resize(&sid, 40, 120);
        assert!(
            mgr.sessions.contains_key(&sid),
            "session should remain open after resize"
        );

        mgr.close(&sid);
    }

    /// Resizing an unknown session id must not panic.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resize_unknown_session_is_noop() {
        let (tx, _rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        // Hits the `None` branch of get_mut in resize().
        mgr.resize("nope", 40, 120);
        assert!(mgr.sessions.is_empty());
    }

    /// Closing an unknown session id is a no-op and must not panic.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_unknown_session_is_noop() {
        let (tx, _rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        // `sessions.remove` returns None -> the `if let Some` body is skipped.
        mgr.close("ghost");
        assert!(mgr.sessions.is_empty());
    }

    /// close_all returns the ids that were open and tears every session down,
    /// reaping their children.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_all_returns_ids_and_clears_sessions() {
        let (tx, mut rx) = mpsc::channel(128);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let ids = ["a", "b"];
        let mut pids = Vec::new();
        for id in ids {
            mgr.open(id.to_string(), 24, 80);
            wait_for_event(
                &mut rx,
                |ev| matches!(ev, TerminalEvent::Started { .. }),
                "started event",
            )
            .await;
            let pid = mgr
                .sessions
                .get(id)
                .expect("session exists")
                .child
                .process_id()
                .expect("shell has a pid");
            pids.push(pid);
        }
        assert_eq!(mgr.sessions.len(), 2);

        let mut returned = mgr.close_all();
        returned.sort();
        assert_eq!(returned, vec!["a".to_string(), "b".to_string()]);
        assert!(
            mgr.sessions.is_empty(),
            "close_all must remove every session"
        );

        // Children should be killed and reaped.
        for pid in pids {
            let mut reaped = false;
            for _ in 0..50 {
                if !pid_alive(pid) {
                    reaped = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert!(reaped, "child {pid} must be reaped by close_all()");
        }
    }

    /// close_all on an empty manager returns an empty vec.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_all_empty_returns_empty() {
        let (tx, _rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        assert!(mgr.close_all().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_reaps_child_process() {
        let (tx, _rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "reap-test".to_string();
        mgr.open(sid.clone(), 24, 80);

        let pid = mgr
            .sessions
            .get(&sid)
            .expect("session should exist after open")
            .child
            .process_id()
            .expect("spawned shell should have a pid");

        assert!(
            pid_alive(pid),
            "shell process {pid} should be running while the session is open"
        );

        mgr.close(&sid);

        assert!(
            !mgr.sessions.contains_key(&sid),
            "session should be removed from the manager after close()"
        );

        // close() kills and reaps the child; allow a brief window for the OS
        // to finish tearing the process down before asserting it is gone.
        let mut reaped = false;
        for _ in 0..50 {
            if !pid_alive(pid) {
                reaped = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            reaped,
            "child process {pid} must be killed and reaped by close(), not left orphaned"
        );
    }

    /// Open is denied when an unrelated capability bit is set but CAP_TERMINAL
    /// is not: the `has_capability` mask must reject it, not just the all-zero
    /// case. No PTY is spawned and no session is created.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn open_denied_with_other_capability_only() {
        let (tx, mut rx) = mpsc::channel(64);
        // A different bit is on (CAP_EXEC), but CAP_TERMINAL is absent.
        let caps = Arc::new(AtomicU32::new(CAP_EXEC));
        let mut mgr = TerminalManager::new(tx, caps);

        let sid = "other-cap".to_string();
        mgr.open(sid.clone(), 24, 80);

        assert!(
            mgr.sessions.is_empty(),
            "no session should be created when only an unrelated capability is set"
        );

        let ev = wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Error { .. }),
            "capability-denied error for unrelated bit",
        )
        .await;
        match ev {
            TerminalEvent::Error { session_id, error } => {
                assert_eq!(session_id, sid);
                assert!(
                    error.contains("disabled"),
                    "error should mention capability disabled, got: {error}"
                );
            }
            _ => unreachable!(),
        }
    }

    /// The capability is re-read from the shared atomic on every `open()` call:
    /// revoking CAP_TERMINAL after construction (e.g. capability revocation)
    /// must deny a subsequent open without spawning a PTY.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn open_respects_runtime_capability_revocation() {
        let (tx, mut rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, Arc::clone(&caps));

        // Revoke the terminal capability through the shared handle before opening.
        caps.store(0, Ordering::SeqCst);

        let sid = "revoked".to_string();
        mgr.open(sid.clone(), 24, 80);

        assert!(
            mgr.sessions.is_empty(),
            "open after revocation must not create a session"
        );

        let ev = wait_for_event(
            &mut rx,
            |ev| matches!(ev, TerminalEvent::Error { .. }),
            "error after runtime capability revocation",
        )
        .await;
        match ev {
            TerminalEvent::Error { session_id, error } => {
                assert_eq!(session_id, sid);
                assert!(error.contains("disabled"));
            }
            _ => unreachable!(),
        }
    }

    /// Empty-but-valid base64 ("") decodes to an empty payload; writing it to an
    /// unknown session hits the `None` get_mut branch and is a no-op. Exercises
    /// the valid-decode path distinct from the invalid-base64 case, with no PTY.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn write_input_empty_base64_unknown_session_is_noop() {
        let (tx, _rx) = mpsc::channel(64);
        let caps = Arc::new(AtomicU32::new(CAP_TERMINAL));
        let mut mgr = TerminalManager::new(tx, caps);

        // "" is valid base64 for an empty byte slice; session does not exist.
        mgr.write_input("ghost", "");
        assert!(mgr.sessions.is_empty());
    }

    /// TerminalEvent variants carry the exact fields they are constructed with;
    /// pure data check independent of any PTY or channel.
    #[test]
    fn terminal_event_variants_carry_expected_fields() {
        let output = TerminalEvent::Output {
            session_id: "s1".to_string(),
            data: "ZGF0YQ==".to_string(),
        };
        match output {
            TerminalEvent::Output { session_id, data } => {
                assert_eq!(session_id, "s1");
                assert_eq!(data, "ZGF0YQ==");
            }
            _ => unreachable!(),
        }

        let started = TerminalEvent::Started {
            session_id: "s2".to_string(),
        };
        assert!(matches!(started, TerminalEvent::Started { session_id } if session_id == "s2"));

        let error = TerminalEvent::Error {
            session_id: "s3".to_string(),
            error: "boom".to_string(),
        };
        match error {
            TerminalEvent::Error { session_id, error } => {
                assert_eq!(session_id, "s3");
                assert_eq!(error, "boom");
            }
            _ => unreachable!(),
        }

        let exited = TerminalEvent::Exited {
            session_id: "s4".to_string(),
        };
        assert!(matches!(exited, TerminalEvent::Exited { session_id } if session_id == "s4"));
    }
}
