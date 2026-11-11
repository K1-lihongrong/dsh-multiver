use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
pub struct JobSnapshot {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub status: String,
    pub stage: String,
    pub done: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
    pub code: Option<String>,
    /// Unix seconds at which the job was created. Additive field: existing
    /// frontend payloads keep all of their previous keys.
    #[serde(default)]
    pub started_at: u64,
}

/// Cancellation flag owned by one job. The registry keeps a handle per job so
/// callers can request cancellation without knowing the job's internal state.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Default)]
pub struct JobRegistry {
    inner: Arc<Mutex<VecDeque<JobSnapshot>>>,
    tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    seq: Arc<AtomicU64>,
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl JobRegistry {
    pub fn create(&self, kind: &str, target: &str) -> String {
        let n = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let id = format!("job-{n}");
        let snapshot = JobSnapshot {
            id: id.clone(),
            kind: kind.into(),
            target: target.into(),
            status: "running".into(),
            stage: "created".into(),
            done: 0,
            total: None,
            message: None,
            code: None,
            started_at: unix_now_secs(),
        };
        let token = CancellationToken::new();
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut tokens = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        q.push_back(snapshot);
        while q.len() > 64 {
            if let Some(old) = q.pop_front() {
                tokens.remove(&old.id);
            }
        }
        tokens.insert(id.clone(), token);
        id
    }

    pub fn token(&self, id: &str) -> Option<CancellationToken> {
        self.tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    /// Most recent job of `kind` that is still running. Used by the legacy
    /// `cancel_scan` command to reach the scan job's cancellation token.
    pub fn latest_running_id(&self, kind: &str) -> Option<String> {
        let q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        q.iter()
            .rev()
            .find(|j| j.kind == kind && j.status == "running")
            .map(|j| j.id.clone())
    }

    pub fn snapshot(&self, id: &str) -> Option<JobSnapshot> {
        let q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        q.iter().find(|j| j.id == id).cloned()
    }

    pub fn progress(&self, id: &str, stage: &str, done: u64, total: Option<u64>) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(j) = q.iter_mut().find(|j| j.id == id) {
            j.stage = stage.into();
            j.done = done;
            j.total = total;
        }
    }

    pub fn finish(&self, id: &str, message: &str) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(j) = q.iter_mut().find(|j| j.id == id) {
            j.status = "done".into();
            j.stage = "done".into();
            j.message = Some(message.into());
        }
    }

    pub fn fail(&self, id: &str, code: &str, message: &str) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(j) = q.iter_mut().find(|j| j.id == id) {
            j.status = "failed".into();
            j.stage = "failed".into();
            j.code = Some(code.into());
            j.message = Some(message.into());
        }
    }

    /// A2-5: only safe paths (currently scan) use cancellation. A cancelled
    /// job is neither a success nor an ordinary failure.
    pub fn cancel(&self, id: &str, message: &str) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(j) = q.iter_mut().find(|j| j.id == id) {
            j.status = "cancelled".into();
            j.stage = "cancelled".into();
            j.code = Some("cancelled".into());
            j.message = Some(message.into());
        }
    }

    pub fn list_recent(&self, limit: usize) -> Vec<JobSnapshot> {
        let q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        q.iter().rev().take(limit).cloned().collect()
    }
}

/// Merge the serialized full snapshot with optional per-event extras.
///
/// The snapshot keys are always present; extras are additive and may add
/// stage-specific fields such as `version`, `index`, `name`, `phase`, `line`,
/// `files` or `fileTotal`.
fn event_payload(snapshot: &JobSnapshot, extra: Option<&serde_json::Value>) -> serde_json::Value {
    let mut value = serde_json::to_value(snapshot)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
    if let (Some(base), Some(extra)) = (value.as_object_mut(), extra) {
        match extra {
            serde_json::Value::Object(fields) => {
                for (k, v) in fields {
                    base.insert(k.clone(), v.clone());
                }
            }
            other => {
                base.insert("extra".into(), other.clone());
            }
        }
    }
    value
}

/// Emits unified `job:*` events and keeps the registry in sync.
#[derive(Clone)]
pub struct JobEventSink {
    app: tauri::AppHandle,
    registry: JobRegistry,
}

