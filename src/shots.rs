//! 无窗口（headless）软件渲染 → PNG：视觉自证 / 像素回归的工具层。
//!
//! 做法（PROBE §2 的实测结论；探针脚手架已在收尾时移除，结论见 `PROBE.md`）：
//! - 平台实现只提供 `create_window_adapter`，返回 `MinimalSoftwareWindow`；
//! - 每张图都**新建组件**（= 新建 Window），所以 DPR 不会被锁死：
//!   `dispatch_event(ScaleFactorChanged)` → `set_size` → `show()` → `request_redraw()`；
//! - 渲染目标必须是 `PremultipliedRgbaColor`（`Rgba8Pixel` 没实现 `TargetPixel`）；
//! - PNG 编码走 tiny-skia（feature `png-format` 默认开），写盘前手工反预乘。
//!
//! 窗口适配器**按组件实例**解析（[`WindowSlot::for_component`]）：平台把创建过的每个适配器都
//! 记在线程本地注册表里，解析时用 `Window` 的指针身份匹配 —— 调用方无法再靠"谁最后被创建"
//! 出错，同一进程里的多个组件各拿各的窗口。
//!
//! 与探针阶段的差别只有一个：这里额外记录**单帧 `draw_if_needed` 耗时**，供性能验收。

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalSize, PhysicalSize, SharedPixelBuffer};

/// 本线程 headless 平台创建过的全部窗口适配器（Slint 的平台是线程本地的）。
type Registry = Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>;

thread_local! {
    /// `install()` 装好的平台句柄（未绑定到具体组件），供 [`WindowSlot::for_component`] 查表。
    static PLATFORM_SLOT: RefCell<Option<WindowSlot>> = const { RefCell::new(None) };
}

/// headless 平台的句柄；也可以绑定到**某一个**组件实例的窗口适配器。
///
/// - `install()` 返回的是**未绑定**的句柄：渲染时按组件身份现场解析；
/// - [`WindowSlot::for_component`] 返回**已绑定**的句柄：只服务那一个组件，传错组件直接报错。
#[derive(Clone, Default)]
pub struct WindowSlot {
    registry: Registry,
    bound: Option<Rc<MinimalSoftwareWindow>>,
}

impl WindowSlot {
    /// 取 `ui` 自己的窗口适配器，返回**独立**的 slot。
    ///
    /// 解析依据是 `Window` 的指针身份（`ui.window()` == `adapter.window()`），与组件/窗口的
    /// 创建顺序无关：同一进程里两个组件并存、交错创建，也各自拿到自己的窗口。
    pub fn for_component(ui: &impl ComponentHandle) -> Result<Self, Box<dyn std::error::Error>> {
        PLATFORM_SLOT.with(|cell| {
            let borrow = cell.borrow();
            let platform = borrow
                .as_ref()
                .ok_or("headless 平台还没安装：先调用 shots::install()")?;
            platform.bind_to(ui)
        })
    }

    /// 绑定到 `ui`（已绑定的 slot 会校验身份，不会静默改绑到别的组件）。
    fn bind_to(&self, ui: &impl ComponentHandle) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(window) = &self.bound {
            if std::ptr::eq(window.window(), ui.window()) {
                return Ok(self.clone());
            }
            return Err(
                "该 slot 已绑定到另一个组件实例；请用 WindowSlot::for_component(&ui) 取本实例的"
                    .into(),
            );
        }
        let target = ui.window();
        let found = self
            .registry
            .borrow()
            .iter()
            .find(|window| std::ptr::eq(window.window(), target))
            .cloned();
        found
            .map(|window| Self {
                registry: self.registry.clone(),
                bound: Some(window),
            })
            .ok_or_else(|| "该组件没有窗口适配器（不是由 headless 平台创建的？）".into())
    }

    /// 解析出真正要驱动的窗口适配器。
    fn resolve(
        &self,
        ui: &impl ComponentHandle,
    ) -> Result<Rc<MinimalSoftwareWindow>, Box<dyn std::error::Error>> {
        match self.bind_to(ui)?.bound {
            Some(window) => Ok(window),
            None => Err("内部错误：绑定后仍没有窗口适配器".into()),
        }
    }
}

struct HeadlessPlatform {
    registry: Registry,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.registry.borrow_mut().push(window.clone());
        Ok(window)
    }

    /// 不跑阻塞事件循环；需要推进定时器 / 动画时手动调用
    /// `slint::platform::update_timers_and_animations()`。
    fn run_event_loop(&self) -> Result<(), PlatformError> {
        Ok(())
    }
}

/// 安装 headless 平台。**整个线程只能调用一次**（Slint 的平台是进程级全局的）。
pub fn install() -> Result<WindowSlot, Box<dyn std::error::Error>> {
    let slot = WindowSlot::default();
    slint::platform::set_platform(Box::new(HeadlessPlatform {
        registry: slot.registry.clone(),
    }))
    .map_err(|e| format!("set_platform failed: {e:?}"))?;
    PLATFORM_SLOT.with(|cell| *cell.borrow_mut() = Some(slot.clone()));
    Ok(slot)
}

