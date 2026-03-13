use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serverbee_common::constants::{has_capability, CAP_TERMINAL, MAX_TERMINAL_SESSIONS};
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
    _reader_handle: tokio::task::JoinHandle<()>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
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
                _reader_handle: reader_handle,
                _child: child,
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
        if let Some(session) = self.sessions.remove(session_id) {
            session._reader_handle.abort();
            tracing::debug!("Closed terminal session {session_id}");
        }
    }

    /// Close all terminal sessions.
    pub fn close_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.close(&id);
        }
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
