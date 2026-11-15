use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

use super::App;
use crate::cache::{self, Target};

pub(super) const WIN_W: f32 = 420.0;
pub(super) const TITLE_H: f32 = 36.0;
pub(super) const MAX_GROUPS: usize = 3;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Hit {
    None,
    Minimize,
    Close,
    Info,
    Link,
    Update,
    Backdrop,
    Row(usize),
    Open(usize),
    Go,
}

pub(super) fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F { left, top, right, bottom }
}

pub(super) fn contains(r: &D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

pub(super) fn visible_groups(target: &Target) -> Vec<&cache::Group> {
    target.groups.iter().filter(|g| g.size > 0).collect()
}

pub(super) fn row_height(target: &Target) -> f32 {
    let groups = visible_groups(target).len();
    let lines = groups.min(MAX_GROUPS) + usize::from(groups > MAX_GROUPS);
    let list = if lines == 0 { 0.0 } else { 6.0 + 18.0 * lines as f32 };
    12.0 + 20.0 + 2.0 + 16.0 + list + 12.0
}

impl App {
    pub(super) fn scale(&self) -> f32 {
        self.dpi / 96.0
    }

    pub(super) fn row_top(&self, index: usize) -> f32 {
        TITLE_H + 4.0 + self.rows[..index].iter().map(|r| row_height(&r.target)).sum::<f32>()
    }

    pub(super) fn list_bottom(&self) -> f32 {
        TITLE_H + 4.0 + self.rows.iter().map(|r| row_height(&r.target)).sum::<f32>() + 8.0
    }

    pub(super) fn go_rect(&self) -> D2D_RECT_F {
        let top = self.list_bottom() + 8.0;
        rect(18.0, top, WIN_W - 18.0, top + 40.0)
    }

    pub(super) fn height(&self) -> f32 {
        (self.go_rect().bottom + 10.0 + 16.0 + 16.0).max(200.0)
    }

    pub(super) fn info_rect() -> D2D_RECT_F {
        rect(WIN_W - 96.0, 4.0, WIN_W - 64.0, 32.0)
    }

    pub(super) fn min_rect() -> D2D_RECT_F {
        rect(WIN_W - 64.0, 4.0, WIN_W - 32.0, 32.0)
    }

    pub(super) fn panel_rect() -> D2D_RECT_F {
        rect(18.0, 44.0, WIN_W - 18.0, 170.0)
    }

    pub(super) fn link_rect() -> D2D_RECT_F {
        rect(34.0, 108.0, 300.0, 132.0)
    }

    pub(super) fn update_rect() -> D2D_RECT_F {
        rect(34.0, 134.0, 300.0, 158.0)
    }

    pub(super) fn close_rect() -> D2D_RECT_F {
        rect(WIN_W - 32.0, 4.0, WIN_W, 32.0)
    }

    pub(super) fn open_rect(&self, index: usize) -> D2D_RECT_F {
        let top = self.row_top(index) + 8.0;
        rect(378.0, top, 406.0, top + 28.0)
    }

    pub(super) fn hit_test(&self, x: f32, y: f32) -> Hit {
        if contains(&Self::min_rect(), x, y) {
            return Hit::Minimize;
        }
        if contains(&Self::close_rect(), x, y) {
            return Hit::Close;
        }
        if contains(&Self::info_rect(), x, y) {
            return Hit::Info;
        }
        if self.about {
            return if contains(&Self::link_rect(), x, y) {
                Hit::Link
            } else if contains(&Self::update_rect(), x, y) {
                Hit::Update
            } else if contains(&Self::panel_rect(), x, y) {
                Hit::None
            } else {
                Hit::Backdrop
            };
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
}
