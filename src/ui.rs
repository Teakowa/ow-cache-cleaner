use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1Factory,
    ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT, ScreenToClient};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetDpiForWindow,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::cache::{self, CleanResult, Kind, Target};
use crate::process;

const WIN_W: f32 = 420.0;
const TITLE_H: f32 = 36.0;
const MAX_GROUPS: usize = 3;
const WM_CLEAN_DONE: u32 = WM_APP + 1;
const TIMER_GAME: usize = 1;
const D2DERR_RECREATE_TARGET: i32 = 0x8899000Cu32 as i32;

const BG: u32 = 0xFFFFFF;
const BORDER: u32 = 0xD4D4D4;
const TEXT: u32 = 0x1A1A1A;
const TEXT_MID: u32 = 0x505050;
const TEXT_DIM: u32 = 0x707070;
const TEXT_FAINT: u32 = 0x909090;
const ROW_HOVER: u32 = 0xF6F6F6;
const BTN_HOVER: u32 = 0xE8E8E8;
const CAPTION_HOVER: u32 = 0xF0F0F0;
const CHECK_BORDER: u32 = 0x8A8A8A;
const ACCENT: u32 = 0x0F6CBD;
const ACCENT_HOVER: u32 = 0x115EA3;
const BUSY_BG: u32 = 0xEBEBEB;
const WARN: u32 = 0x9D5D00;
const OK: u32 = 0x107C10;

const ICON_MIN: &str = "\u{E921}";
const ICON_CLOSE: &str = "\u{E8BB}";
const ICON_FOLDER: &str = "\u{E838}";
const ICON_CHECK: &str = "\u{E73E}";

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    None,
    Minimize,
    Close,
    Row(usize),
    Open(usize),
    Go,
}

enum Phase {
    Idle,
    Cleaning,
    Done(CleanResult),
}

struct Row {
    target: Target,
    checked: bool,
}

struct Fonts {
    caption: IDWriteTextFormat,
    title: IDWriteTextFormat,
    size: IDWriteTextFormat,
    small: IDWriteTextFormat,
    small_right: IDWriteTextFormat,
    button: IDWriteTextFormat,
    icon: IDWriteTextFormat,
    icon_small: IDWriteTextFormat,
}

struct Canvas {
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
}

struct App {
    hwnd: HWND,
    local_app_data: PathBuf,
    d2d: ID2D1Factory,
    fonts: Fonts,
    canvas: Option<Canvas>,
    rows: Vec<Row>,
    phase: Phase,
    game_running: bool,
    hover: Hit,
    tracking: bool,
    dpi: f32,
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F { left, top, right, bottom }
}

