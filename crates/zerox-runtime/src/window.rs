//! Window manager — composites application windows onto the display.

pub struct WindowManager {
    pub windows: Vec<Window>,
    pub focused: Option<u64>,
    pub next_id: u64,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: u64,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub visible: bool,
    pub is_game: bool,
}

impl WindowManager {
    pub fn new() -> Self { Self { windows: Vec::new(), focused: None, next_id: 1 } }

    pub fn create_window(&mut self, title: impl Into<String>, x: i32, y: i32, w: u32, h: u32) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let win = Window { id, title: title.into(), x, y, w, h, visible: true, is_game: false };
        log::info!("[wm] +window id={} '{}'", id, win.title);
        self.windows.push(win);
        self.focused = Some(id);
        id
    }

    pub fn destroy_window(&mut self, id: u64) {
        self.windows.retain(|w| w.id != id);
        if self.focused == Some(id) { self.focused = None; }
        log::info!("[wm] -window id={}", id);
    }

    pub fn focus(&mut self, id: u64) {
        self.focused = Some(id);
        log::trace!("[wm] focus id={}", id);
    }

    pub fn window_count(&self) -> usize { self.windows.len() }
}
