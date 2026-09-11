//! 「进入应用」窗口（label = "web"）的几何记忆：关闭时保存位置与大小，下次原样打开。
//!
//! Windows 上有三种"看上去都是个正常窗口"的状态必须先分辨清楚，否则还原出来会跑偏：
//!
//! 1. **最大化**（包含把窗口拖到屏幕顶部触发的那种）：此时 `outer_position` 是负的
//!    边框偏移、`inner_size` 是整块工作区，照单存下会记成"比屏幕还大"；只记「要最大化」，
//!    位置与大小沿用最近一次正常态。
//! 2. **最小化**：Windows 把窗口挪到 (-32000,-32000) 并压成 0 尺寸，同样跳过 ——
//!    否则下次打开是个贴在工作区外的迷你窗口。
//! 3. **贴边吸附**（拖到左右边缘的半屏、拖到四角的一格）：这是正常窗口状态，
//!    位置与大小如实保存即可，还原后仍是同样的半屏/四分之一。
//!
//! 几何一律按逻辑像素存取（`inner_size` 语义对应还原时的 `set_size`），
//! 这样在缩放比例不同的显示器之间来回搬窗口，视觉大小也保持一致。

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{LogicalPosition, LogicalSize, Runtime, WebviewWindow};

use crate::paths;

/// 窗口最小尺寸（逻辑像素），与 builder 的 `min_inner_size` 保持一致
pub const MIN_WIDTH: f64 = 880.0;
pub const MIN_HEIGHT: f64 = 600.0;
/// 首次运行、没有历史几何时的默认尺寸
pub const DEFAULT_WIDTH: f64 = 1280.0;
pub const DEFAULT_HEIGHT: f64 = 840.0;

/// 窗口几何（逻辑像素）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 上次关闭时是否处于最大化；为真时还原后自动最大化
    #[serde(default)]
    pub maximized: bool,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            maximized: false,
        }
    }
}

/// 窗口当前处于哪种状态（决定这次的位置大小能不能信）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPlacement {
    Normal,
    Maximized,
    Minimized,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WindowStateFile {
    #[serde(default)]
    app: Option<WindowGeometry>,
}

/// 读取上次保存的窗口几何；文件缺失或损坏时按「没保存过」处理
pub fn load(data_dir: &Path) -> Option<WindowGeometry> {
    let raw = std::fs::read_to_string(paths::window_state_path(data_dir)).ok()?;
    serde_json::from_str::<WindowStateFile>(&raw).ok()?.app
}

pub fn save(data_dir: &Path, geometry: &WindowGeometry) -> std::io::Result<()> {
    let body = serde_json::to_string_pretty(&WindowStateFile {
        app: Some(*geometry),
    })
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(paths::window_state_path(data_dir), body)
}

/// 记住最近一次"正常态"的位置与大小：最大化/最小化期间的事件不覆盖它。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct GeometryTracker {
    restored: Option<WindowGeometry>,
    maximized: bool,
}

impl GeometryTracker {
    /// 用已保存的几何预热：窗口刚按它创建出来，事件还没来之前就有值可存
    pub fn prime(&mut self, geometry: WindowGeometry) {
        self.restored = Some(WindowGeometry {
            maximized: false,
            ..geometry
        });
        self.maximized = geometry.maximized;
    }

    pub fn observe(&mut self, geometry: WindowGeometry, placement: WindowPlacement) {
        match placement {
            WindowPlacement::Normal => {
                self.restored = Some(WindowGeometry {
                    maximized: false,
                    ..geometry
                });
                self.maximized = false;
            }
            // 最大化时的位置大小不可信，只更新「下次要不要最大化」
            WindowPlacement::Maximized => self.maximized = true,
            WindowPlacement::Minimized => {}
        }
    }

    /// 待落盘的几何：位置大小取最近一次正常态，最大化标记取最新状态
    pub fn snapshot(&self) -> Option<WindowGeometry> {
        self.restored.map(|geometry| WindowGeometry {
            maximized: self.maximized,
            ..geometry
        })
    }
}

