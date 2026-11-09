//! Trabajo de daemon fuera del hilo de dibujo.
//!
//! La UI pide y sigue dibujando; el resultado llega por canal y se aplica en
//! `drain`. Si el daemon tarda, como mucho se ve "cargando…", nunca una
//! pantalla congelada.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

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
    Settings,
}

#[derive(Debug)]
pub enum Reply {
    Providers(Result<Value, String>),
    Workspaces(Result<Value, String>),
    Threads(Result<Value, String>),
    Transcript(Result<Value, String>),
    Settings(Result<Value, String>),
}

#[derive(Debug)]
pub enum Poll {
    Empty,
    Reply(Reply),
    /// El worker terminó: no habrá más respuestas.
    Closed,
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
                    Request::Settings => Reply::Settings(load_settings()),
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

    /// Solo para tests: un cliente cuyo worker ya no existe.
    #[doc(hidden)]
    pub fn closed() -> Self {
        let (tx, jobs) = channel::<Request>();
        drop(jobs);
        let (replies, rx) = channel::<Reply>();
        drop(replies);
        Self { tx, rx }
    }

    pub fn request(&self, req: Request) {
        let _ = self.tx.send(req);
    }

    /// Sin bloquear. Distingue «nada todavía» de «el worker murió»: sin eso,
    /// la UI se quedaba en `cargando…` para siempre.
    pub fn poll(&self) -> Poll {
        match self.rx.try_recv() {
            Ok(reply) => Poll::Reply(reply),
            Err(TryRecvError::Empty) => Poll::Empty,
            Err(TryRecvError::Disconnected) => Poll::Closed,
        }
    }

    pub fn try_recv(&self) -> Option<Reply> {
        match self.poll() {
            Poll::Reply(reply) => Some(reply),
            _ => None,
        }
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

/// Las cuatro consultas de Settings en paralelo: 800 ms de techo total, no por
/// consulta. Antes, un daemon lento sumaba 3,2 s antes de pintar la vista.
fn load_settings() -> Result<Value, String> {
    let budget = std::time::Duration::from_millis(800);
    let calls: [(&'static str, Value); 4] = [
        ("provider.list", json!({})),
        ("module.list", json!({})),
        ("mch.list", json!({})),
        ("prices.list", json!({})),
    ];
    let handles: Vec<_> = calls
        .into_iter()
        .map(|(cmd, args)| std::thread::spawn(move || client::call_with_timeout(cmd, args, budget)))
        .collect();
    let mut results = Vec::with_capacity(4);
    for handle in handles {
        results.push(handle.join().unwrap_or_else(|_| Err("worker caído".into())));
    }
    let mut it = results.into_iter();
    let providers = it.next().unwrap_or_else(|| Err("sin resultado".into()))?;
    let modules = it.next().unwrap_or_else(|| Err("sin resultado".into()))?;
    let mchs = it.next().unwrap_or_else(|| Err("sin resultado".into()))?;
    let prices = it.next().unwrap_or_else(|| Err("sin resultado".into()))?;
    Ok(json!({
        "providers": providers,
        "modules": modules,
        "mchs": mchs,
        "prices": prices,
    }))
}
