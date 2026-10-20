//! 输出辅助：JSON 序列化、统一结果输出、格式化。

/// 序列化为单行 JSON（失败时退化为 null，不 panic）。
pub fn json_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".to_string())
}

/// 统一的"操作结果"输出：JSON 模式输出 {"ok":bool,"message":...}，否则按原样打印。
/// 用于破坏性命令的成功/失败提示。
pub fn out_result(json: bool, ok: bool, message: &str) {
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({ "ok": ok, "message": message }))
        );
    } else if ok {
        println!("{}", message);
    } else {
        eprintln!("{}", message);
    }
}

/// which 失败：JSON 输出 {"ok":false,"error":...}，否则 stderr。
pub fn which_fail(json: bool, msg: &str) -> i32 {
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({ "ok": false, "error": msg }))
        );
    } else {
        eprintln!("{}", msg);
    }
    1
}

/// info 失败：与 which_fail 同义（保留独立命名以匹配原语义）。
pub fn info_fail(json: bool, msg: &str) -> i32 {
    if json {
        println!(
            "{}",
            json_str(&serde_json::json!({ "ok": false, "error": msg }))
        );
    } else {
        eprintln!("{}", msg);
    }
    1
}

/// 人类可读的字节数（B / KB / MB / GB，保留两位）。
pub fn fmt_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if f >= GB {
        format!("{:.2} GB", f / GB)
    } else if f >= MB {
        format!("{:.2} MB", f / MB)
    } else if f >= KB {
        format!("{:.2} KB", f / KB)
    } else {
        format!("{} B", n)
    }
}

/// 由 ProgressEvent 估算总进度百分比（0-100）。
/// 简化版加权：每个阶段等权，阶段内按 fraction 推进。
pub fn pct_of(ev: &crate::versions::ProgressEvent) -> u32 {
    let done = (ev.step.saturating_sub(1)) as f32 + ev.fraction;
    let total = ev.total.max(1) as f32;
    ((done / total) * 100.0).round().clamp(0.0, 100.0) as u32
}
