use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
}

#[derive(Clone, Default)]
pub struct JobRegistry {
    inner: Arc<Mutex<VecDeque<JobSnapshot>>>,
    seq: Arc<AtomicU64>,
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
        };
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        q.push_back(snapshot);
        while q.len() > 64 {
            q.pop_front();
        }
        id
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

    #[allow(dead_code)]
    pub fn fail(&self, id: &str, code: &str, message: &str) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(j) = q.iter_mut().find(|j| j.id == id) {
            j.status = "failed".into();
            j.stage = "failed".into();
            j.code = Some(code.into());
            j.message = Some(message.into());
        }
    }

    pub fn list_recent(&self, limit: usize) -> Vec<JobSnapshot> {
        let q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        q.iter().rev().take(limit).cloned().collect()
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
    }

    #[test]
    fn failure_keeps_code() {
        let reg = JobRegistry::default();
        let id = reg.create("install", "x");
        reg.fail(&id, "network", "down");
        let jobs = reg.list_recent(1);
        assert_eq!(jobs[0].code.as_deref(), Some("network"));
    }
}
