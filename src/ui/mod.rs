use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U;
use windows::Win32::Graphics::Direct2D::{D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Factory};
use windows::Win32::Graphics::DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory};
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

mod draw;
mod layout;

use self::draw::{Canvas, Fonts, OK, TEXT_DIM, WARN};
use self::layout::*;

const WM_CLEAN_DONE: u32 = WM_APP + 1;
const TIMER_GAME: usize = 1;

const APP_NAME: &str = "OW Cache Cleaner";
const REPO_URL: &str = "https://github.com/Teakowa/ow-cache-cleaner";
const REPO_LABEL: &str = "github.com/Teakowa/ow-cache-cleaner";

enum Phase {
    Idle,
    Cleaning,
    Done(CleanResult),
}

struct Row {
    target: Target,
    checked: bool,
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
    about: bool,
    dpi: f32,
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
                about: false,
                dpi: GetDpiForSystem() as f32,
            }
        }
    }

    fn checked_count(&self) -> usize {
        self.rows.iter().filter(|r| r.checked).count()
    }

    fn selected_size(&self) -> u64 {
        self.rows.iter().filter(|r| r.checked).map(|r| r.target.size).sum()
    }

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
            Hit::Info => {
                self.about = !self.about;
                self.invalidate();
            }
            Hit::Backdrop => {
                self.about = false;
                self.invalidate();
            }
            Hit::Link => unsafe {
                let url: Vec<u16> = REPO_URL.encode_utf16().chain([0]).collect();
                ShellExecuteW(None, w!("open"), PCWSTR(url.as_ptr()), None, None, SW_SHOWNORMAL);
                self.about = false;
                self.invalidate();
            },
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
                if dy >= 0.0 && dy < TITLE_H && matches!(app.hit_test(dx, dy), Hit::None | Hit::Backdrop) {
                    LRESULT(HTCAPTION as isize)
                } else {
                    LRESULT(HTCLIENT as isize)
                }
            }
            WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
                let id = if matches!(app.hover, Hit::None | Hit::Backdrop) { IDC_ARROW } else { IDC_HAND };
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
                        if app.about {
                            app.about = false;
                            app.invalidate();
                        } else {
                            let _ = DestroyWindow(hwnd);
                        }
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
        let icon = LoadIconW(Some(instance.into()), PCWSTR(1 as *const u16)).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hIcon: icon,
            hIconSm: icon,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class,
            w!("OW Cache Cleaner"),
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
