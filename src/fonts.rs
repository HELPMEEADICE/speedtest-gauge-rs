//! 运行时字体注册（Slint 1.18.1 没有 `register_font_from_path`，只有
//! `slint::fontique_011::shared_collection()` + `Collection::register_fonts`）。
//!
//! 两个字体：
//! - `Montserrat`（SIL OFL 1.1，`assets/fonts/montserrat.ttf`，Bold 700 静态实例）：
//!   用 `include_bytes!` 打进二进制，任何宿主都拿得到。
//! - `Gauge Mono`（Ookla 资产，**不随仓库分发**）：只在 `.local-fonts/gaugemono.ttf`
//!   真实存在时注册，否则静默跳过（报告里会记一条 `missing`）。
//!
//! `font-family` 不支持逗号回退列表（PROBE E1），所以 `.slint` 里只写单一族名；
//! 缺字体时 Slint 会退回系统默认字体，这在视觉上会明显变宽——所以本模块报告
//! "到底注册上了没有"，由调用方打印。

#[cfg(feature = "bundled-fonts")]
use std::path::PathBuf;
#[cfg(feature = "bundled-fonts")]
use std::sync::{Arc, OnceLock};

/// 注册结果。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FontReport {
    /// 成功注册的 face 数。
    pub faces: usize,
    /// 成功注册的来源（人类可读）。
    pub sources: Vec<String>,
    /// 找不到而跳过的路径。
    pub missing: Vec<String>,
}

impl FontReport {
    /// 是否两个字体都到位（`Gauge Mono` 缺失时仍有 Montserrat）。
    pub fn all_faces(&self) -> usize {
        self.faces
    }
}

impl std::fmt::Display for FontReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} face(s): {}", self.faces, self.sources.join(", "))?;
        if !self.missing.is_empty() {
            write!(f, "；缺失：{}", self.missing.join(", "))?;
        }
        Ok(())
    }
}

/// 随包的 Montserrat（OFL 1.1，可分发）。
#[cfg(feature = "bundled-fonts")]
const MONTSERRAT: &[u8] = include_bytes!("../assets/fonts/montserrat.ttf");

/// 注册随包字体 + 本机专属字体（幂等：同进程只真正注册一次）。
#[cfg(feature = "bundled-fonts")]
pub fn register_bundled_fonts() -> FontReport {
    static STATE: OnceLock<FontReport> = OnceLock::new();
    STATE.get_or_init(register_once).clone()
}

/// 没开 `bundled-fonts` 时的空操作（保持调用点无条件编译）。
#[cfg(not(feature = "bundled-fonts"))]
pub fn register_bundled_fonts() -> FontReport {
    FontReport::default()
}

#[cfg(feature = "bundled-fonts")]
fn register_once() -> FontReport {
    let mut report = FontReport::default();
    let mut collection = slint::fontique_011::shared_collection();

    // 1) Montserrat（随包）。
    let blob = slint::fontique_011::fontique::Blob::new(Arc::new(MONTSERRAT.to_vec()));
    let faces = collection.register_fonts(blob, None);
    if !faces.is_empty() {
        report.faces += faces.len();
        report
            .sources
            .push(format!("montserrat.ttf ({} face)", faces.len()));
    }

    // 2) Gauge Mono（专有，可能不存在）。
    match gauge_mono_path() {
        Some(path) => match std::fs::read(&path) {
            Ok(data) => {
                let blob = slint::fontique_011::fontique::Blob::new(Arc::new(data));
                let faces = collection.register_fonts(blob, None);
                if !faces.is_empty() {
                    report.faces += faces.len();
                    report
                        .sources
                        .push(format!("gaugemono.ttf ({} face)", faces.len()));
                } else {
                    report
                        .missing
                        .push(format!("{} (无法解析)", path.display()));
                }
            }
            Err(e) => report.missing.push(format!("{} ({e})", path.display())),
        },
        None => report.missing.push(".local-fonts/gaugemono.ttf".into()),
    }

    report
}

/// 找到 `gaugemono.ttf`：优先 crate 源目录（开发期），其次可执行文件同级目录
/// 与当前工作目录（部署期）。
#[cfg(feature = "bundled-fonts")]
fn gauge_mono_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".local-fonts/gaugemono.ttf"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(".local-fonts/gaugemono.ttf"));
            candidates.push(dir.join("gaugemono.ttf"));
        }
    }
    candidates.push(PathBuf::from(".local-fonts/gaugemono.ttf"));
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "bundled-fonts")]
    #[test]
    fn bundled_montserrat_is_present_in_the_binary() {
        assert!(
            MONTSERRAT.len() > 100_000,
            "montserrat.ttf 应该被打进二进制"
        );
    }

    #[test]
    fn registering_is_idempotent() {
        let a = register_bundled_fonts();
        let b = register_bundled_fonts();
        assert_eq!(a, b);
    }
}
