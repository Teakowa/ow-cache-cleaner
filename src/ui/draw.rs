use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_ROUNDED_RECT, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_MEDIUM,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory, IDWriteFontCollection,
    IDWriteTextFormat,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{PCWSTR, w};

use super::layout::*;
use super::{APP_NAME, App, REPO_LABEL};
use crate::cache;

const D2DERR_RECREATE_TARGET: i32 = 0x8899000Cu32 as i32;

pub(super) const BG: u32 = 0xFFFFFF;
pub(super) const BORDER: u32 = 0xD4D4D4;
pub(super) const TEXT: u32 = 0x1A1A1A;
pub(super) const TEXT_MID: u32 = 0x505050;
pub(super) const TEXT_DIM: u32 = 0x707070;
pub(super) const TEXT_FAINT: u32 = 0x909090;
pub(super) const ROW_HOVER: u32 = 0xF6F6F6;
pub(super) const BTN_HOVER: u32 = 0xE8E8E8;
pub(super) const CAPTION_HOVER: u32 = 0xF0F0F0;
pub(super) const CHECK_BORDER: u32 = 0x8A8A8A;
pub(super) const ACCENT: u32 = 0x0F6CBD;
pub(super) const ACCENT_HOVER: u32 = 0x115EA3;
pub(super) const BUSY_BG: u32 = 0xEBEBEB;
pub(super) const WARN: u32 = 0x9D5D00;
pub(super) const OK: u32 = 0x107C10;

pub(super) const ICON_INFO: &str = "\u{E946}";
pub(super) const ICON_MIN: &str = "\u{E921}";
pub(super) const ICON_CLOSE: &str = "\u{E8BB}";
pub(super) const ICON_FOLDER: &str = "\u{E838}";
pub(super) const ICON_CHECK: &str = "\u{E73E}";

pub(super) struct Fonts {
    caption: IDWriteTextFormat,
    title: IDWriteTextFormat,
    size: IDWriteTextFormat,
    small: IDWriteTextFormat,
    small_right: IDWriteTextFormat,
    button: IDWriteTextFormat,
    icon: IDWriteTextFormat,
    icon_small: IDWriteTextFormat,
}

pub(super) struct Canvas {
    pub(super) target: ID2D1HwndRenderTarget,
    pub(super) brush: ID2D1SolidColorBrush,
}

pub(super) fn color(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
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
    pub(super) fn new(dw: &IDWriteFactory) -> Self {
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
    pub(super) fn ensure_canvas(&mut self) -> bool {
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

    pub(super) fn paint(&mut self) {
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

            text(APP_NAME, &f.caption, rect(16.0, 0.0, 250.0, TITLE_H), TEXT_DIM);
            for (rc, hit, glyph) in [
                (Self::info_rect(), Hit::Info, ICON_INFO),
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

            if self.about {
                let mut veil = color(BG);
                veil.a = 0.8;
                brush.SetColor(&veil);
                rt.FillRectangle(&rect(0.0, TITLE_H, WIN_W, self.height()), brush);
                let panel = Self::panel_rect();
                fill(panel, 8.0, BORDER);
                fill(rect(panel.left + 1.0, panel.top + 1.0, panel.right - 1.0, panel.bottom - 1.0), 7.0, BG);
                let x = panel.left + 16.0;
                text(APP_NAME, &f.title, rect(x, panel.top + 12.0, x + 220.0, panel.top + 36.0), TEXT);
                text(
                    concat!("v", env!("CARGO_PKG_VERSION")),
                    &f.small_right,
                    rect(panel.right - 96.0, panel.top + 12.0, panel.right - 16.0, panel.top + 36.0),
                    TEXT_DIM,
                );
                text("MIT 许可 · 非暴雪官方工具", &f.small, rect(x, panel.top + 40.0, panel.right - 16.0, panel.top + 60.0), TEXT_DIM);
                let link = if self.hover == Hit::Link { ACCENT_HOVER } else { ACCENT };
                text(REPO_LABEL, &f.title, Self::link_rect(), link);
            }

            brush.SetColor(&color(BORDER));
            rt.DrawRectangle(&rect(0.5, 0.5, WIN_W - 0.5, self.height() - 0.5), brush, 1.0, None);

            if let Err(e) = rt.EndDraw(None, None) {
                if e.code().0 == D2DERR_RECREATE_TARGET {
                    self.canvas = None;
                }
            }
        }
    }
}