/// 一帧的结果。
pub struct Shot {
    /// 位图宽（物理像素）。
    pub width: u32,
    /// 位图高（物理像素）。
    pub height: u32,
    /// 渲染时用的 DPR。
    pub scale_factor: f32,
    /// 预乘 alpha 的 RGBA 位图。
    pub buffer: SharedPixelBuffer<PremultipliedRgbaColor>,
    /// `draw_if_needed` 是否真的重画了。
    pub drew: bool,
    /// 本次 `draw_if_needed` 的耗时（= 一帧光栅化时间，不含 PNG 编码）。
    pub draw_time: Duration,
}

impl Shot {
    /// 非背景像素数（背景 = 全黑；只有严格全黑才算背景）。
    pub fn non_black_pixels(&self) -> usize {
        self.buffer
            .as_slice()
            .iter()
            .filter(|p| p.red != 0 || p.green != 0 || p.blue != 0)
            .count()
    }

    /// 取一个像素的预乘 RGBA（越界返回 `None`）。
    pub fn pixel(&self, x: u32, y: u32) -> Option<PremultipliedRgbaColor> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.buffer
            .as_slice()
            .get((y * self.width + x) as usize)
            .copied()
    }
}

/// 把组件渲染成一张预乘 RGBA 位图，**任意逻辑尺寸**。
///
/// - `logical`：逻辑宽高（Slint 布局单位，即 `.slint` 里写的 px），`(宽, 高)`；
/// - `scale_factor`：DPR，物理像素 = `logical * scale_factor`（逐轴取整）。
pub fn render_sized(
    ui: &impl ComponentHandle,
    slot: &WindowSlot,
    logical: (f32, f32),
    scale_factor: f32,
) -> Result<Shot, Box<dyn std::error::Error>> {
    let window = slot.resolve(ui)?;

    // 公开 API 里唯一能改 DPR 的途径：派发 ScaleFactorChanged（必须早于 set_size）。
    window
        .window()
        .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor });
    window.set_size(LogicalSize::new(logical.0, logical.1));
    ui.show()?;
    window.window().request_redraw();

    let physical: PhysicalSize = window.window().size();
    let (pw, ph) = (physical.width.max(1), physical.height.max(1));

    let mut out: Option<SharedPixelBuffer<PremultipliedRgbaColor>> = None;
    let started = Instant::now();
    let drew = window.draw_if_needed(|renderer| {
        let mut buffer = SharedPixelBuffer::<PremultipliedRgbaColor>::new(pw, ph);
        let stride = buffer.width() as usize;
        let _region = renderer.render(buffer.make_mut_slice(), stride);
        out = Some(buffer);
    });
    let draw_time = started.elapsed();

    let buffer = match out {
        Some(b) => b,
        None => SharedPixelBuffer::<PremultipliedRgbaColor>::new(pw, ph),
    };

    Ok(Shot {
        width: buffer.width(),
        height: buffer.height(),
        scale_factor,
        buffer,
        drew,
        draw_time,
    })
}

/// 正方形便捷入口：等价于 `render_sized(ui, slot, (logical, logical), scale_factor)`。
///
/// 仪表组件是正方形的（`viewBox 0 0 286 286`），旧调用点因此保持不变；需要矩形窗口
/// （例如整个 Demo 面板）时用 [`render_sized`]。
pub fn render(
    ui: &impl ComponentHandle,
    slot: &WindowSlot,
    logical: f32,
    scale_factor: f32,
) -> Result<Shot, Box<dyn std::error::Error>> {
    render_sized(ui, slot, (logical, logical), scale_factor)
}

/// 保存为 PNG（反预乘后写盘）。
pub fn save_png(shot: &Shot, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let size = tiny_skia::IntSize::from_wh(shot.width, shot.height).ok_or("bad image size")?;
    let mut data = Vec::with_capacity((shot.width * shot.height * 4) as usize);
    for px in shot.buffer.as_slice() {
        let a = px.alpha;
        let un = |c: u8| -> u8 {
            if a == 0 {
                0
            } else if a == 255 {
                c
            } else {
                ((c as u32 * 255 + (a as u32 / 2)) / a as u32).min(255) as u8
            }
        };
        data.extend_from_slice(&[un(px.red), un(px.green), un(px.blue), a]);
    }
    let pixmap = tiny_skia::Pixmap::from_vec(data, size).ok_or("bad pixel buffer")?;
    let png = pixmap.encode_png()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, png)?;
    Ok(())
}

/// 渲染 + 落盘一步到位（任意逻辑尺寸）。
pub fn render_to_png_sized(
    ui: &impl ComponentHandle,
    slot: &WindowSlot,
    path: &Path,
    logical: (f32, f32),
    scale_factor: f32,
) -> Result<Shot, Box<dyn std::error::Error>> {
    let shot = render_sized(ui, slot, logical, scale_factor)?;
    save_png(&shot, path)?;
    Ok(shot)
}

/// 正方形便捷入口（见 [`render`]）。
pub fn render_to_png(
    ui: &impl ComponentHandle,
    slot: &WindowSlot,
    path: &Path,
    logical: f32,
    scale_factor: f32,
) -> Result<Shot, Box<dyn std::error::Error>> {
    let shot = render(ui, slot, logical, scale_factor)?;
    save_png(&shot, path)?;
    Ok(shot)
}