/// 读一次窗口当前的位置与大小；窗口正在销毁等读不到的情况返回 None。
/// 尺寸为 0 也按读不到处理 —— 最小化或销毁中的窗口会给出这种值，
/// 存进去就成了"一个没有大小的窗口"。
pub fn capture<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Option<(WindowGeometry, WindowPlacement)> {
    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?.to_logical::<f64>(scale);
    let size = window.inner_size().ok()?.to_logical::<f64>(scale);
    if !(size.width >= 1.0 && size.height >= 1.0) {
        return None;
    }
    let placement = if window.is_minimized().unwrap_or(false) {
        WindowPlacement::Minimized
    } else if window.is_maximized().unwrap_or(false) {
        WindowPlacement::Maximized
    } else {
        WindowPlacement::Normal
    };
    let geometry = WindowGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: placement == WindowPlacement::Maximized,
    };
    Some((geometry, placement))
}

fn sane(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// 只保证不小于最小尺寸（取不到显示器信息时的兜底）
fn fit_minimum(geometry: WindowGeometry) -> WindowGeometry {
    WindowGeometry {
        x: sane(geometry.x, 0.0),
        y: sane(geometry.y, 0.0),
        width: sane(geometry.width, DEFAULT_WIDTH).max(MIN_WIDTH),
        height: sane(geometry.height, DEFAULT_HEIGHT).max(MIN_HEIGHT),
        maximized: geometry.maximized,
    }
}

/// 把保存的几何夹进当前显示器的工作区：换过显示器、拔掉副屏或改过分辨率之后，
/// 窗口不会还原到看不见的地方，也不会比工作区还大。
pub fn fit_to_desktop<R: Runtime>(
    window: &WebviewWindow<R>,
    geometry: WindowGeometry,
) -> WindowGeometry {
    let fitted = fit_minimum(geometry);
    let Ok(monitors) = window.available_monitors() else {
        return fitted;
    };
    if monitors.is_empty() {
        return fitted;
    }
    // 优先落在上次那台显示器上；它不在了就退回主屏
    let contains = |monitor: &tauri::Monitor| {
        let position = monitor.position();
        let size = monitor.size();
        let left = f64::from(position.x);
        let top = f64::from(position.y);
        geometry.x >= left
            && geometry.y >= top
            && geometry.x < left + f64::from(size.width)
            && geometry.y < top + f64::from(size.height)
    };
    let primary = window.primary_monitor().ok().flatten();
    let monitor = monitors
        .iter()
        .find(|monitor| contains(monitor))
        .or(primary.as_ref())
        .or_else(|| monitors.first());
    let Some(monitor) = monitor else {
        return fitted;
    };

    let area = monitor.work_area();
    let scale = monitor.scale_factor().max(f64::EPSILON);
    let room_width = f64::from(area.size.width) / scale;
    let room_height = f64::from(area.size.height) / scale;
    let left = f64::from(area.position.x) / scale;
    let top = f64::from(area.position.y) / scale;
    let width = fitted.width.min(room_width.max(MIN_WIDTH));
    let height = fitted.height.min(room_height.max(MIN_HEIGHT));
    WindowGeometry {
        x: fitted.x.clamp(left, (left + room_width - width).max(left)),
        y: fitted.y.clamp(top, (top + room_height - height).max(top)),
        width,
        height,
        maximized: fitted.maximized,
    }
}

/// 按这份几何摆放窗口，返回实际生效的几何。
///
/// 尺寸与位置走 `set_size`/`set_position`（客户区尺寸 + 左上角坐标，与 `capture`
/// 的取值口径一致）；最大化交给建窗时的 `.maximized()`，这里不重复调用 —— 那样
/// 会在窗口已经显示之后才放大，闪一下。
pub fn apply_geometry<R: Runtime>(
    window: &WebviewWindow<R>,
    geometry: WindowGeometry,
) -> WindowGeometry {
    let fitted = fit_to_desktop(window, geometry);
    let _ = window.set_size(LogicalSize::new(fitted.width, fitted.height));
    let _ = window.set_position(LogicalPosition::new(fitted.x, fitted.y));
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(x: f64, y: f64, width: f64, height: f64) -> WindowGeometry {
        WindowGeometry {
            x,
            y,
            width,
            height,
            maximized: false,
        }
    }

    #[test]
    fn maximized_state_keeps_the_last_normal_geometry() {
        let mut tracker = GeometryTracker::default();
        tracker.observe(
            geometry(120.0, 80.0, 1280.0, 840.0),
            WindowPlacement::Normal,
        );
        // Windows 最大化时读到的是负坐标 + 整块工作区，不能当真
        tracker.observe(
            geometry(-8.0, -8.0, 1936.0, 1056.0),
            WindowPlacement::Maximized,
        );

        let saved = tracker.snapshot().unwrap();
        assert_eq!((saved.x, saved.y), (120.0, 80.0));
        assert_eq!((saved.width, saved.height), (1280.0, 840.0));
        assert!(saved.maximized);
    }

    #[test]
    fn minimizing_does_not_clobber_the_saved_geometry() {
        let mut tracker = GeometryTracker::default();
        tracker.observe(
            geometry(200.0, 120.0, 1000.0, 700.0),
            WindowPlacement::Normal,
        );
        tracker.observe(
            geometry(-32000.0, -32000.0, 0.0, 0.0),
            WindowPlacement::Minimized,
        );

        let saved = tracker.snapshot().unwrap();
        assert_eq!((saved.x, saved.y), (200.0, 120.0));
        assert_eq!((saved.width, saved.height), (1000.0, 700.0));
        assert!(!saved.maximized);
    }

    #[test]
    fn snapped_windows_are_stored_as_is() {
        // 拖到左边缘的半屏：仍是正常窗口状态，如实保存即可
        let mut tracker = GeometryTracker::default();
        tracker.observe(
            geometry(0.0, 0.0, 960.0, 1040.0),
            WindowPlacement::Normal,
        );
        tracker.observe(
            geometry(960.0, 0.0, 960.0, 1040.0),
            WindowPlacement::Normal,
        );

        let saved = tracker.snapshot().unwrap();
        assert_eq!((saved.x, saved.width), (960.0, 960.0));
        assert!(!saved.maximized);
    }

    #[test]
    fn restoring_from_maximized_clears_the_flag() {
        let mut tracker = GeometryTracker::default();
        tracker.prime(WindowGeometry {
            maximized: true,
            ..geometry(60.0, 60.0, 1100.0, 780.0)
        });
        assert!(tracker.snapshot().unwrap().maximized);

        // 点「还原」按钮：Windows 回到正常态，事件随即把标记清掉
        tracker.observe(geometry(60.0, 60.0, 1100.0, 780.0), WindowPlacement::Normal);
        assert!(!tracker.snapshot().unwrap().maximized);
    }

    #[test]
    fn nothing_is_saved_before_the_first_observation() {
        assert!(GeometryTracker::default().snapshot().is_none());
    }

    #[test]
    fn geometry_round_trips_through_the_state_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_none());

        let saved = WindowGeometry {
            maximized: true,
            ..geometry(140.0, 96.0, 1440.0, 900.0)
        };
        save(dir.path(), &saved).unwrap();

        assert_eq!(load(dir.path()).unwrap(), saved);
    }

    #[test]
    fn corrupt_state_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(paths::window_state_path(dir.path()), "{ not json").unwrap();

        assert!(load(dir.path()).is_none());
    }

    #[test]
    fn minimum_size_is_enforced_for_missing_or_broken_values() {
        let fitted = fit_minimum(geometry(f64::NAN, 10.0, 10.0, f64::INFINITY));
        assert_eq!((fitted.x, fitted.y), (0.0, 10.0));
        assert_eq!((fitted.width, fitted.height), (MIN_WIDTH, DEFAULT_HEIGHT));
    }
}