fn color(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

fn contains(r: &D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn visible_groups(target: &Target) -> Vec<&cache::Group> {
    target.groups.iter().filter(|g| g.size > 0).collect()
}

fn row_height(target: &Target) -> f32 {
    let groups = visible_groups(target).len();
    let lines = groups.min(MAX_GROUPS) + usize::from(groups > MAX_GROUPS);
    let list = if lines == 0 { 0.0 } else { 6.0 + 18.0 * lines as f32 };
    12.0 + 20.0 + 2.0 + 16.0 + list + 12.0
}

fn text_format(
    dw: &IDWriteFactory,
    family: PCWSTR,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    align: DWRITE_TEXT_ALIGNMENT,
) -> IDWriteTextFormat {
    unsafe {
        let format = dw
            .CreateTextFormat(
                family,
                None::<&IDWriteFontCollection>,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("zh-cn"),
            )
            .expect("create text format");
        let _ = format.SetTextAlignment(align);
        let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        format
    }
}

impl Fonts {
    fn new(dw: &IDWriteFactory) -> Self {
        let ui = w!("Microsoft YaHei UI");
        let icons = w!("Segoe MDL2 Assets");
        let normal = DWRITE_FONT_WEIGHT_NORMAL;
        Fonts {
            caption: text_format(dw, ui, 12.0, normal, DWRITE_TEXT_ALIGNMENT_LEADING),
            title: text_format(dw, ui, 14.0, normal, DWRITE_TEXT_ALIGNMENT_LEADING),
            size: text_format(dw, ui, 13.0, normal, DWRITE_TEXT_ALIGNMENT_TRAILING),
            small: text_format(dw, ui, 12.0, normal, DWRITE_TEXT_ALIGNMENT_LEADING),
            small_right: text_format(dw, ui, 12.0, normal, DWRITE_TEXT_ALIGNMENT_TRAILING),
            button: text_format(dw, ui, 14.0, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_TEXT_ALIGNMENT_CENTER),
            icon: text_format(dw, icons, 16.0, normal, DWRITE_TEXT_ALIGNMENT_CENTER),
            icon_small: text_format(dw, icons, 12.0, normal, DWRITE_TEXT_ALIGNMENT_CENTER),
        }
    }
}

impl App {
    fn new(local_app_data: PathBuf) -> Self {
        unsafe {
            let d2d: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).expect("create Direct2D factory");
            let dw: IDWriteFactory =
                DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).expect("create DirectWrite factory");
            let rows = cache::discover(&local_app_data)
                .into_iter()
                .map(|target| Row { target, checked: true })
                .collect();
            App {
                hwnd: HWND::default(),
                local_app_data,
                d2d,
                fonts: Fonts::new(&dw),
                canvas: None,
                rows,
                phase: Phase::Idle,
                game_running: process::is_running("Overwatch.exe"),
                hover: Hit::None,
                tracking: false,
                dpi: GetDpiForSystem() as f32,
            }
        }
    }

    fn scale(&self) -> f32 {
        self.dpi / 96.0
    }

    fn row_top(&self, index: usize) -> f32 {
        TITLE_H + 4.0 + self.rows[..index].iter().map(|r| row_height(&r.target)).sum::<f32>()
    }

    fn list_bottom(&self) -> f32 {
        TITLE_H + 4.0 + self.rows.iter().map(|r| row_height(&r.target)).sum::<f32>() + 8.0
    }

    fn go_rect(&self) -> D2D_RECT_F {
        let top = self.list_bottom() + 8.0;
        rect(18.0, top, WIN_W - 18.0, top + 40.0)
    }

    fn height(&self) -> f32 {
        self.go_rect().bottom + 10.0 + 16.0 + 16.0
    }

    fn min_rect() -> D2D_RECT_F {
        rect(WIN_W - 64.0, 4.0, WIN_W - 32.0, 32.0)
    }

    fn close_rect() -> D2D_RECT_F {
        rect(WIN_W - 32.0, 4.0, WIN_W, 32.0)
    }

    fn open_rect(&self, index: usize) -> D2D_RECT_F {
        let top = self.row_top(index) + 8.0;
        rect(378.0, top, 406.0, top + 28.0)
    }

    fn hit_test(&self, x: f32, y: f32) -> Hit {
        if contains(&Self::min_rect(), x, y) {
            return Hit::Minimize;
        }
        if contains(&Self::close_rect(), x, y) {
            return Hit::Close;
        }
        for i in 0..self.rows.len() {
            if contains(&self.open_rect(i), x, y) {
                return Hit::Open(i);
            }
            let top = self.row_top(i);
            if contains(&rect(8.0, top, WIN_W - 8.0, top + row_height(&self.rows[i].target)), x, y) {
                return Hit::Row(i);
            }
        }
        if contains(&self.go_rect(), x, y) {
            return Hit::Go;
        }
        Hit::None
    }

    fn checked_count(&self) -> usize {
        self.rows.iter().filter(|r| r.checked).count()
    }

    fn selected_size(&self) -> u64 {
        self.rows.iter().filter(|r| r.checked).map(|r| r.target.size).sum()
    }

    /// Label, whether the button is inactive, status text, status color.
    fn button_state(&self) -> (String, bool, String, u32) {
        let none = String::new();
        if self.game_running {
            return ("关闭守望先锋后清理".into(), true, "检测到守望先锋正在运行".into(), WARN);
        }
        match &self.phase {
            Phase::Cleaning => ("清理中…".into(), true, none, TEXT_DIM),
            Phase::Done(result) => {
                let freed = cache::format_size(result.freed);
                if result.skipped > 0 {
                    let text = format!("已清理 {freed}，{} 个文件被占用，已跳过", result.skipped);
                    ("再次清理".into(), self.checked_count() == 0, text, WARN)
                } else {
                    let text = format!("已清理 {freed}，首次进入游戏会重新编译着色器");
                    ("再次清理".into(), self.checked_count() == 0, text, OK)
                }
            }
            Phase::Idle if self.rows.is_empty() => ("没有可清理的缓存".into(), true, none, TEXT_DIM),
            Phase::Idle if self.checked_count() == 0 => ("选择要清理的项目".into(), true, none, TEXT_DIM),
            Phase::Idle => (format!("清理 {}", cache::format_size(self.selected_size())), false, none, TEXT_DIM),
        }
    }

    fn refresh(&mut self) {
        let old: Vec<(Kind, bool)> = self.rows.iter().map(|r| (r.target.kind, r.checked)).collect();
        self.rows = cache::discover(&self.local_app_data)
            .into_iter()
            .map(|target| {
                let checked = old.iter().find(|(k, _)| *k == target.kind).is_none_or(|(_, c)| *c);
                Row { target, checked }
            })
            .collect();
    }

    fn fit_window(&self, center: bool) {
        unsafe {
            let s = self.scale();
            let (w, h) = ((WIN_W * s).round() as i32, (self.height() * s).round() as i32);
            let (x, y) = if center {
                (
                    (GetSystemMetrics(SM_CXSCREEN) - w) / 2,
                    (GetSystemMetrics(SM_CYSCREEN) - h) / 2,
                )
            } else {
                let mut r = RECT::default();
                let _ = GetWindowRect(self.hwnd, &mut r);
                (r.left, r.top)
            };
            let _ = SetWindowPos(self.hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    fn invalidate(&self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn ensure_canvas(&mut self) -> bool {
        if self.canvas.is_some() {
            return true;
        }
        unsafe {
            let mut client = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut client);
            let props = D2D1_RENDER_TARGET_PROPERTIES::default();
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: D2D_SIZE_U {
                    width: (client.right - client.left) as u32,
                    height: (client.bottom - client.top) as u32,
                },
                ..Default::default()
            };
            let Ok(target) = self.d2d.CreateHwndRenderTarget(&props, &hwnd_props) else {
                return false;
            };
            let Ok(brush) = target.CreateSolidColorBrush(&color(0), None) else {
                return false;
            };
            target.SetDpi(self.dpi, self.dpi);
            self.canvas = Some(Canvas { target, brush });
        }
        true
    }

    fn paint(&mut self) {
        if !self.ensure_canvas() {
            return;
        }
        let (button_label, inactive, status, status_color) = self.button_state();
        let canvas = self.canvas.as_ref().unwrap();
        let rt = &canvas.target;
        let brush = &canvas.brush;
        let f = &self.fonts;

        let fill = |r: D2D_RECT_F, radius: f32, rgb: u32| unsafe {
            brush.SetColor(&color(rgb));
            rt.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius },
                brush,
            );
        };
        let text = |s: &str, format: &IDWriteTextFormat, r: D2D_RECT_F, rgb: u32| unsafe {
            brush.SetColor(&color(rgb));
            let wide: Vec<u16> = s.encode_utf16().collect();
            rt.DrawText(
                &wide,
                format,
                &r,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        };

        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&color(BG)));

            text("着色器缓存清理", &f.caption, rect(16.0, 0.0, 250.0, TITLE_H), TEXT_DIM);
            for (rc, hit, glyph) in [
                (Self::min_rect(), Hit::Minimize, ICON_MIN),
                (Self::close_rect(), Hit::Close, ICON_CLOSE),
            ] {
                if self.hover == hit {
                    fill(rc, 4.0, CAPTION_HOVER);
                }
                text(glyph, &f.icon_small, rc, TEXT_MID);
            }

            for (i, row) in self.rows.iter().enumerate() {
                let top = self.row_top(i);
                let t = &row.target;
                if matches!(self.hover, Hit::Row(j) | Hit::Open(j) if j == i) {
                    fill(rect(8.0, top, WIN_W - 8.0, top + row_height(t)), 6.0, ROW_HOVER);
                }

                let cb = rect(18.0, top + 13.0, 36.0, top + 31.0);
                if row.checked {
                    fill(cb, 4.0, ACCENT);
                    text(ICON_CHECK, &f.icon_small, cb, 0xFFFFFF);
                } else {
                    fill(cb, 4.0, CHECK_BORDER);
                    fill(rect(cb.left + 1.5, cb.top + 1.5, cb.right - 1.5, cb.bottom - 1.5), 2.5, BG);
                    if matches!(self.hover, Hit::Row(j) | Hit::Open(j) if j == i) {
                        fill(rect(cb.left + 1.5, cb.top + 1.5, cb.right - 1.5, cb.bottom - 1.5), 2.5, ROW_HOVER);
                    }
                }

                let title_y = top + 12.0;
                text(t.name, &f.title, rect(48.0, title_y, 250.0, title_y + 20.0), TEXT);
                text(&cache::format_size(t.size), &f.size, rect(200.0, title_y, 374.0, title_y + 20.0), TEXT_MID);
                let open = self.open_rect(i);
                if self.hover == Hit::Open(i) {
                    fill(open, 4.0, BTN_HOVER);
                }
                text(ICON_FOLDER, &f.icon, open, TEXT_MID);
                text(&t.tag, &f.small, rect(48.0, title_y + 22.0, WIN_W - 18.0, title_y + 38.0), TEXT_DIM);

                let groups = visible_groups(t);
                let mut y = title_y + 44.0;
                for g in groups.iter().take(MAX_GROUPS) {
                    text(&g.name, &f.small, rect(48.0, y, 300.0, y + 18.0), TEXT_DIM);
                    text(&cache::format_size(g.size), &f.small_right, rect(300.0, y, 398.0, y + 18.0), TEXT_DIM);
                    y += 18.0;
                }
                if groups.len() > MAX_GROUPS {
                    text(
                        &format!("另有 {} 个", groups.len() - MAX_GROUPS),
                        &f.small,
                        rect(48.0, y, 300.0, y + 18.0),
                        TEXT_FAINT,
                    );
                }
            }

            let go = self.go_rect();
            let (bg, fg) = match (inactive, self.hover == Hit::Go) {
                (true, _) => (BUSY_BG, TEXT_DIM),
                (false, true) => (ACCENT_HOVER, 0xFFFFFF),
                (false, false) => (ACCENT, 0xFFFFFF),
            };
            fill(go, 6.0, bg);
            text(&button_label, &f.button, go, fg);
            text(&status, &f.caption, rect(18.0, go.bottom + 10.0, WIN_W - 18.0, go.bottom + 26.0), status_color);

            brush.SetColor(&color(BORDER));
            rt.DrawRectangle(&rect(0.5, 0.5, WIN_W - 0.5, self.height() - 0.5), brush, 1.0, None);

            if let Err(e) = rt.EndDraw(None, None) {
                if e.code().0 == D2DERR_RECREATE_TARGET {
                    self.canvas = None;
                }
            }
        }
    }

    fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Minimize => unsafe {
                let _ = ShowWindow(self.hwnd, SW_MINIMIZE);
            },
            Hit::Close => unsafe {
                let _ = DestroyWindow(self.hwnd);
            },
            Hit::Row(i) => {
                self.rows[i].checked = !self.rows[i].checked;
                if matches!(self.phase, Phase::Done(_)) {
                    self.phase = Phase::Idle;
                }
                self.invalidate();
            }
            Hit::Open(i) => unsafe {
                let path: Vec<u16> = self.rows[i].target.open.to_string_lossy().encode_utf16().chain([0]).collect();
                ShellExecuteW(None, w!("open"), PCWSTR(path.as_ptr()), None, None, SW_SHOWNORMAL);
            },
            Hit::Go => self.start_clean(),
            Hit::None => {}
        }
    }

    fn start_clean(&mut self) {
        self.game_running = process::is_running("Overwatch.exe");
        let (_, inactive, _, _) = self.button_state();
        if inactive {
            self.invalidate();
            return;
        }
        let dirs: Vec<PathBuf> = self
            .rows
            .iter()
            .filter(|r| r.checked)
            .flat_map(|r| r.target.dirs.clone())
            .collect();
        self.phase = Phase::Cleaning;
        self.invalidate();
        let hwnd = self.hwnd.0 as isize;
        std::thread::spawn(move || {
            let result = Box::new(cache::clean(&dirs));
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    WM_CLEAN_DONE,
                    WPARAM(0),
                    LPARAM(Box::into_raw(result) as isize),
                );
            }
        });
    }

    fn finish_clean(&mut self, result: CleanResult) {
        self.phase = Phase::Done(result);
        self.refresh();
        self.fit_window(false);
        self.invalidate();
    }
}

