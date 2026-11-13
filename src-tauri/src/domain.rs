//! 领域类型（v0.4.0-alpha.1）。
//!
//! 目标不是消灭所有 `String`，而是把最容易混淆的三类字符串收进类型：
//! - 版本号：`VersionId`
//! - 实例 id：`InstanceId`
//! - 进程/窗口身份：`ProcKey`
//!
//! 这些类型都保持字符串友好的 serde 表示，迁移旧配置时不需要额外格式转换。

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

/// 已安装 dsh 版本号。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VersionId(String);

impl VersionId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, String> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err("版本号不能为空".into());
        }
        if value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        {
            return Err(format!("版本号含非法字符：{value}"));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for VersionId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl AsRef<str> for VersionId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for VersionId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// 全局实例 id：`i1` / `i2`……永不复用。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(String);

impl InstanceId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, String> {
        let value = value.as_ref().trim();
        let digits = value
            .strip_prefix('i')
            .ok_or_else(|| format!("实例 id 必须以 i 开头：{value}"))?;
        if digits.is_empty()
            || digits.len() > 1 && digits.starts_with('0')
            || !digits.chars().all(|c| c.is_ascii_digit())
        {
            return Err(format!("实例 id 格式错误：{value}"));
        }
        let n: u64 = digits
            .parse()
            .map_err(|_| format!("实例 id 超出范围：{value}"))?;
        if n == 0 {
            return Err("实例 id 不能为 i0".into());
        }
        Ok(Self(format!("i{n}")))
    }

    pub fn from_u64(n: u64) -> Result<Self, String> {
        if n == 0 {
            return Err("实例序号不能为 0".into());
        }
        Ok(Self(format!("i{n}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn value(&self) -> u64 {
        self.0[1..].parse().unwrap_or(0)
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for InstanceId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// 某个版本窗口应使用哪一份 DSH_HOME。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeRef {
    /// 管理器共享 home：<root>/home
    Shared,
    /// 官方 ~/.dsh
    Official,
    /// 实例 home：<root>/homes/<id>
    Instance(InstanceId),
}

/// 进程表中的身份。
///
/// 非隔离窗口按版本区分；实例窗口按实例 id 区分，不携带版本——
/// 同一实例在哪个版本先启动就在哪跑，符合 v0.4.0 的实例语义。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum ProcKey {
    Shared(VersionId),
    Instance(InstanceId),
}

impl ProcKey {
    #[allow(dead_code)]
    pub fn instance(id: InstanceId) -> Self {
        Self::Instance(id)
    }

    /// 窗口 label 前缀（不含 generation）。保持既有共享窗口 label 形状。
    pub fn label_prefix(&self) -> String {
        match self {
            Self::Shared(v) => format!("dsh-{}", sanitize_label(v.as_str())),
            Self::Instance(id) => format!("dsh-instance-{}", sanitize_label(id.as_str())),
        }
    }
}

/// 创建窗口时携带的凭据：身份、窗口 label、代次与实际 home，避免三者在调用链中错配。
#[derive(Debug, Clone)]
pub struct LaunchTicket {
    pub proc_key: ProcKey,
    pub label: String,
    pub generation: u64,
    pub home: PathBuf,
}

fn sanitize_label(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '/' | ':' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_id_validates_and_normalizes() {
        assert_eq!(
            VersionId::new("  0.2.2-rc.1 ").unwrap().as_str(),
            "0.2.2-rc.1"
        );
        assert!(VersionId::new("").is_err());
        assert!(VersionId::new("a/b").is_err());
        assert!(VersionId::new("a\\b").is_err());
        assert!(VersionId::new("a\nb").is_err());
    }

    #[test]
    fn instance_id_validates_and_round_trips() {
        assert_eq!(InstanceId::new("i12").unwrap().to_string(), "i12");
        assert!(InstanceId::new("12").is_err());
        assert!(InstanceId::new("i0").is_err());
        assert!(InstanceId::new("i01").is_err());
        assert!(InstanceId::new("i-1").is_err());
        let id: InstanceId = "i3".parse().unwrap();
        assert_eq!(id.value(), 3);
    }

    #[test]
    fn proc_key_labels_are_stable() {
        assert_eq!(
            ProcKey::Shared(VersionId::new("0.2.2-rc.1").unwrap()).label_prefix(),
            "dsh-0_2_2-rc_1"
        );
        assert_eq!(
            ProcKey::instance(InstanceId::new("i2").unwrap()).label_prefix(),
            "dsh-instance-i2"
        );
    }

    #[test]
    fn home_ref_serde_is_readable() {
        let json =
            serde_json::to_string(&HomeRef::Instance(InstanceId::new("i2").unwrap())).unwrap();
        assert_eq!(json, r#"{"instance":"i2"}"#);
        assert_eq!(
            serde_json::to_string(&HomeRef::Shared).unwrap(),
            r#""shared""#
        );
    }
}
