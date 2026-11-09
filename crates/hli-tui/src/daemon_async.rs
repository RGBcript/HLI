//! Trabajo de daemon fuera del hilo de dibujo.
//!
//! La UI pide y sigue dibujando; el resultado llega por canal y se aplica en
//! `drain`. Si el daemon tarda, como mucho se ve "cargando…", nunca una
//! pantalla congelada.

use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};

use crate::client;

#[derive(Debug)]
pub enum Request {
    Providers,
    Workspaces,
    Threads(String),
    Transcript {
        workspace_id: String,
        branch_id: Option<String>,
    },
}

#[derive(Debug)]
pub enum Reply {
    Providers(Result<Value, String>),
    Workspaces(Result<Value, String>),
    Threads(Result<Value, String>),
    Transcript(Result<Value, String>),
}

pub struct AsyncDaemon {
    tx: Sender<Request>,
    rx: Receiver<Reply>,
}

impl AsyncDaemon {
    pub fn new() -> Self {
        let (tx, jobs) = channel::<Request>();
        let (replies, rx) = channel::<Reply>();
        std::thread::spawn(move || {
            while let Ok(job) = jobs.recv() {
                let reply = match job {
                    Request::Providers => {
                        Reply::Providers(client::call_ui("provider.list", json!({})))
                    }
                    Request::Workspaces => {
                        Reply::Workspaces(client::call_ui("workspace.list", json!({})))
                    }
                    Request::Threads(workspace_id) => Reply::Threads(client::call_ui(
                        "thread.list",
                        json!({ "workspace_id": workspace_id }),
                    )),
                    Request::Transcript {
                        workspace_id,
                        branch_id,
                    } => {
                        let mut args = json!({ "workspace_id": workspace_id });
                        if let Some(branch) = branch_id {
                            args["branch_id"] = json!(branch);
                        }
                        let events = client::call_ui("event.list", args);
                        let runs =
                            client::call_ui("run.list", json!({ "workspace_id": workspace_id }));
                        Reply::Transcript(match (runs, events) {
                            (Ok(runs), Ok(events)) => Ok(json!({ "runs": runs, "events": events })),
                            (Err(e), _) | (_, Err(e)) => Err(e),
                        })
                    }
                };
                if replies.send(reply).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx }
    }

    pub fn request(&self, req: Request) {
        let _ = self.tx.send(req);
    }

    /// Sin bloquear: lo que haya listo, nada más.
    pub fn try_recv(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }

    /// Solo para tests y arranques: espera un resultado con un techo.
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<Reply> {
        self.rx.recv_timeout(timeout).ok()
    }
}

impl Default for AsyncDaemon {
    fn default() -> Self {
        Self::new()
    }
}
