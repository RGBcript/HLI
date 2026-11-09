//! Actor único de escritura. Todas las mutaciones pasan por este hilo.

use std::path::PathBuf;
use std::sync::mpsc::{Sender, SyncSender, channel, sync_channel};
use std::thread;

use hli_core::{
    HliError, Result,
    canon::{canonical_json, zero_hash},
    ids::new_id,
    model::*,
    time::now_iso,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::export::{self, ArchiveFile};
use crate::schema;

pub type Reply<T> = SyncSender<Result<T>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ArtifactInfo {
    pub id: String,
    pub sha256: String,
    pub mime: String,
    pub bytes: i64,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotInfo {
    pub id: String,
    pub branch_id: String,
    pub at_sequence: i64,
    pub path: String,
    pub content_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExportReport {
    pub path: String,
    pub manifest_hash: String,
    pub events: i64,
    pub artifacts: i64,
    pub chain_head: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImportReport {
    pub workspace_id: String,
    pub events: i64,
    pub artifacts: i64,
    pub verified: bool,
    pub chain_head: String,
}

#[allow(clippy::large_enum_variant)]
pub enum WriteCmd {
    CreateWorkspace {
        name: String,
        root: String,
        reply: Reply<Workspace>,
    },
    CreateThread {
        workspace_id: String,
        title: String,
        reply: Reply<Thread>,
    },
    CreateBranch {
        thread_id: String,
        name: String,
        from_event: Option<String>,
        reply: Reply<Branch>,
    },
    AppendEvent {
        workspace_id: String,
        thread_id: Option<String>,
        branch_id: Option<String>,
        kind: String,
        payload: Value,
        idempotency_key: Option<String>,
        reply: Reply<Event>,
    },
    PutArtifact {
        workspace_id: String,
        source: PathBuf,
        reply: Reply<ArtifactInfo>,
    },
    Snapshot {
        branch_id: String,
        reply: Reply<SnapshotInfo>,
    },
    Export {
        workspace_id: String,
        out: PathBuf,
        reply: Reply<ExportReport>,
    },
    Import {
        archive: PathBuf,
        reply: Reply<ImportReport>,
    },
    CreateRun {
        workspace_id: String,
        thread_id: String,
        branch_id: String,
        prompt: String,
        reply: Reply<Run>,
    },
    SaveManifest {
        manifest: ContextManifest,
        reply: Reply<ContextManifest>,
    },
    RunEvent {
        run_id: String,
        kind: String,
        payload: Value,
        reply: Reply<Event>,
    },
    SetRunState {
        run_id: String,
        state: String,
        reply: Reply<Run>,
    },
    FinishRun {
        run_id: String,
        state: String,
        result: Option<Value>,
        receipt: Option<Receipt>,
        reply: Reply<Run>,
    },
    SavePlan {
        plan: RunPlan,
        execution: Option<Value>,
        reply: Reply<RunPlan>,
    },
    RevisePlan {
        plan: RunPlan,
        execution: Option<Value>,
        reply: Reply<RunPlan>,
    },
    UpdatePlan {
        plan: RunPlan,
        reply: Reply<RunPlan>,
    },
    SetPlanStatus {
        plan_id: String,
        status: String,
        approval_scope: Option<String>,
        approved_by: Option<String>,
        rejection_reason: Option<String>,
        reply: Reply<RunPlan>,
    },
    RecordGrant {
        workspace_id: String,
        effect: String,
        scope: String,
        created_by: String,
        run_id: Option<String>,
        reply: Reply<PermissionGrant>,
    },
    CommitMemory {
        workspace_id: String,
        scope: String,
        scope_id: Option<String>,
        kind: String,
        content: String,
        supersedes: Vec<String>,
        contradicts: Vec<String>,
        source_authority: Option<String>,
        subject: Option<String>,
        relation: Option<String>,
        object: Option<String>,
        reply: Reply<Memory>,
    },
    UpdateMemory {
        id: String,
        content: Option<String>,
        status: Option<String>,
        valid_until: Option<String>,
        reply: Reply<Memory>,
    },
    ForgetMemory {
        id: String,
        reply: Reply<Memory>,
    },
    InstallModule {
        name: String,
        version: String,
        path: String,
        manifest: Value,
        checksum: String,
        reply: Reply<Value>,
    },
    RemoveModule {
        name: String,
        reply: Reply<()>,
    },
    MchCreate {
        name: String,
        module_name: String,
        domain: String,
        workspace_id: Option<String>,
        autostart: bool,
        manifest: Value,
        reply: Reply<Value>,
    },
    MchSetState {
        name: String,
        state: String,
        pid: Option<i64>,
        reply: Reply<Value>,
    },
    MchSetAutostart {
        name: String,
        autostart: bool,
        reply: Reply<Value>,
    },
    MchRemove {
        name: String,
        reply: Reply<()>,
    },
    CreateSlice {
        artifact_id: String,
        workspace_id: String,
        artifact_hash: String,
        media_type: String,
        selector: Value,
        content: Vec<u8>,
        reason: String,
        reply: Reply<ArtifactSlice>,
    },
    Stop {
        reply: Reply<()>,
    },
}

#[derive(Clone)]
pub struct Writer {
    tx: Sender<WriteCmd>,
}

impl Writer {
    fn call<T>(&self, make: impl FnOnce(Reply<T>) -> WriteCmd) -> Result<T> {
        let (tx, rx) = sync_channel(1);
        self.tx
            .send(make(tx))
            .map_err(|_| HliError::internal("writer actor detenido"))?;
        rx.recv()
            .map_err(|_| HliError::internal("writer actor sin respuesta"))?
    }

    pub fn create_workspace(&self, name: &str, root: &str) -> Result<Workspace> {
        self.call(|reply| WriteCmd::CreateWorkspace {
            name: name.to_string(),
            root: root.to_string(),
            reply,
        })
    }

    pub fn create_thread(&self, workspace_id: &str, title: &str) -> Result<Thread> {
        self.call(|reply| WriteCmd::CreateThread {
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            reply,
        })
    }

    pub fn create_branch(
        &self,
        thread_id: &str,
        name: &str,
        from_event: Option<String>,
    ) -> Result<Branch> {
        self.call(|reply| WriteCmd::CreateBranch {
            thread_id: thread_id.to_string(),
            name: name.to_string(),
            from_event,
            reply,
        })
    }

    pub fn append_event(
        &self,
        workspace_id: &str,
        thread_id: Option<String>,
        branch_id: Option<String>,
        kind: &str,
        payload: Value,
        idempotency_key: Option<String>,
    ) -> Result<Event> {
        self.call(|reply| WriteCmd::AppendEvent {
            workspace_id: workspace_id.to_string(),
            thread_id,
            branch_id,
            kind: kind.to_string(),
            payload,
            idempotency_key,
            reply,
        })
    }

    pub fn put_artifact(&self, workspace_id: &str, source: PathBuf) -> Result<ArtifactInfo> {
        self.call(|reply| WriteCmd::PutArtifact {
            workspace_id: workspace_id.to_string(),
            source,
            reply,
        })
    }

    pub fn snapshot(&self, branch_id: &str) -> Result<SnapshotInfo> {
        self.call(|reply| WriteCmd::Snapshot {
            branch_id: branch_id.to_string(),
            reply,
        })
    }

    pub fn export(&self, workspace_id: &str, out: PathBuf) -> Result<ExportReport> {
        self.call(|reply| WriteCmd::Export {
            workspace_id: workspace_id.to_string(),
            out,
            reply,
        })
    }

    pub fn import(&self, archive: PathBuf) -> Result<ImportReport> {
        self.call(|reply| WriteCmd::Import { archive, reply })
    }

    pub fn create_run(
        &self,
        workspace_id: &str,
        thread_id: &str,
        branch_id: &str,
        prompt: &str,
    ) -> Result<Run> {
        self.call(|reply| WriteCmd::CreateRun {
            workspace_id: workspace_id.to_string(),
            thread_id: thread_id.to_string(),
            branch_id: branch_id.to_string(),
            prompt: prompt.to_string(),
            reply,
        })
    }

    pub fn save_manifest(&self, manifest: ContextManifest) -> Result<ContextManifest> {
        self.call(|reply| WriteCmd::SaveManifest { manifest, reply })
    }

    pub fn run_event(&self, run_id: &str, kind: &str, payload: Value) -> Result<Event> {
        self.call(|reply| WriteCmd::RunEvent {
            run_id: run_id.to_string(),
            kind: kind.to_string(),
            payload,
            reply,
        })
    }

    pub fn set_run_state(&self, run_id: &str, state: &str) -> Result<Run> {
        self.call(|reply| WriteCmd::SetRunState {
            run_id: run_id.to_string(),
            state: state.to_string(),
            reply,
        })
    }

    pub fn finish_run(
        &self,
        run_id: &str,
        state: &str,
        result: Option<Value>,
        receipt: Option<Receipt>,
    ) -> Result<Run> {
        self.call(|reply| WriteCmd::FinishRun {
            run_id: run_id.to_string(),
            state: state.to_string(),
            result,
            receipt,
            reply,
        })
    }

    pub fn save_plan(&self, plan: RunPlan, execution: Option<Value>) -> Result<RunPlan> {
        self.call(|reply| WriteCmd::SavePlan {
            plan,
            execution,
            reply,
        })
    }

    /// Inserta una **nueva revisión** del plan. La anterior se conserva intacta.
    pub fn revise_plan(&self, plan: RunPlan, execution: Option<Value>) -> Result<RunPlan> {
        self.call(|reply| WriteCmd::RevisePlan {
            plan,
            execution,
            reply,
        })
    }

    pub fn update_plan(&self, plan: RunPlan) -> Result<RunPlan> {
        self.call(|reply| WriteCmd::UpdatePlan { plan, reply })
    }

    pub fn set_plan_status(
        &self,
        plan_id: &str,
        status: &str,
        approval_scope: Option<String>,
        approved_by: Option<String>,
        rejection_reason: Option<String>,
    ) -> Result<RunPlan> {
        self.call(|reply| WriteCmd::SetPlanStatus {
            plan_id: plan_id.to_string(),
            status: status.to_string(),
            approval_scope,
            approved_by,
            rejection_reason,
            reply,
        })
    }

    pub fn record_grant(
        &self,
        workspace_id: &str,
        effect: &str,
        scope: &str,
        created_by: &str,
        run_id: Option<String>,
    ) -> Result<PermissionGrant> {
        self.call(|reply| WriteCmd::RecordGrant {
            workspace_id: workspace_id.to_string(),
            effect: effect.to_string(),
            scope: scope.to_string(),
            created_by: created_by.to_string(),
            run_id,
            reply,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_memory(
        &self,
        workspace_id: &str,
        scope: &str,
        scope_id: Option<String>,
        kind: &str,
        content: &str,
        supersedes: Vec<String>,
        contradicts: Vec<String>,
        source_authority: Option<String>,
        subject: Option<String>,
        relation: Option<String>,
        object: Option<String>,
    ) -> Result<Memory> {
        self.call(|reply| WriteCmd::CommitMemory {
            workspace_id: workspace_id.to_string(),
            scope: scope.to_string(),
            scope_id,
            kind: kind.to_string(),
            content: content.to_string(),
            supersedes,
            contradicts,
            source_authority,
            subject,
            relation,
            object,
            reply,
        })
    }

    pub fn update_memory(
        &self,
        id: &str,
        content: Option<String>,
        status: Option<String>,
        valid_until: Option<String>,
    ) -> Result<Memory> {
        self.call(|reply| WriteCmd::UpdateMemory {
            id: id.to_string(),
            content,
            status,
            valid_until,
            reply,
        })
    }

    pub fn forget_memory(&self, id: &str) -> Result<Memory> {
        self.call(|reply| WriteCmd::ForgetMemory {
            id: id.to_string(),
            reply,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn install_module(
        &self,
        name: &str,
        version: &str,
        path: &str,
        manifest: Value,
        checksum: &str,
    ) -> Result<Value> {
        self.call(|reply| WriteCmd::InstallModule {
            name: name.to_string(),
            version: version.to_string(),
            path: path.to_string(),
            manifest,
            checksum: checksum.to_string(),
            reply,
        })
    }

    pub fn remove_module(&self, name: &str) -> Result<()> {
        self.call(|reply| WriteCmd::RemoveModule {
            name: name.to_string(),
            reply,
        })
    }

    pub fn mch_create(
        &self,
        name: &str,
        module_name: &str,
        domain: &str,
        workspace_id: Option<String>,
        autostart: bool,
        manifest: Value,
    ) -> Result<Value> {
        self.call(|reply| WriteCmd::MchCreate {
            name: name.to_string(),
            module_name: module_name.to_string(),
            domain: domain.to_string(),
            workspace_id,
            autostart,
            manifest,
            reply,
        })
    }

    pub fn mch_set_state(&self, name: &str, state: &str, pid: Option<i64>) -> Result<Value> {
        self.call(|reply| WriteCmd::MchSetState {
            name: name.to_string(),
            state: state.to_string(),
            pid,
            reply,
        })
    }

    pub fn mch_set_autostart(&self, name: &str, autostart: bool) -> Result<Value> {
        self.call(|reply| WriteCmd::MchSetAutostart {
            name: name.to_string(),
            autostart,
            reply,
        })
    }

    pub fn mch_remove(&self, name: &str) -> Result<()> {
        self.call(|reply| WriteCmd::MchRemove {
            name: name.to_string(),
            reply,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_slice(
        &self,
        artifact_id: &str,
        workspace_id: &str,
        artifact_hash: &str,
        media_type: &str,
        selector: Value,
        content: Vec<u8>,
        reason: &str,
    ) -> Result<ArtifactSlice> {
        self.call(|reply| WriteCmd::CreateSlice {
            artifact_id: artifact_id.to_string(),
            workspace_id: workspace_id.to_string(),
            artifact_hash: artifact_hash.to_string(),
            media_type: media_type.to_string(),
            selector,
            content,
            reason: reason.to_string(),
            reply,
        })
    }

    pub fn stop(&self) -> Result<()> {
        self.call(|reply| WriteCmd::Stop { reply })
    }
}

struct State {
    conn: Connection,
    artifacts_dir: PathBuf,
    snapshots_dir: PathBuf,
}

pub fn spawn(conn: Connection, artifacts_dir: PathBuf, snapshots_dir: PathBuf) -> Writer {
    let (tx, rx) = channel::<WriteCmd>();
    thread::Builder::new()
        .name("hli-writer".into())
        .spawn(move || {
            let mut st = State {
                conn,
                artifacts_dir,
                snapshots_dir,
            };
            while let Ok(cmd) = rx.recv() {
                let stop = matches!(cmd, WriteCmd::Stop { .. });
                st.dispatch(cmd);
                if stop {
                    break;
                }
            }
        })
        .expect("no se pudo arrancar el writer actor");
    Writer { tx }
}

impl State {
    fn dispatch(&mut self, cmd: WriteCmd) {
        match cmd {
            WriteCmd::CreateWorkspace { name, root, reply } => {
                let _ = reply.send(self.create_workspace(&name, &root));
            }
            WriteCmd::CreateThread {
                workspace_id,
                title,
                reply,
            } => {
                let _ = reply.send(self.create_thread(&workspace_id, &title));
            }
            WriteCmd::CreateBranch {
                thread_id,
                name,
                from_event,
                reply,
            } => {
                let _ = reply.send(self.create_branch(&thread_id, &name, from_event));
            }
            WriteCmd::AppendEvent {
                workspace_id,
                thread_id,
                branch_id,
                kind,
                payload,
                idempotency_key,
                reply,
            } => {
                let r = (|| {
                    let tx = self.conn.transaction()?;
                    let ev = append_event(
                        &tx,
                        &workspace_id,
                        thread_id.as_deref(),
                        branch_id.as_deref(),
                        &kind,
                        json!({"kind": "user", "id": "cli"}),
                        payload,
                        vec![],
                        idempotency_key.as_deref(),
                    )?;
                    tx.commit()?;
                    Ok(ev)
                })();
                let _ = reply.send(r);
            }
            WriteCmd::PutArtifact {
                workspace_id,
                source,
                reply,
            } => {
                let _ = reply.send(self.put_artifact(&workspace_id, &source));
            }
            WriteCmd::Snapshot { branch_id, reply } => {
                let _ = reply.send(self.snapshot(&branch_id));
            }
            WriteCmd::Export {
                workspace_id,
                out,
                reply,
            } => {
                let _ = reply.send(self.export(&workspace_id, &out));
            }
            WriteCmd::Import { archive, reply } => {
                let _ = reply.send(self.import(&archive));
            }
            WriteCmd::CreateRun {
                workspace_id,
                thread_id,
                branch_id,
                prompt,
                reply,
            } => {
                let _ = reply.send(self.create_run(&workspace_id, &thread_id, &branch_id, &prompt));
            }
            WriteCmd::SaveManifest { manifest, reply } => {
                let _ = reply.send(self.save_manifest(manifest));
            }
            WriteCmd::RunEvent {
                run_id,
                kind,
                payload,
                reply,
            } => {
                let _ = reply.send(self.run_event(&run_id, &kind, payload));
            }
            WriteCmd::SetRunState {
                run_id,
                state,
                reply,
            } => {
                let out = self.set_run_state(&run_id, &state);
                let _ = reply.send(out);
            }
            WriteCmd::FinishRun {
                run_id,
                state,
                result,
                receipt,
                reply,
            } => {
                let _ = reply.send(self.finish_run(&run_id, &state, result, receipt));
            }
            WriteCmd::SavePlan {
                plan,
                execution,
                reply,
            } => {
                let _ = reply.send(self.save_plan(plan, execution));
            }
            WriteCmd::RevisePlan {
                plan,
                execution,
                reply,
            } => {
                let _ = reply.send(self.revise_plan(plan, execution));
            }
            WriteCmd::UpdatePlan { plan, reply } => {
                let _ = reply.send(self.update_plan(plan));
            }
            WriteCmd::SetPlanStatus {
                plan_id,
                status,
                approval_scope,
                approved_by,
                rejection_reason,
                reply,
            } => {
                let _ = reply.send(self.set_plan_status(
                    &plan_id,
                    &status,
                    approval_scope,
                    approved_by,
                    rejection_reason,
                ));
            }
            WriteCmd::RecordGrant {
                workspace_id,
                effect,
                scope,
                created_by,
                run_id,
                reply,
            } => {
                let _ = reply.send(self.record_grant(
                    &workspace_id,
                    &effect,
                    &scope,
                    &created_by,
                    run_id,
                ));
            }
            WriteCmd::CommitMemory {
                workspace_id,
                scope,
                scope_id,
                kind,
                content,
                supersedes,
                contradicts,
                source_authority,
                subject,
                relation,
                object,
                reply,
            } => {
                let _ = reply.send(self.commit_memory(
                    &workspace_id,
                    &scope,
                    scope_id,
                    &kind,
                    &content,
                    supersedes,
                    contradicts,
                    source_authority,
                    subject,
                    relation,
                    object,
                ));
            }
            WriteCmd::UpdateMemory {
                id,
                content,
                status,
                valid_until,
                reply,
            } => {
                let _ = reply.send(self.update_memory(&id, content, status, valid_until));
            }
            WriteCmd::ForgetMemory { id, reply } => {
                let _ = reply.send(self.forget_memory(&id));
            }
            WriteCmd::InstallModule {
                name,
                version,
                path,
                manifest,
                checksum,
                reply,
            } => {
                let _ =
                    reply.send(self.install_module(&name, &version, &path, manifest, &checksum));
            }
            WriteCmd::RemoveModule { name, reply } => {
                let _ = reply.send(self.remove_module(&name));
            }
            WriteCmd::MchCreate {
                name,
                module_name,
                domain,
                workspace_id,
                autostart,
                manifest,
                reply,
            } => {
                let _ = reply.send(self.mch_create(
                    &name,
                    &module_name,
                    &domain,
                    workspace_id,
                    autostart,
                    manifest,
                ));
            }
            WriteCmd::MchSetState {
                name,
                state,
                pid,
                reply,
            } => {
                let _ = reply.send(self.mch_set_state(&name, &state, pid));
            }
            WriteCmd::MchSetAutostart {
                name,
                autostart,
                reply,
            } => {
                let _ = reply.send(self.mch_set_autostart(&name, autostart));
            }
            WriteCmd::MchRemove { name, reply } => {
                let _ = reply.send(self.mch_remove(&name));
            }
            WriteCmd::CreateSlice {
                artifact_id,
                workspace_id,
                artifact_hash,
                media_type,
                selector,
                content,
                reason,
                reply,
            } => {
                let _ = reply.send(self.create_slice(
                    &artifact_id,
                    &workspace_id,
                    &artifact_hash,
                    &media_type,
                    selector,
                    content,
                    &reason,
                ));
            }
            WriteCmd::Stop { reply } => {
                let _ = reply.send(Ok(()));
            }
        }
    }

    fn create_workspace(&mut self, name: &str, root: &str) -> Result<Workspace> {
        if name.trim().is_empty() {
            return Err(HliError::invalid(
                "el nombre del workspace no puede estar vacío",
            ));
        }
        let tx = self.conn.transaction()?;
        let ws = Workspace {
            id: new_id("ws"),
            name: name.to_string(),
            root_path: root.to_string(),
            created_at: now_iso(),
            archived_at: None,
            config: json!({}),
        };
        tx.execute(
            "INSERT INTO workspaces(id, name, root_path, created_at, archived_at, config_json)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![
                ws.id,
                ws.name,
                ws.root_path,
                ws.created_at,
                canonical_json(&ws.config)
            ],
        )?;
        append_event(
            &tx,
            &ws.id,
            None,
            None,
            "workspace.created",
            json!({"kind": "user", "id": "cli"}),
            json!({"name": ws.name, "root_path": ws.root_path}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(ws)
    }

    fn create_thread(&mut self, workspace_id: &str, title: &str) -> Result<Thread> {
        schema::require_workspace(&self.conn, workspace_id)?;
        if title.trim().is_empty() {
            return Err(HliError::invalid(
                "el título del thread no puede estar vacío",
            ));
        }
        let tx = self.conn.transaction()?;
        let th = Thread {
            id: new_id("thr"),
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            created_at: now_iso(),
            archived_at: None,
        };
        tx.execute(
            "INSERT INTO threads(id, workspace_id, title, created_at, archived_at)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            params![th.id, th.workspace_id, th.title, th.created_at],
        )?;
        append_event(
            &tx,
            workspace_id,
            Some(&th.id),
            None,
            "thread.created",
            json!({"kind": "user", "id": "cli"}),
            json!({"title": th.title}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(th)
    }

    fn create_branch(
        &mut self,
        thread_id: &str,
        name: &str,
        from_event: Option<String>,
    ) -> Result<Branch> {
        let (workspace_id, _title): (String, String) = self
            .conn
            .query_row(
                "SELECT workspace_id, title FROM threads WHERE id = ?1",
                [thread_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("thread {thread_id}")))?;

        let mut head_event_id = None;
        let mut head_sequence = 0i64;
        let mut parent_branch_id = None;
        if let Some(ev_id) = &from_event {
            let ev = load_event(&self.conn, ev_id)?
                .ok_or_else(|| HliError::not_found(&format!("event {ev_id}")))?;
            if ev.workspace_id != workspace_id {
                return Err(HliError::conflict(
                    "el evento de bifurcación pertenece a otro workspace",
                ));
            }
            parent_branch_id = ev.branch_id.clone();
            head_event_id = Some(ev.event_id.clone());
            head_sequence = ev.sequence;
        }

        let tx = self.conn.transaction()?;
        let br = Branch {
            id: new_id("br"),
            thread_id: thread_id.to_string(),
            name: name.to_string(),
            parent_branch_id,
            fork_event_id: head_event_id.clone(),
            head_event_id: head_event_id.clone(),
            head_sequence,
            snapshot_id: None,
            created_at: now_iso(),
        };
        tx.execute(
            "INSERT INTO branches(id, thread_id, name, parent_branch_id, fork_event_id,
                                  head_event_id, head_sequence, snapshot_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
            params![
                br.id,
                br.thread_id,
                name,
                br.parent_branch_id,
                br.fork_event_id,
                br.head_event_id,
                br.head_sequence,
                br.created_at
            ],
        )?;
        append_event(
            &tx,
            &workspace_id,
            Some(thread_id),
            Some(&br.id),
            "branch.created",
            json!({"kind": "user", "id": "cli"}),
            json!({"name": name, "from_event": from_event}),
            vec![],
            None,
        )?;
        tx.commit()?;
        // Snapshot obligatorio al ramificar (§16.1 de la especificación).
        let snap = self.snapshot(&br.id)?;
        self.conn.execute(
            "UPDATE branches SET snapshot_id = ?1 WHERE id = ?2",
            params![snap.id, br.id],
        )?;
        let mut br = br;
        br.snapshot_id = Some(snap.id);
        Ok(br)
    }

    fn create_run(
        &mut self,
        workspace_id: &str,
        thread_id: &str,
        branch_id: &str,
        prompt: &str,
    ) -> Result<Run> {
        schema::require_workspace(&self.conn, workspace_id)?;
        let (th_ws, br_thread): (String, String) = self
            .conn
            .query_row(
                "SELECT workspace_id, id FROM threads WHERE id = ?1",
                [thread_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("thread {thread_id}")))?;
        if th_ws != workspace_id {
            return Err(HliError::conflict("el thread pertenece a otro workspace"));
        }
        if br_thread != thread_id {
            return Err(HliError::conflict("la branch pertenece a otro thread"));
        }
        let revision: i64 = self
            .conn
            .query_row(
                "SELECT head_sequence FROM branches WHERE id = ?1 AND thread_id = ?2",
                params![branch_id, thread_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("branch {branch_id}")))?;

        let tx = self.conn.transaction()?;
        let run = Run {
            id: new_id("run"),
            workspace_id: workspace_id.to_string(),
            thread_id: thread_id.to_string(),
            branch_id: branch_id.to_string(),
            branch_revision: revision,
            state: "queued".into(),
            prompt: prompt.to_string(),
            started_at: now_iso(),
            ended_at: None,
            result: None,
            receipt: None,
        };
        tx.execute(
            "INSERT INTO runs(id, workspace_id, thread_id, branch_id, branch_revision, state,
                              prompt, started_at, ended_at, result_json, receipt_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, NULL)",
            params![
                run.id,
                run.workspace_id,
                run.thread_id,
                run.branch_id,
                run.branch_revision,
                run.state,
                run.prompt,
                run.started_at
            ],
        )?;
        append_event(
            &tx,
            workspace_id,
            Some(thread_id),
            Some(branch_id),
            "run.queued",
            json!({"kind": "agent", "id": "hli"}),
            json!({"run_id": run.id, "branch_revision": run.branch_revision}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(run)
    }

    fn save_manifest(&mut self, manifest: ContextManifest) -> Result<ContextManifest> {
        let mut m = manifest;
        m.seal();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO context_manifests(id, workspace_id, run_id, branch_revision, budget_tokens,
                                           estimated_tokens, content_hash, body_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![m.manifest_id, m.workspace_id, m.run_id, m.branch_revision, m.budget_tokens,
                    m.estimated_tokens, m.content_hash, canonical_json(&serde_json::to_value(&m).unwrap_or(json!({}))), m.created_at],
        )?;
        append_event(
            &tx,
            &m.workspace_id,
            None,
            None,
            "context.manifest.created",
            json!({"kind": "agent", "id": "hli"}),
            json!({"run_id": m.run_id, "manifest_id": m.manifest_id,
                   "content_hash": m.content_hash, "estimated_tokens": m.estimated_tokens}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(m)
    }

    fn run_event(&mut self, run_id: &str, kind: &str, payload: Value) -> Result<Event> {
        let (ws, th, br): (String, String, String) = self
            .conn
            .query_row(
                "SELECT workspace_id, thread_id, branch_id FROM runs WHERE id = ?1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("run {run_id}")))?;
        let tx = self.conn.transaction()?;
        let mut payload = payload;
        if payload.get("run_id").is_none()
            && let Some(obj) = payload.as_object_mut()
        {
            obj.insert("run_id".into(), json!(run_id));
        }
        let ev = append_event(
            &tx,
            &ws,
            Some(&th),
            Some(&br),
            kind,
            json!({"kind": "agent", "id": "hli"}),
            payload,
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(ev)
    }

    fn set_run_state(&mut self, run_id: &str, state: &str) -> Result<Run> {
        let run = load_run(&self.conn, run_id)?
            .ok_or_else(|| HliError::not_found(&format!("run {run_id}")))?;
        if !hli_core::model::run_transition_allowed(&run.state, state) {
            return Err(HliError::conflict(format!(
                "transicion de run invalida: {} -> {state}",
                run.state
            )));
        }
        self.conn.execute(
            "UPDATE runs SET state = ?1 WHERE id = ?2",
            params![state, run_id],
        )?;
        self.run_event(run_id, &format!("run.{state}"), json!({"state": state}))?;
        load_run(&self.conn, run_id)?.ok_or_else(|| HliError::internal("run desaparecido"))
    }

    fn finish_run(
        &mut self,
        run_id: &str,
        state: &str,
        result: Option<Value>,
        receipt: Option<Receipt>,
    ) -> Result<Run> {
        let run = load_run(&self.conn, run_id)?
            .ok_or_else(|| HliError::not_found(&format!("run {run_id}")))?;
        if !hli_core::model::run_transition_allowed(&run.state, state) {
            return Err(HliError::conflict(format!(
                "transicion de run invalida: {} -> {state}",
                run.state
            )));
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE runs SET state = ?1, ended_at = ?2, result_json = ?3, receipt_json = ?4 WHERE id = ?5",
            params![
                state,
                now_iso(),
                result.as_ref().map(canonical_json),
                receipt.as_ref().map(|r| canonical_json(&serde_json::to_value(r).unwrap_or(json!({})))),
                run_id
            ],
        )?;
        let ev = append_event(
            &tx,
            &run.workspace_id,
            Some(&run.thread_id),
            Some(&run.branch_id),
            &format!("run.{state}"),
            json!({"kind": "agent", "id": "hli"}),
            json!({"run_id": run.id, "state": state,
                   "receipt": receipt.is_some(), "result": result.is_some()}),
            vec![],
            None,
        )?;

        // Cierre causal: el receipt referencia el evento terminal, que ya existe.
        let mut receipt = receipt;
        if let Some(r) = receipt.as_mut() {
            if r.terminal_event.is_none() {
                r.terminal_event = Some(ev.event_id.clone());
            }
            let body = canonical_json(&serde_json::to_value(r).unwrap_or(json!({})));
            tx.execute(
                "UPDATE runs SET receipt_json = ?1 WHERE id = ?2",
                params![body, run_id],
            )?;
        }
        tx.commit()?;
        let mut out =
            load_run(&self.conn, run_id)?.ok_or_else(|| HliError::internal("run desaparecido"))?;
        if let Some(r) = receipt {
            out.receipt = Some(r);
        }
        Ok(out)
    }

    fn save_plan(&mut self, mut plan: RunPlan, execution: Option<Value>) -> Result<RunPlan> {
        if !plan_status_valid(&plan.status) {
            return Err(HliError::invalid(format!(
                "estado de plan inválido: {}",
                plan.status
            )));
        }
        if plan.origin != "provider" && plan.origin != "runtime_generated" {
            return Err(HliError::invalid(format!(
                "origen de plan inválido: {}",
                plan.origin
            )));
        }
        plan.seal();
        let tx = self.conn.transaction()?;
        upsert_plan_row(&tx, &plan, execution.as_ref())?;
        append_event(
            &tx,
            &plan.workspace_id,
            Some(&plan.thread_id),
            Some(&plan.branch_id),
            "plan.created",
            json!({"kind": "agent", "id": "hli"}),
            json!({
                "run_id": plan.run_id,
                "plan_id": plan.plan_id,
                "origin": plan.origin,
                "status": plan.status,
                "content_hash": plan.content_hash,
                "requested_effects": plan.requested_effects,
                "steps": plan.steps.len()
            }),
            vec![],
            Some(&format!("plan.created:{}", plan.plan_id)),
        )?;
        tx.commit()?;
        Ok(plan)
    }

    /// Nueva revisión del plan para el mismo run. No modifica la anterior y
    /// exige que `supersedes_plan_id` apunte a la vigente.
    fn revise_plan(&mut self, mut plan: RunPlan, execution: Option<Value>) -> Result<RunPlan> {
        if plan.revision < 2 {
            return Err(HliError::invalid(
                "una revisión nueva debe tener revision >= 2",
            ));
        }
        let Some(prev_id) = plan.supersedes_plan_id.clone() else {
            return Err(HliError::invalid("falta supersedes_plan_id"));
        };
        let prev = load_plan(&self.conn, &prev_id)?
            .ok_or_else(|| HliError::not_found(&format!("plan anterior {prev_id}")))?;
        if prev.run_id != plan.run_id {
            return Err(HliError::conflict("el plan anterior pertenece a otro run"));
        }
        if plan.revision != prev.revision + 1 {
            return Err(HliError::conflict(format!(
                "revisión no consecutiva: {} tras {}",
                plan.revision, prev.revision
            )));
        }
        if plan.status != "waiting_approval" {
            return Err(HliError::invalid(
                "una revisión replanificada nace en waiting_approval",
            ));
        }
        plan.seal();
        let tx = self.conn.transaction()?;
        insert_plan_row(&tx, &plan, execution.as_ref())?;
        append_event(
            &tx,
            &plan.workspace_id,
            Some(&plan.thread_id),
            Some(&plan.branch_id),
            "plan.revised",
            json!({"kind": "agent", "id": "hli"}),
            json!({
                "run_id": plan.run_id,
                "plan_id": plan.plan_id,
                "revision": plan.revision,
                "supersedes_plan_id": plan.supersedes_plan_id,
                "invalidation_reasons": plan.invalidation_reasons,
                "content_hash": plan.content_hash,
                "status": plan.status
            }),
            vec![],
            Some(&format!("plan.revised:{}", plan.plan_id)),
        )?;
        tx.commit()?;
        Ok(plan)
    }

    fn update_plan(&mut self, mut plan: RunPlan) -> Result<RunPlan> {
        plan.seal();
        let tx = self.conn.transaction()?;
        upsert_plan_row(&tx, &plan, None)?;
        tx.commit()?;
        Ok(plan)
    }

    fn set_plan_status(
        &mut self,
        plan_id: &str,
        status: &str,
        approval_scope: Option<String>,
        approved_by: Option<String>,
        rejection_reason: Option<String>,
    ) -> Result<RunPlan> {
        if !plan_status_valid(status) {
            return Err(HliError::invalid(format!(
                "estado de plan inválido: {status}"
            )));
        }
        if let Some(scope) = &approval_scope
            && !approval_scope_valid(scope)
        {
            return Err(HliError::invalid(format!(
                "scope de aprobación inválido: {scope}"
            )));
        }
        let mut plan = match load_plan(&self.conn, plan_id)? {
            Some(p) => p,
            None => return Err(HliError::not_found(&format!("plan {plan_id}"))),
        };
        // Idempotencia: repetir la misma transición devuelve el estado actual
        // sin emitir un segundo evento.
        if plan.status == status {
            return Ok(plan);
        }
        if !plan_transition_allowed(&plan.status, status) {
            return Err(HliError::conflict(format!(
                "transición de plan inválida: {} -> {status}",
                plan.status
            )));
        }
        let now = now_iso();
        plan.status = status.to_string();
        match status {
            "approved" => {
                plan.approved_at = Some(now.clone());
                plan.approved_by = approved_by.clone();
                plan.approval_scope = approval_scope.clone();
                plan.rejected_at = None;
                plan.rejection_reason = None;
            }
            "rejected" => {
                plan.rejected_at = Some(now.clone());
                plan.rejection_reason = rejection_reason.clone();
                plan.approval_scope = approval_scope.clone();
            }
            "cancelled" => {
                plan.rejection_reason = rejection_reason.clone();
            }
            _ => {}
        }
        plan.seal();

        let kind = match status {
            "approved" => "plan.approved",
            "rejected" => "plan.rejected",
            "cancelled" => "run.approval_cancelled",
            "executing" => "plan.execution_started",
            "completed" => "plan.completed",
            "failed" => "plan.failed",
            _ => "plan.updated",
        };
        let mut payload = json!({
            "run_id": plan.run_id,
            "plan_id": plan.plan_id,
            "status": plan.status,
            "content_hash": plan.content_hash,
            "timestamp": now
        });
        if let Some(scope) = &plan.approval_scope {
            payload["scope"] = json!(scope);
        }
        if let Some(user) = &plan.approved_by {
            payload["approved_by"] = json!(user);
        }
        if status == "approved" {
            payload["approved_effects"] = json!(plan.effectful_requests());
        }
        if let Some(reason) = &plan.rejection_reason {
            payload["reason"] = json!(reason);
        }
        if status == "rejected" {
            payload["rejected_effects"] = json!(plan.effectful_requests());
        }

        let tx = self.conn.transaction()?;
        upsert_plan_row(&tx, &plan, None)?;
        append_event(
            &tx,
            &plan.workspace_id,
            Some(&plan.thread_id),
            Some(&plan.branch_id),
            kind,
            json!({"kind": "agent", "id": "hli"}),
            payload,
            vec![],
            Some(&format!("{kind}:{}:{status}", plan.plan_id)),
        )?;
        tx.commit()?;
        Ok(plan)
    }

    fn record_grant(
        &mut self,
        workspace_id: &str,
        effect: &str,
        scope: &str,
        created_by: &str,
        run_id: Option<String>,
    ) -> Result<PermissionGrant> {
        if !plan_effect_is_known(effect) {
            return Err(HliError::invalid(format!("efecto inválido: {effect}")));
        }
        schema::require_workspace(&self.conn, workspace_id)?;
        let now = now_iso();
        self.conn.execute(
            "INSERT INTO permission_grants(grant_id, workspace_id, effect, scope, created_at, created_by, run_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(workspace_id, effect, scope) DO NOTHING",
            params![new_id("grant"), workspace_id, effect, scope, now, created_by, run_id],
        )?;
        let grant = self.conn.query_row(
            "SELECT grant_id, workspace_id, effect, scope, created_at, created_by, run_id
                 FROM permission_grants WHERE workspace_id = ?1 AND effect = ?2 AND scope = ?3",
            params![workspace_id, effect, scope],
            row_to_grant,
        )?;
        Ok(grant)
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_memory(
        &mut self,
        workspace_id: &str,
        scope: &str,
        scope_id: Option<String>,
        kind: &str,
        content: &str,
        supersedes: Vec<String>,
        contradicts: Vec<String>,
        source_authority: Option<String>,
        subject: Option<String>,
        relation: Option<String>,
        object: Option<String>,
    ) -> Result<Memory> {
        schema::require_workspace(&self.conn, workspace_id)?;
        const KINDS: [&str; 6] = [
            "fact",
            "decision",
            "constraint",
            "summary",
            "reference",
            "procedure",
        ];
        const SCOPES: [&str; 4] = ["global", "workspace", "thread", "branch"];
        if !KINDS.contains(&kind) {
            return Err(HliError::invalid(format!(
                "tipo de memoria inválido: {kind}"
            )));
        }
        if !SCOPES.contains(&scope) {
            return Err(HliError::invalid(format!("ámbito inválido: {scope}")));
        }
        if content.trim().is_empty() {
            return Err(HliError::invalid("la memoria no puede estar vacía"));
        }
        let now = now_iso();
        let m = Memory {
            id: new_id("mem"),
            workspace_id: workspace_id.to_string(),
            scope: scope.to_string(),
            scope_id,
            kind: kind.to_string(),
            status: "active".into(),
            content: content.to_string(),
            observed_at: now.clone(),
            valid_from: now.clone(),
            valid_until: None,
            superseded_at: None,
            source_authority: source_authority.unwrap_or_else(|| "user".into()),
            subject,
            relation,
            object,
            supersedes: supersedes.clone(),
            contradicts,
            sensitivity: "internal".into(),
            authorship: json!({"kind": "user", "id": "cli"}),
            source_events: vec![],
            created_at: now_iso(),
        };
        let tx = self.conn.transaction()?;
        // Validar supersedes antes de insertar: atomicidad o nada.
        for old in &supersedes {
            let exists: i64 = tx.query_row(
                "SELECT COUNT(*) FROM memories WHERE id = ?1 AND workspace_id = ?2",
                params![old, workspace_id],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(HliError::not_found(&format!("memoria a superseder {old}")));
            }
        }
        tx.execute(
            "INSERT INTO memories(id, workspace_id, scope, scope_id, type, status, content,
                                  observed_at, valid_from, valid_until, superseded_at,
                                  source_authority, subject, relation, object,
                                  supersedes_json, contradicts_json,
                                  sensitivity, authorship_json, source_events, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10, ?11, ?12, ?13,
                     ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                m.id,
                m.workspace_id,
                m.scope,
                m.scope_id,
                m.kind,
                m.status,
                m.content,
                m.observed_at,
                m.valid_from,
                m.source_authority,
                m.subject,
                m.relation,
                m.object,
                canonical_json(&json!(m.supersedes)),
                canonical_json(&json!(m.contradicts)),
                m.sensitivity,
                canonical_json(&m.authorship),
                canonical_json(&json!(m.source_events)),
                m.created_at
            ],
        )?;
        for old in &supersedes {
            tx.execute(
                "UPDATE memories SET status = 'superseded', superseded_at = ?1, valid_until = ?1
                 WHERE id = ?2 AND workspace_id = ?3",
                params![now, old, workspace_id],
            )?;
            tx.execute("DELETE FROM memories_fts WHERE mem_id = ?1", [old])?;
        }
        tx.execute(
            "INSERT INTO memories_fts(mem_id, content, workspace_id) VALUES (?1, ?2, ?3)",
            params![m.id, m.content, m.workspace_id],
        )?;
        append_event(
            &tx,
            workspace_id,
            None,
            None,
            "memory.committed",
            json!({"kind": "user", "id": "cli"}),
            json!({"memory_id": m.id, "type": m.kind, "scope": m.scope,
                   "content_hash": hli_core::canon::hash_canonical(&json!(m.content))}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(m)
    }

    fn update_memory(
        &mut self,
        id: &str,
        content: Option<String>,
        status: Option<String>,
        valid_until: Option<String>,
    ) -> Result<Memory> {
        let mut m = load_memory(&self.conn, id)?
            .ok_or_else(|| HliError::not_found(&format!("memoria {id}")))?;
        if let Some(c) = content {
            if c.trim().is_empty() {
                return Err(HliError::invalid("la memoria no puede estar vacía"));
            }
            m.content = c;
        }
        if let Some(s) = status {
            const STATES: [&str; 5] = [
                "proposed",
                "active",
                "superseded",
                "revoked",
                "deleted_by_policy",
            ];
            if !STATES.contains(&s.as_str()) {
                return Err(HliError::invalid(format!("estado inválido: {s}")));
            }
            m.status = s;
        }
        if let Some(v) = valid_until {
            m.valid_until = if v.is_empty() { None } else { Some(v) };
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE memories SET content = ?1, status = ?2, valid_until = ?3 WHERE id = ?4",
            params![m.content, m.status, m.valid_until, m.id],
        )?;
        tx.execute("DELETE FROM memories_fts WHERE mem_id = ?1", [&m.id])?;
        if m.status == "active" || m.status == "proposed" {
            tx.execute(
                "INSERT INTO memories_fts(mem_id, content, workspace_id) VALUES (?1, ?2, ?3)",
                params![m.id, m.content, m.workspace_id],
            )?;
        }
        append_event(
            &tx,
            &m.workspace_id,
            None,
            None,
            "memory.updated",
            json!({"kind": "user", "id": "cli"}),
            json!({"memory_id": m.id, "status": m.status}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(m)
    }

    fn forget_memory(&mut self, id: &str) -> Result<Memory> {
        let mut m = load_memory(&self.conn, id)?
            .ok_or_else(|| HliError::not_found(&format!("memoria {id}")))?;
        m.status = "revoked".into();
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE memories SET status = 'revoked' WHERE id = ?1",
            [&m.id],
        )?;
        tx.execute("DELETE FROM memories_fts WHERE mem_id = ?1", [&m.id])?;
        append_event(
            &tx,
            &m.workspace_id,
            None,
            None,
            "memory.forgotten",
            json!({"kind": "user", "id": "cli"}),
            json!({"memory_id": m.id}),
            vec![],
            None,
        )?;
        tx.commit()?;
        Ok(m)
    }

    fn install_module(
        &mut self,
        name: &str,
        version: &str,
        path: &str,
        manifest: Value,
        checksum: &str,
    ) -> Result<Value> {
        self.conn.execute(
            "INSERT INTO modules(name, version, path, manifest_json, checksum, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(name, version) DO UPDATE SET path=excluded.path,
                manifest_json=excluded.manifest_json, checksum=excluded.checksum",
            params![
                name,
                version,
                path,
                canonical_json(&manifest),
                checksum,
                now_iso()
            ],
        )?;
        let lock = self.conn.query_row(
            "SELECT name, version, checksum FROM modules ORDER BY name, version",
            [],
            |_| Ok(()),
        );
        let _ = lock;
        Ok(json!({"name": name, "version": version, "path": path, "checksum": checksum}))
    }

    fn remove_module(&mut self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM modules WHERE name = ?1", [name])?;
        Ok(())
    }

    fn mch_create(
        &mut self,
        name: &str,
        module_name: &str,
        domain: &str,
        workspace_id: Option<String>,
        autostart: bool,
        manifest: Value,
    ) -> Result<Value> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO mch_profiles(name, module_name, domain, manifest_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET module_name=excluded.module_name,
                domain=excluded.domain, manifest_json=excluded.manifest_json",
            params![
                name,
                module_name,
                domain,
                canonical_json(&manifest),
                now_iso()
            ],
        )?;
        tx.execute(
            "INSERT INTO mch_instances(name, profile, workspace_id, state, pid, autostart, created_at)
             VALUES (?1, ?1, ?2, 'configured', NULL, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET workspace_id=excluded.workspace_id,
                state='configured', autostart=excluded.autostart",
            params![name, workspace_id, autostart as i64, now_iso()],
        )?;
        tx.commit()?;
        Ok(json!({
            "profile": name,
            "instance": name,
            "module": module_name,
            "domain": domain,
            "autostart": autostart
        }))
    }

    fn mch_set_state(&mut self, name: &str, state: &str, pid: Option<i64>) -> Result<Value> {
        self.conn.execute(
            "UPDATE mch_instances SET state = ?1, pid = ?2 WHERE name = ?3",
            params![state, pid, name],
        )?;
        Ok(json!({"instance": name, "state": state, "pid": pid}))
    }

    fn mch_set_autostart(&mut self, name: &str, autostart: bool) -> Result<Value> {
        let changed = self.conn.execute(
            "UPDATE mch_instances SET autostart = ?1 WHERE name = ?2",
            params![autostart as i64, name],
        )?;
        if changed == 0 {
            return Err(HliError::not_found(&format!("MCH {name}")));
        }
        Ok(json!({"instance": name, "autostart": autostart}))
    }

    fn mch_remove(&mut self, name: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM mch_instances WHERE name = ?1", [name])?;
        self.conn
            .execute("DELETE FROM mch_profiles WHERE name = ?1", [name])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn create_slice(
        &mut self,
        artifact_id: &str,
        workspace_id: &str,
        artifact_hash: &str,
        media_type: &str,
        selector: Value,
        content: Vec<u8>,
        reason: &str,
    ) -> Result<ArtifactSlice> {
        let slice = ArtifactSlice {
            slice_id: new_id("sl"),
            artifact_id: artifact_id.to_string(),
            artifact_hash: artifact_hash.to_string(),
            media_type: media_type.to_string(),
            selector: selector.clone(),
            slice_hash: format!("sha256:{}", crate::export::sha256_hex(&content)),
            estimated_tokens: ((content.len() as i64) + 3) / 4,
            reason: reason.to_string(),
        };
        self.conn.execute(
            "INSERT INTO artifact_slices(id, artifact_id, workspace_id, selector_json, media_type,
                                         slice_hash, bytes, content, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                slice.slice_id,
                artifact_id,
                workspace_id,
                canonical_json(&selector),
                media_type,
                slice.slice_hash,
                content.len() as i64,
                content,
                reason,
                now_iso()
            ],
        )?;
        Ok(slice)
    }

    fn put_artifact(&mut self, workspace_id: &str, source: &PathBuf) -> Result<ArtifactInfo> {
        schema::require_workspace(&self.conn, workspace_id)?;
        let bytes = std::fs::read(source)?;
        let sha = export::sha256_hex(&bytes);
        let mime = export::guess_mime(&source.to_string_lossy());
        let sha_ref = format!("sha256:{sha}");

        if let Some((id, path)) = self
            .conn
            .query_row(
                "SELECT id, sha256 FROM artifacts WHERE workspace_id = ?1 AND sha256 = ?2",
                params![workspace_id, sha],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            return Ok(ArtifactInfo {
                id,
                sha256: sha_ref,
                mime: mime.to_string(),
                bytes: bytes.len() as i64,
                path,
            });
        }

        let dest = self.artifacts_dir.join(&sha);
        std::fs::write(&dest, &bytes)?;

        let tx = self.conn.transaction()?;
        let info = ArtifactInfo {
            id: new_id("art"),
            sha256: sha_ref.clone(),
            mime: mime.to_string(),
            bytes: bytes.len() as i64,
            path: dest.to_string_lossy().to_string(),
        };
        tx.execute(
            "INSERT INTO artifacts(id, workspace_id, sha256, mime, bytes, source_event_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
            params![info.id, workspace_id, sha, info.mime, info.bytes, now_iso()],
        )?;
        append_event(
            &tx,
            workspace_id,
            None,
            None,
            "artifact.created",
            json!({"kind": "user", "id": "cli"}),
            json!({"sha256": sha_ref, "mime": info.mime, "bytes": info.bytes}),
            vec![ArtifactRef {
                sha256: format!("sha256:{sha}"),
                mime: info.mime.clone(),
                bytes: info.bytes,
            }],
            None,
        )?;
        tx.commit()?;
        Ok(info)
    }

    fn snapshot(&mut self, branch_id: &str) -> Result<SnapshotInfo> {
        let br: Branch = self
            .conn
            .query_row(
                "SELECT id, thread_id, name, parent_branch_id, fork_event_id, head_event_id,
                        head_sequence, snapshot_id, created_at
                 FROM branches WHERE id = ?1",
                [branch_id],
                row_to_branch,
            )
            .optional()?
            .ok_or_else(|| HliError::not_found(&format!("branch {branch_id}")))?;
        let ws_id: String = self.conn.query_row(
            "SELECT workspace_id FROM threads WHERE id = ?1",
            [&br.thread_id],
            |r| r.get(0),
        )?;
        let head = br
            .head_event_id
            .as_ref()
            .and_then(|id| load_event(&self.conn, id).ok().flatten());

        let content = json!({
            "branch": br,
            "head_event": head,
            "at": now_iso()
        });
        let hash = hli_core::canon::hash_canonical(&content);
        let dir = self.snapshots_dir.join(branch_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", br.head_sequence));
        std::fs::write(&path, canonical_json(&content))?;

        let info = SnapshotInfo {
            id: new_id("snap"),
            branch_id: branch_id.to_string(),
            at_sequence: br.head_sequence,
            path: path.to_string_lossy().to_string(),
            content_hash: hash,
        };
        self.conn.execute(
            "INSERT INTO snapshots(id, workspace_id, branch_id, at_sequence, path, content_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(branch_id, at_sequence) DO UPDATE SET path=excluded.path, content_hash=excluded.content_hash",
            params![info.id, ws_id, info.branch_id, info.at_sequence, info.path, info.content_hash, now_iso()],
        )?;
        Ok(info)
    }

    fn export(&mut self, workspace_id: &str, out: &PathBuf) -> Result<ExportReport> {
        schema::require_workspace(&self.conn, workspace_id)?;

        let ws = self.conn.query_row(
            "SELECT id, name, root_path, created_at, archived_at, config_json
                 FROM workspaces WHERE id = ?1",
            [workspace_id],
            |r| {
                Ok(Workspace {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    root_path: r.get(2)?,
                    created_at: r.get(3)?,
                    archived_at: r.get(4)?,
                    config: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(json!({})),
                })
            },
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT id, workspace_id, title, created_at, archived_at FROM threads
             WHERE workspace_id = ?1 ORDER BY created_at",
        )?;
        let threads: Vec<Thread> = stmt
            .query_map([workspace_id], |r| {
                Ok(Thread {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    title: r.get(2)?,
                    created_at: r.get(3)?,
                    archived_at: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);

        let mut stmt = self.conn.prepare(
            "SELECT b.id, b.thread_id, b.name, b.parent_branch_id, b.fork_event_id, b.head_event_id,
                    b.head_sequence, b.snapshot_id, b.created_at
             FROM branches b JOIN threads t ON t.id = b.thread_id
             WHERE t.workspace_id = ?1 ORDER BY b.created_at",
        )?;
        let branches: Vec<Branch> = stmt
            .query_map([workspace_id], row_to_branch)?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);

        let events = load_events(&self.conn, workspace_id)?;
        let chain_head = events
            .last()
            .map(|e| e.integrity.event_hash.clone())
            .unwrap_or_else(zero_hash);

        let mut files: Vec<ArchiveFile> = Vec::new();

        let state = json!({"workspace": ws, "threads": threads, "branches": branches});
        let state_bytes = canonical_json(&state).into_bytes();
        files.push(ArchiveFile {
            path: "state.json".into(),
            bytes: state_bytes,
        });

        let mut ndjson = String::new();
        for e in &events {
            ndjson.push_str(&canonical_json(&serde_json::to_value(e)?));
            ndjson.push('\n');
        }
        files.push(ArchiveFile {
            path: "events.ndjson".into(),
            bytes: ndjson.into_bytes(),
        });

        let runs = load_runs(&self.conn, workspace_id)?;
        let mut runs_nd = String::new();
        for r in &runs {
            runs_nd.push_str(&canonical_json(&serde_json::to_value(r)?));
            runs_nd.push('\n');
        }
        files.push(ArchiveFile {
            path: "runs.ndjson".into(),
            bytes: runs_nd.into_bytes(),
        });

        let manifests = load_manifests(&self.conn, workspace_id)?;
        let mut man_nd = String::new();
        for m in &manifests {
            man_nd.push_str(&canonical_json(&serde_json::to_value(m)?));
            man_nd.push('\n');
        }
        files.push(ArchiveFile {
            path: "manifests.ndjson".into(),
            bytes: man_nd.into_bytes(),
        });

        let memories = load_memories(&self.conn, workspace_id)?;
        let mut mem_nd = String::new();
        for m in &memories {
            mem_nd.push_str(&canonical_json(&serde_json::to_value(m)?));
            mem_nd.push('\n');
        }
        files.push(ArchiveFile {
            path: "memories.ndjson".into(),
            bytes: mem_nd.into_bytes(),
        });

        let plans = load_plans(&self.conn, workspace_id)?;
        let mut plans_nd = String::new();
        for p in &plans {
            plans_nd.push_str(&canonical_json(&serde_json::to_value(p)?));
            plans_nd.push('\n');
        }
        files.push(ArchiveFile {
            path: "plans.ndjson".into(),
            bytes: plans_nd.into_bytes(),
        });

        let grants = load_grants(&self.conn, workspace_id)?;
        let mut grants_nd = String::new();
        for g in &grants {
            grants_nd.push_str(&canonical_json(&serde_json::to_value(g)?));
            grants_nd.push('\n');
        }
        files.push(ArchiveFile {
            path: "grants.ndjson".into(),
            bytes: grants_nd.into_bytes(),
        });

        let mut stmt = self
            .conn
            .prepare("SELECT sha256 FROM artifacts WHERE workspace_id = ?1 ORDER BY sha256")?;
        let shas: Vec<String> = stmt
            .query_map([workspace_id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        drop(stmt);
        for sha in &shas {
            let p = self.artifacts_dir.join(sha);
            if p.exists() {
                files.push(ArchiveFile {
                    path: format!("artifacts/{sha}"),
                    bytes: std::fs::read(&p)?,
                });
            }
        }

        let file_meta: Vec<Value> = files
            .iter()
            .map(|f| {
                json!({
                    "path": f.path,
                    "sha256": format!("sha256:{}", export::sha256_hex(&f.bytes)),
                    "bytes": f.bytes.len()
                })
            })
            .collect();

        let range = if events.is_empty() {
            json!([0, 0])
        } else {
            json!([
                events.first().unwrap().sequence,
                events.last().unwrap().sequence
            ])
        };

        let manifest = json!({
            "format": export::FORMAT,
            "format_version": export::FORMAT_VERSION,
            "workspace_id": workspace_id,
            "exported_at": now_iso(),
            "sequence_range": range,
            "chain_head": chain_head,
            "counts": {
                "events": events.len(),
                "threads": threads.len(),
                "branches": branches.len(),
                "artifacts": shas.len(),
                "runs": runs.len(),
                "manifests": manifests.len(),
                "memories": memories.len(),
                "plans": plans.len(),
                "grants": grants.len()
            },
            "files": file_meta,
            "redactions": {"secrets": "handles-only"}
        });
        let mhash = export::manifest_hash(&manifest);

        let mut all = vec![ArchiveFile {
            path: "manifest.json".into(),
            bytes: canonical_json(&manifest).into_bytes(),
        }];
        all.extend(files);
        let container = export::container_bytes(&all);
        std::fs::write(out, &container)?;

        Ok(ExportReport {
            path: out.to_string_lossy().to_string(),
            manifest_hash: mhash,
            events: events.len() as i64,
            artifacts: shas.len() as i64,
            chain_head,
        })
    }

    fn import(&mut self, archive: &std::path::Path) -> Result<ImportReport> {
        let meta = std::fs::metadata(archive)?;
        if meta.len() > export::HLIAR_LIMITS.total_bytes {
            return Err(HliError::invalid("archivo excede el limite de 2 GiB"));
        }
        let raw = std::fs::read(archive)?;
        let files = export::read_container_limited(&raw, export::HLIAR_LIMITS)?;
        let manifest_bytes = files
            .iter()
            .find(|f| f.path == "manifest.json")
            .ok_or_else(|| HliError::invalid("archivo sin manifest.json"))?
            .bytes
            .clone();
        let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
        if manifest["format"] != export::FORMAT {
            return Err(HliError::invalid("formato de archivo no soportado"));
        }
        match manifest.get("format_version") {
            None | Some(Value::Null) => {}
            Some(v) => match v.as_u64() {
                Some(n) if n > export::FORMAT_VERSION as u64 => {
                    return Err(HliError::conflict(format!(
                        "archivo hliar con versión futura {n} > {}; actualiza HLI",
                        export::FORMAT_VERSION
                    )));
                }
                Some(n) if n == export::FORMAT_VERSION as u64 => {}
                Some(n) => {
                    return Err(HliError::invalid(format!(
                        "archivo hliar con versión desconocida {n}"
                    )));
                }
                None => return Err(HliError::invalid("format_version no numérico")),
            },
        }
        let workspace_id = manifest["workspace_id"]
            .as_str()
            .ok_or_else(|| HliError::invalid("manifest sin workspace_id"))?
            .to_string();

        for meta in manifest["files"].as_array().unwrap_or(&vec![]) {
            let path = meta["path"].as_str().unwrap_or_default();
            let expect = meta["sha256"].as_str().unwrap_or_default();
            let f = files
                .iter()
                .find(|f| f.path == path)
                .ok_or_else(|| HliError::invalid(format!("falta {path} en el archivo")))?;
            let got = format!("sha256:{}", export::sha256_hex(&f.bytes));
            if got != expect {
                return Err(HliError::conflict(format!("hash no coincide para {path}")));
            }
        }

        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM workspaces WHERE id = ?1",
            [&workspace_id],
            |r| r.get(0),
        )?;
        if exists > 0 {
            return Err(HliError::conflict(format!(
                "workspace {workspace_id} ya existe"
            )));
        }

        let state: Value = serde_json::from_slice(
            &files
                .iter()
                .find(|f| f.path == "state.json")
                .ok_or_else(|| HliError::invalid("archivo sin state.json"))?
                .bytes,
        )?;
        let events: Vec<Event> = {
            let raw = files
                .iter()
                .find(|f| f.path == "events.ndjson")
                .ok_or_else(|| HliError::invalid("archivo sin events.ndjson"))?
                .bytes
                .clone();
            let text = String::from_utf8(raw)
                .map_err(|_| HliError::invalid("events.ndjson no es UTF-8"))?;
            let mut out = Vec::new();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                out.push(serde_json::from_str::<Event>(line)?);
            }
            out
        };

        // Verificación completa antes de escribir.
        let mut prev = zero_hash();
        for e in &events {
            if e.integrity.previous_hash != prev {
                return Err(HliError::conflict(format!(
                    "cadena rota en sequence {}: prev_hash no coincide",
                    e.sequence
                )));
            }
            if !e.verify() {
                return Err(HliError::conflict(format!(
                    "event_hash inválido en sequence {}",
                    e.sequence
                )));
            }
            prev = e.integrity.event_hash.clone();
        }
        let chain_head = events
            .last()
            .map(|e| e.integrity.event_hash.clone())
            .unwrap_or_else(zero_hash);
        if let Some(h) = manifest["chain_head"].as_str()
            && h != chain_head
        {
            return Err(HliError::conflict("chain_head del manifiesto no coincide"));
        }

        let tx = self.conn.transaction()?;
        let ws = &state["workspace"];
        tx.execute(
            "INSERT INTO workspaces(id, name, root_path, created_at, archived_at, config_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                ws["id"].as_str().unwrap_or(&workspace_id),
                ws["name"].as_str().unwrap_or("imported"),
                ws["root_path"].as_str().unwrap_or(""),
                ws["created_at"].as_str().unwrap_or(""),
                ws["archived_at"].as_str(),
                canonical_json(&ws["config"])
            ],
        )?;

        for t in state["threads"].as_array().unwrap_or(&vec![]) {
            tx.execute(
                "INSERT INTO threads(id, workspace_id, title, created_at, archived_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    t["id"].as_str(),
                    t["workspace_id"].as_str(),
                    t["title"].as_str(),
                    t["created_at"].as_str(),
                    t["archived_at"].as_str()
                ],
            )?;
        }
        for b in state["branches"].as_array().unwrap_or(&vec![]) {
            tx.execute(
                "INSERT INTO branches(id, thread_id, name, parent_branch_id, fork_event_id,
                                      head_event_id, head_sequence, snapshot_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    b["id"].as_str(),
                    b["thread_id"].as_str(),
                    b["name"]
                        .as_str()
                        .unwrap_or(b["id"].as_str().unwrap_or("branch")),
                    b["parent_branch_id"].as_str(),
                    b["fork_event_id"].as_str(),
                    b["head_event_id"].as_str(),
                    b["head_sequence"].as_i64().unwrap_or(0),
                    b["snapshot_id"].as_str(),
                    b["created_at"].as_str()
                ],
            )?;
        }
        for e in &events {
            tx.execute(
                "INSERT INTO events(event_id, workspace_id, thread_id, branch_id, run_id, sequence,
                                    type, schema_version, created_at, actor_json, payload_json,
                                    artifacts_json, causation_id, correlation_id, idempotency_key,
                                    prev_hash, event_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    e.event_id,
                    e.workspace_id,
                    e.thread_id,
                    e.branch_id,
                    e.run_id,
                    e.sequence,
                    e.kind,
                    e.schema_version,
                    e.created_at,
                    canonical_json(&e.actor),
                    canonical_json(&e.payload),
                    canonical_json(&serde_json::to_value(&e.artifacts)?),
                    e.causation_id,
                    e.correlation_id,
                    e.idempotency_key,
                    e.integrity.previous_hash,
                    e.integrity.event_hash
                ],
            )?;
        }

        // Runs, manifiestos y memorias (0.3).
        if let Some(f) = files.iter().find(|f| f.path == "runs.ndjson") {
            let text = String::from_utf8_lossy(&f.bytes).to_string();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let r: Run = serde_json::from_str(line)?;
                tx.execute(
                    "INSERT OR IGNORE INTO runs(id, workspace_id, thread_id, branch_id, branch_revision,
                        state, prompt, started_at, ended_at, result_json, receipt_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![r.id, r.workspace_id, r.thread_id, r.branch_id, r.branch_revision,
                            r.state, r.prompt, r.started_at, r.ended_at,
                            r.result.as_ref().map(canonical_json),
                            r.receipt.as_ref().map(|x| canonical_json(&serde_json::to_value(x).unwrap_or(json!({}))))],
                )?;
            }
        }
        if let Some(f) = files.iter().find(|f| f.path == "manifests.ndjson") {
            let text = String::from_utf8_lossy(&f.bytes).to_string();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let m: ContextManifest = serde_json::from_str(line)?;
                tx.execute(
                    "INSERT OR IGNORE INTO context_manifests(id, workspace_id, run_id, branch_revision,
                        budget_tokens, estimated_tokens, content_hash, body_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![m.manifest_id, m.workspace_id, m.run_id, m.branch_revision,
                            m.budget_tokens, m.estimated_tokens, m.content_hash,
                            canonical_json(&serde_json::to_value(&m).unwrap_or(json!({}))), m.created_at],
                )?;
            }
        }
        if let Some(f) = files.iter().find(|f| f.path == "plans.ndjson") {
            let text = String::from_utf8_lossy(&f.bytes).to_string();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let p: RunPlan = serde_json::from_str(line)?;
                let body = canonical_json(&serde_json::to_value(&p).unwrap_or(json!({})));
                tx.execute(
                    "INSERT OR IGNORE INTO run_plans(plan_id, run_id, revision, supersedes_plan_id,
                        invalidation_reasons, workspace_id, thread_id, branch_id, branch_revision,
                        context_manifest_id, origin, status, content_hash, body_json,
                        execution_json, created_at, updated_at, approved_at, rejected_at,
                        approved_by, approval_scope, rejection_reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL,
                             ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                    params![
                        p.plan_id,
                        p.run_id,
                        p.revision,
                        p.supersedes_plan_id,
                        canonical_json(&json!(p.invalidation_reasons)),
                        p.workspace_id,
                        p.thread_id,
                        p.branch_id,
                        p.branch_revision,
                        p.context_manifest_id,
                        p.origin,
                        p.status,
                        p.content_hash,
                        body,
                        p.created_at,
                        p.created_at,
                        p.approved_at,
                        p.rejected_at,
                        p.approved_by,
                        p.approval_scope,
                        p.rejection_reason
                    ],
                )?;
            }
        }
        if let Some(f) = files.iter().find(|f| f.path == "grants.ndjson") {
            let text = String::from_utf8_lossy(&f.bytes).to_string();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let g: PermissionGrant = serde_json::from_str(line)?;
                tx.execute(
                    "INSERT OR IGNORE INTO permission_grants(grant_id, workspace_id, effect, scope,
                        created_at, created_by, run_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        g.grant_id,
                        g.workspace_id,
                        g.effect,
                        g.scope,
                        g.created_at,
                        g.created_by,
                        g.run_id
                    ],
                )?;
            }
        }
        if let Some(f) = files.iter().find(|f| f.path == "memories.ndjson") {
            let text = String::from_utf8_lossy(&f.bytes).to_string();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let m: Memory = serde_json::from_str(line)?;
                tx.execute(
                    "INSERT OR IGNORE INTO memories(id, workspace_id, scope, scope_id, type, status,
                        content, observed_at, valid_from, valid_until, superseded_at,
                        source_authority, subject, relation, object, supersedes_json, contradicts_json,
                        sensitivity, authorship_json, source_events, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                             ?16, ?17, ?18, ?19, ?20, ?21)",
                    params![m.id, m.workspace_id, m.scope, m.scope_id, m.kind, m.status, m.content,
                            m.observed_at, m.valid_from, m.valid_until, m.superseded_at,
                            m.source_authority, m.subject, m.relation, m.object,
                            canonical_json(&json!(m.supersedes)),
                            canonical_json(&json!(m.contradicts)), m.sensitivity,
                            canonical_json(&m.authorship), canonical_json(&json!(m.source_events)),
                            m.created_at],
                )?;
                if m.status == "active" || m.status == "proposed" {
                    tx.execute(
                        "INSERT INTO memories_fts(mem_id, content, workspace_id) VALUES (?1, ?2, ?3)",
                        params![m.id, m.content, m.workspace_id],
                    )?;
                }
            }
        }

        let mut imported_artifacts = 0i64;
        for f in files.iter().filter(|f| f.path.starts_with("artifacts/")) {
            let sha = f.path.trim_start_matches("artifacts/").to_string();
            std::fs::write(self.artifacts_dir.join(&sha), &f.bytes)?;
            tx.execute(
                "INSERT OR IGNORE INTO artifacts(id, workspace_id, sha256, mime, bytes, source_event_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
                params![
                    new_id("art"),
                    workspace_id,
                    sha,
                    export::guess_mime(&f.path),
                    f.bytes.len() as i64,
                    now_iso()
                ],
            )?;
            imported_artifacts += 1;
        }

        tx.commit()?;
        Ok(ImportReport {
            workspace_id,
            events: events.len() as i64,
            artifacts: imported_artifacts,
            verified: true,
            chain_head,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn append_event(
    tx: &Transaction<'_>,
    workspace_id: &str,
    thread_id: Option<&str>,
    branch_id: Option<&str>,
    kind: &str,
    actor: Value,
    payload: Value,
    artifacts: Vec<ArtifactRef>,
    idempotency_key: Option<&str>,
) -> Result<Event> {
    if let Some(key) = idempotency_key
        && let Some(existing) = tx
            .query_row(
                "SELECT event_id FROM events WHERE workspace_id = ?1 AND idempotency_key = ?2",
                params![workspace_id, key],
                |r| r.get::<_, String>(0),
            )
            .optional()?
    {
        return load_event_from(tx, &existing)?
            .ok_or_else(|| HliError::internal("idempotencia: evento perdido"));
    }

    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events WHERE workspace_id = ?1",
        [workspace_id],
        |r| r.get(0),
    )?;
    let prev: String = tx
        .query_row(
            "SELECT event_hash FROM events WHERE workspace_id = ?1 ORDER BY sequence DESC LIMIT 1",
            [workspace_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(zero_hash);

    let mut ev = Event {
        event_id: new_id("evt"),
        workspace_id: workspace_id.to_string(),
        thread_id: thread_id.map(str::to_string),
        branch_id: branch_id.map(str::to_string),
        run_id: payload
            .get("run_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        causation_id: None,
        correlation_id: payload
            .get("run_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        idempotency_key: idempotency_key.map(str::to_string),
        sequence: seq,
        kind: kind.to_string(),
        schema_version: EVENT_SCHEMA_VERSION,
        created_at: now_iso(),
        actor,
        payload,
        artifacts,
        integrity: Integrity {
            canonicalization: "RFC8785".into(),
            hash_alg: "sha256".into(),
            previous_hash: prev,
            event_hash: String::new(),
        },
    };
    ev.seal();

    tx.execute(
        "INSERT INTO events(event_id, workspace_id, thread_id, branch_id, run_id, sequence,
                            type, schema_version, created_at, actor_json, payload_json,
                            artifacts_json, causation_id, correlation_id, idempotency_key,
                            prev_hash, event_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            ev.event_id,
            ev.workspace_id,
            ev.thread_id,
            ev.branch_id,
            ev.run_id,
            ev.sequence,
            ev.kind,
            ev.schema_version,
            ev.created_at,
            canonical_json(&ev.actor),
            canonical_json(&ev.payload),
            canonical_json(&serde_json::to_value(&ev.artifacts)?),
            ev.causation_id,
            ev.correlation_id,
            ev.idempotency_key,
            ev.integrity.previous_hash,
            ev.integrity.event_hash
        ],
    )?;

    if let Some(b) = branch_id {
        tx.execute(
            "UPDATE branches SET head_event_id = ?1, head_sequence = ?2 WHERE id = ?3",
            params![ev.event_id, ev.sequence, b],
        )?;
    }
    Ok(ev)
}

pub(crate) fn row_to_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        event_id: r.get(0)?,
        workspace_id: r.get(1)?,
        thread_id: r.get(2)?,
        branch_id: r.get(3)?,
        run_id: r.get(4)?,
        sequence: r.get(5)?,
        kind: r.get(6)?,
        schema_version: r.get(7)?,
        created_at: r.get(8)?,
        actor: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or(json!({})),
        payload: serde_json::from_str(&r.get::<_, String>(10)?).unwrap_or(json!({})),
        artifacts: serde_json::from_str(&r.get::<_, String>(11)?).unwrap_or_default(),
        causation_id: r.get(12)?,
        correlation_id: r.get(13)?,
        idempotency_key: r.get(14)?,
        integrity: Integrity {
            canonicalization: "RFC8785".into(),
            hash_alg: "sha256".into(),
            previous_hash: r.get(15)?,
            event_hash: r.get(16)?,
        },
    })
}

pub(crate) fn row_to_branch(r: &rusqlite::Row<'_>) -> rusqlite::Result<Branch> {
    Ok(Branch {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        name: r.get(2)?,
        parent_branch_id: r.get(3)?,
        fork_event_id: r.get(4)?,
        head_event_id: r.get(5)?,
        head_sequence: r.get(6)?,
        snapshot_id: r.get(7)?,
        created_at: r.get(8)?,
    })
}

pub(crate) const EVENT_COLS: &str =
    "event_id, workspace_id, thread_id, branch_id, run_id, sequence, type,
    schema_version, created_at, actor_json, payload_json, artifacts_json, causation_id,
    correlation_id, idempotency_key, prev_hash, event_hash";

pub(crate) fn load_event(conn: &Connection, id: &str) -> Result<Option<Event>> {
    Ok(conn
        .query_row(
            &format!("SELECT {EVENT_COLS} FROM events WHERE event_id = ?1"),
            [id],
            row_to_event,
        )
        .optional()?)
}

fn load_event_from(tx: &Transaction<'_>, id: &str) -> Result<Option<Event>> {
    Ok(tx
        .query_row(
            &format!("SELECT {EVENT_COLS} FROM events WHERE event_id = ?1"),
            [id],
            row_to_event,
        )
        .optional()?)
}

pub(crate) fn load_events(conn: &Connection, workspace_id: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLS} FROM events WHERE workspace_id = ?1 ORDER BY sequence"
    ))?;
    let rows = stmt.query_map([workspace_id], row_to_event)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub(crate) const RUN_COLS: &str = "id, workspace_id, thread_id, branch_id, branch_revision, state,
    prompt, started_at, ended_at, result_json, receipt_json";

pub(crate) const PLAN_COLS: &str = "plan_id, run_id, workspace_id, thread_id, branch_id,
    branch_revision, context_manifest_id, origin, status, content_hash, body_json,
    execution_json, created_at, updated_at, approved_at, rejected_at, approved_by,
    approval_scope, rejection_reason, revision";

pub(crate) fn row_to_plan(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunPlan> {
    let body: String = r.get(10)?;
    serde_json::from_str::<RunPlan>(&body).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
    })
}

pub(crate) fn load_plan(conn: &Connection, plan_id: &str) -> Result<Option<RunPlan>> {
    Ok(conn
        .query_row(
            &format!("SELECT {PLAN_COLS} FROM run_plans WHERE plan_id = ?1"),
            [plan_id],
            row_to_plan,
        )
        .optional()?)
}

fn upsert_plan_row(tx: &Transaction<'_>, plan: &RunPlan, execution: Option<&Value>) -> Result<()> {
    let body = canonical_json(&serde_json::to_value(plan).unwrap_or(json!({})));
    let exec = execution.map(canonical_json);
    let reasons = canonical_json(&json!(plan.invalidation_reasons));
    tx.execute(
        "INSERT INTO run_plans(plan_id, run_id, revision, supersedes_plan_id, invalidation_reasons,
                               workspace_id, thread_id, branch_id, branch_revision,
                               context_manifest_id, origin, status, content_hash, body_json,
                               execution_json, created_at, updated_at, approved_at, rejected_at,
                               approved_by, approval_scope, rejection_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22)
         ON CONFLICT(plan_id) DO UPDATE SET
           status = excluded.status,
           content_hash = excluded.content_hash,
           body_json = excluded.body_json,
           execution_json = COALESCE(excluded.execution_json, run_plans.execution_json),
           updated_at = excluded.updated_at,
           approved_at = excluded.approved_at,
           rejected_at = excluded.rejected_at,
           approved_by = excluded.approved_by,
           approval_scope = excluded.approval_scope,
           rejection_reason = excluded.rejection_reason",
        params![
            plan.plan_id,
            plan.run_id,
            plan.revision,
            plan.supersedes_plan_id,
            reasons,
            plan.workspace_id,
            plan.thread_id,
            plan.branch_id,
            plan.branch_revision,
            plan.context_manifest_id,
            plan.origin,
            plan.status,
            plan.content_hash,
            body,
            exec,
            plan.created_at,
            now_iso(),
            plan.approved_at,
            plan.rejected_at,
            plan.approved_by,
            plan.approval_scope,
            plan.rejection_reason
        ],
    )?;
    Ok(())
}

fn insert_plan_row(tx: &Transaction<'_>, plan: &RunPlan, execution: Option<&Value>) -> Result<()> {
    let n: i64 = tx.query_row(
        "SELECT COUNT(*) FROM run_plans WHERE plan_id = ?1",
        [&plan.plan_id],
        |r| r.get(0),
    )?;
    if n > 0 {
        return Err(HliError::conflict(format!(
            "la revisión {} ya existe",
            plan.plan_id
        )));
    }
    upsert_plan_row(tx, plan, execution)
}

pub(crate) fn row_to_grant(r: &rusqlite::Row<'_>) -> rusqlite::Result<PermissionGrant> {
    Ok(PermissionGrant {
        grant_id: r.get(0)?,
        workspace_id: r.get(1)?,
        effect: r.get(2)?,
        scope: r.get(3)?,
        created_at: r.get(4)?,
        created_by: r.get(5)?,
        run_id: r.get(6)?,
    })
}

fn load_plans(conn: &Connection, workspace_id: &str) -> Result<Vec<RunPlan>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PLAN_COLS} FROM run_plans WHERE workspace_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt.query_map([workspace_id], row_to_plan)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn load_grants(conn: &Connection, workspace_id: &str) -> Result<Vec<PermissionGrant>> {
    let mut stmt = conn.prepare(
        "SELECT grant_id, workspace_id, effect, scope, created_at, created_by, run_id
         FROM permission_grants WHERE workspace_id = ?1 ORDER BY effect",
    )?;
    let rows = stmt.query_map([workspace_id], row_to_grant)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub(crate) fn row_to_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    Ok(Run {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        thread_id: r.get(2)?,
        branch_id: r.get(3)?,
        branch_revision: r.get(4)?,
        state: r.get(5)?,
        prompt: r.get(6)?,
        started_at: r.get(7)?,
        ended_at: r.get(8)?,
        result: r
            .get::<_, Option<String>>(9)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        receipt: r
            .get::<_, Option<String>>(10)?
            .and_then(|s| serde_json::from_str(&s).ok()),
    })
}

pub(crate) fn load_run(conn: &Connection, id: &str) -> Result<Option<Run>> {
    Ok(conn
        .query_row(
            &format!("SELECT {RUN_COLS} FROM runs WHERE id = ?1"),
            [id],
            row_to_run,
        )
        .optional()?)
}

#[allow(dead_code)]
pub(crate) fn load_manifest(conn: &Connection, id: &str) -> Result<Option<ContextManifest>> {
    let body: Option<String> = conn
        .query_row(
            "SELECT body_json FROM context_manifests WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(body.and_then(|b| serde_json::from_str(&b).ok()))
}

pub(crate) const MEMORY_COLS: &str = "id, workspace_id, scope, scope_id, type, status, content,
    observed_at, valid_from, valid_until, superseded_at, source_authority, subject, relation,
    object, supersedes_json, contradicts_json, sensitivity, authorship_json, source_events, created_at";

pub(crate) fn row_to_memory(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        scope: r.get(2)?,
        scope_id: r.get(3)?,
        kind: r.get(4)?,
        status: r.get(5)?,
        content: r.get(6)?,
        observed_at: r.get(7)?,
        valid_from: r.get(8)?,
        valid_until: r.get(9)?,
        superseded_at: r.get(10)?,
        source_authority: r.get(11)?,
        subject: r.get(12)?,
        relation: r.get(13)?,
        object: r.get(14)?,
        supersedes: serde_json::from_str(&r.get::<_, String>(15)?).unwrap_or_default(),
        contradicts: serde_json::from_str(&r.get::<_, String>(16)?).unwrap_or_default(),
        sensitivity: r.get(17)?,
        authorship: serde_json::from_str(&r.get::<_, String>(18)?).unwrap_or(json!({})),
        source_events: serde_json::from_str(&r.get::<_, String>(19)?).unwrap_or_default(),
        created_at: r.get(20)?,
    })
}

pub(crate) fn load_memory(conn: &Connection, id: &str) -> Result<Option<Memory>> {
    Ok(conn
        .query_row(
            &format!("SELECT {MEMORY_COLS} FROM memories WHERE id = ?1"),
            [id],
            row_to_memory,
        )
        .optional()?)
}

fn load_runs(conn: &Connection, workspace_id: &str) -> Result<Vec<Run>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {RUN_COLS} FROM runs WHERE workspace_id = ?1 ORDER BY started_at"
    ))?;
    let rows = stmt.query_map([workspace_id], row_to_run)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn load_manifests(conn: &Connection, workspace_id: &str) -> Result<Vec<ContextManifest>> {
    let mut stmt = conn.prepare(
        "SELECT body_json FROM context_manifests WHERE workspace_id = ?1 ORDER BY created_at",
    )?;
    let rows = stmt.query_map([workspace_id], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        if let Ok(m) = serde_json::from_str::<ContextManifest>(&row?) {
            out.push(m);
        }
    }
    Ok(out)
}

fn load_memories(conn: &Connection, workspace_id: &str) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEMORY_COLS} FROM memories WHERE workspace_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt.query_map([workspace_id], row_to_memory)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}