impl JobEventSink {
    pub fn new(app: tauri::AppHandle, registry: JobRegistry) -> Self {
        Self { app, registry }
    }

    pub fn create(&self, kind: &str, target: &str) -> String {
        self.registry.create(kind, target)
    }

    pub fn token(&self, id: &str) -> Option<CancellationToken> {
        self.registry.token(id)
    }

    pub fn progress(
        &self,
        id: &str,
        stage: &str,
        done: u64,
        total: Option<u64>,
        extra: Option<serde_json::Value>,
    ) {
        self.registry.progress(id, stage, done, total);
        self.emit("job:progress", id, extra);
    }

    pub fn finish(&self, id: &str, message: &str, extra: Option<serde_json::Value>) {
        self.registry.finish(id, message);
        self.emit("job:done", id, extra);
    }

    pub fn fail(&self, id: &str, code: &str, message: &str, extra: Option<serde_json::Value>) {
        self.registry.fail(id, code, message);
        self.emit("job:failed", id, extra);
    }

    pub fn cancel(&self, id: &str, message: &str, extra: Option<serde_json::Value>) {
        self.registry.cancel(id, message);
        self.emit("job:failed", id, extra);
    }

    fn emit(&self, event: &str, id: &str, extra: Option<serde_json::Value>) {
        let Some(snapshot) = self.registry.snapshot(id) else {
            return;
        };
        let payload = event_payload(&snapshot, extra.as_ref());
        let _ = self.app.emit(event, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_lifecycle_and_limit() {
        let reg = JobRegistry::default();
        let id = reg.create("update", "0.2.2-rc.1/web");
        reg.progress(&id, "installing", 1, Some(2));
        reg.finish(&id, "ok");
        let jobs = reg.list_recent(10);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, "done");
        assert_eq!(jobs[0].done, 1);
        assert!(jobs[0].started_at > 0);
    }

    #[test]
    fn failure_keeps_code() {
        let reg = JobRegistry::default();
        let id = reg.create("install", "x");
        reg.fail(&id, "network", "down");
        let jobs = reg.list_recent(1);
        assert_eq!(jobs[0].code.as_deref(), Some("network"));
    }

    #[test]
    fn cancel_marks_job_as_cancelled() {
        let reg = JobRegistry::default();
        let id = reg.create("scan", "2 versions");
        reg.cancel(&id, "scan cancelled");
        let jobs = reg.list_recent(1);
        assert_eq!(jobs[0].status, "cancelled");
        assert_eq!(jobs[0].stage, "cancelled");
        assert_eq!(jobs[0].code.as_deref(), Some("cancelled"));
    }

    #[test]
    fn token_cancels_and_is_shared() {
        let reg = JobRegistry::default();
        let id = reg.create("scan", "3 versions");
        let token = reg.token(&id).expect("job should have a token");
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
        assert!(reg.token(&id).expect("token still exists").is_cancelled());
    }

    #[test]
    fn history_is_capped_at_64_and_drops_old_tokens() {
        let reg = JobRegistry::default();
        let first = reg.create("scan", "old");
        for i in 0..70 {
            reg.create("install", &format!("v{i}"));
        }
        let jobs = reg.list_recent(100);
        assert_eq!(jobs.len(), 64);
        assert!(jobs.iter().all(|j| j.id != first));
        assert!(reg.token(&first).is_none());
    }

    #[test]
    fn event_payload_has_full_snapshot_and_extras() {
        let reg = JobRegistry::default();
        let id = reg.create("scan", "2 versions");
        reg.progress(&id, "detail", 1, Some(2));
        let snapshot = reg.snapshot(&id).unwrap();
        let payload = event_payload(
            &snapshot,
            Some(&serde_json::json!({ "version": "1.0.0", "index": 1, "cached": true })),
        );
        assert_eq!(payload["id"], id);
        assert_eq!(payload["kind"], "scan");
        assert_eq!(payload["target"], "2 versions");
        assert_eq!(payload["status"], "running");
        assert_eq!(payload["stage"], "detail");
        assert_eq!(payload["done"], 1);
        assert_eq!(payload["total"], 2);
        assert_eq!(payload["started_at"], snapshot.started_at);
        assert_eq!(payload["version"], "1.0.0");
        assert_eq!(payload["index"], 1);
        assert_eq!(payload["cached"], true);
    }
}