unsafe fn app<'a>(hwnd: HWND) -> Option<&'a mut App> {
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut() }
}

fn point_of(lparam: LPARAM) -> (i32, i32) {
    (lparam.0 as i16 as i32, (lparam.0 >> 16) as i16 as i32)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            let app = create.lpCreateParams as *mut App;
            (*app).hwnd = hwnd;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let Some(app) = app(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                app.paint();
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                if let Some(canvas) = &app.canvas {
                    let (w, h) = point_of(lparam);
                    let _ = canvas.target.Resize(&D2D_SIZE_U { width: w as u32, height: h as u32 });
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                app.dpi = (wparam.0 & 0xFFFF) as f32;
                if let Some(canvas) = &app.canvas {
                    canvas.target.SetDpi(app.dpi, app.dpi);
                }
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let (x, y) = point_of(lparam);
                let mut p = POINT { x, y };
                let _ = ScreenToClient(hwnd, &mut p);
                let (dx, dy) = (p.x as f32 / app.scale(), p.y as f32 / app.scale());
                if dy >= 0.0 && dy < TITLE_H && app.hit_test(dx, dy) == Hit::None {
                    LRESULT(HTCAPTION as isize)
                } else {
                    LRESULT(HTCLIENT as isize)
                }
            }
            WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
                let id = if app.hover == Hit::None { IDC_ARROW } else { IDC_HAND };
                SetCursor(LoadCursorW(None, id).ok());
                LRESULT(1)
            }
            WM_MOUSEMOVE => {
                let (x, y) = point_of(lparam);
                let hit = app.hit_test(x as f32 / app.scale(), y as f32 / app.scale());
                if hit != app.hover {
                    app.hover = hit;
                    app.invalidate();
                }
                if !app.tracking {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                    app.tracking = true;
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                app.tracking = false;
                app.hover = Hit::None;
                app.invalidate();
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let (x, y) = point_of(lparam);
                let hit = app.hit_test(x as f32 / app.scale(), y as f32 / app.scale());
                app.click(hit);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                match wparam.0 as u16 {
                    k if k == VK_ESCAPE.0 => {
                        let _ = DestroyWindow(hwnd);
                    }
                    k if k == VK_RETURN.0 => app.start_clean(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_TIMER => {
                let running = process::is_running("Overwatch.exe");
                if running != app.game_running {
                    app.game_running = running;
                    app.invalidate();
                }
                LRESULT(0)
            }
            WM_CLEAN_DONE => {
                let result = Box::from_raw(lparam.0 as *mut CleanResult);
                app.finish_clean(*result);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub fn run() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
        let app = Box::into_raw(Box::new(App::new(local)));

        let instance = GetModuleHandleW(None).expect("module handle");
        let class = w!("ShaderCacheCleaner");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class,
            w!("着色器缓存清理"),
            WS_POPUP | WS_MINIMIZEBOX,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            Some(app as *const c_void),
        )
        .expect("create window");

        let app = &mut *app;
        app.dpi = GetDpiForWindow(hwnd) as f32;
        let margins = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let round: i32 = 2;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
        app.fit_window(true);
        SetTimer(Some(hwnd), TIMER_GAME, 1500, None);
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(Box::from_raw(app as *mut App));
    }
}
