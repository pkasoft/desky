//! Native X11 XEmbed System Tray Icon & Context Menu (Linux)
//!
//! Integrates with `_NET_SYSTEM_TRAY_S0` (LXQt/Lubuntu, XFCE, KDE, MATE, Cinnamon, i3):
//! - Docks a 24x24 X11 icon window into the desktop panel tray
//! - Renders a crisp emerald Resky icon via `PixelCanvas`
//! - Left-click restores & focuses the main Resky window
//! - Right-click opens an override-redirect X11 menu ("Open Resky" / "Quit Resky")

use std::sync::atomic::Ordering;
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use crate::linux::ui::launcher::{bring_main_window_to_front, RUNNING};
use crate::linux::ui::theme::{PixelCanvas, PK_CARD, PK_CARD_BORDER, PK_GREEN, PK_GREEN_DARK, PK_RED, PK_TEXT};

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;

pub struct LinuxTray {
    pub tray_win: Window,
    pub gc: Gcontext,
    pub menu_win: Option<Window>,
    pub menu_gc: Option<Gcontext>,
    pub menu_hover: i32,
}

impl LinuxTray {
    pub fn try_create(conn: &impl Connection, screen_num: usize, screen: &Screen) -> Option<Self> {
        let tray_atom_name = format!("_NET_SYSTEM_TRAY_S{}", screen_num);
        let tray_atom = conn.intern_atom(false, tray_atom_name.as_bytes()).ok()?.reply().ok()?.atom;
        let owner = conn.get_selection_owner(tray_atom).ok()?.reply().ok()?.owner;
        if owner == x11rb::NONE {
            info!("No XEmbed system tray manager active for {}; tray icon skipped.", tray_atom_name);
            return None;
        }

        let tray_win = conn.generate_id().ok()?;
        conn.create_window(
            screen.root_depth,
            tray_win,
            screen.root,
            0,
            0,
            24,
            24,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .background_pixel(screen.black_pixel)
                .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS | EventMask::STRUCTURE_NOTIFY),
        )
        .ok()?;

        // Set _XEMBED_INFO = [0, 1] (version 0, XEMBED_MAPPED = 1)
        if let Ok(xembed_info) = conn.intern_atom(false, b"_XEMBED_INFO").ok()?.reply() {
            let _ = conn.change_property32(
                PropMode::REPLACE,
                tray_win,
                xembed_info.atom,
                xembed_info.atom,
                &[0, 1],
            );
        }

        // Send _NET_SYSTEM_TRAY_OPCODE (SYSTEM_TRAY_REQUEST_DOCK) to tray owner
        let opcode_atom = conn.intern_atom(false, b"_NET_SYSTEM_TRAY_OPCODE").ok()?.reply().ok()?.atom;
        let ev = ClientMessageEvent::new(
            32,
            owner,
            opcode_atom,
            [x11rb::CURRENT_TIME, SYSTEM_TRAY_REQUEST_DOCK, tray_win, 0, 0],
        );
        conn.send_event(false, owner, EventMask::NO_EVENT, ev).ok()?;

        let gc = conn.generate_id().ok()?;
        conn.create_gc(gc, tray_win, &CreateGCAux::new().graphics_exposures(0)).ok()?;
        let _ = conn.flush();

        info!("Docked Resky XEmbed tray icon (0x{:x}) into tray manager 0x{:x}", tray_win, owner);

        Some(Self {
            tray_win,
            gc,
            menu_win: None,
            menu_gc: None,
            menu_hover: -1,
        })
    }

    pub fn draw_icon(&self, conn: &impl Connection, screen: &Screen) {
        let size: i32 = 24;
        let mut canvas = PixelCanvas::new(size as u16, size as u16);
        canvas.fill_rect(0, 0, size, size, PK_GREEN);
        canvas.draw_rounded_box(1, 1, size - 2, size - 2, PK_GREEN, PK_GREEN_DARK, 4);

        // White "D" mark in center
        for y in 6..18 {
            for x in 7..17 {
                let is_left = x >= 7 && x <= 9;
                let is_top = (y >= 6 && y <= 8) && x <= 14;
                let is_bot = (y >= 15 && y <= 17) && x <= 14;
                let is_right = (x >= 14 && x <= 16) && (y >= 8 && y <= 15);
                if is_left || is_top || is_bot || is_right {
                    canvas.set_pixel(x, y, 0x00FFFFFF);
                }
            }
        }

        canvas.present_to_x11(conn, self.tray_win, self.gc, screen.root_depth);
        let _ = conn.flush();
    }

    pub fn show_menu(&mut self, conn: &impl Connection, screen: &Screen, root_x: i16, root_y: i16) {
        self.close_menu(conn);

        let w = 150u16;
        let h = 68u16;
        let mx = (root_x as i32).min(screen.width_in_pixels as i32 - w as i32 - 8).max(4) as i16;
        let my = if (root_y as i32) > h as i32 + 24 {
            (root_y as i32 - h as i32 - 4) as i16
        } else {
            root_y + 4
        };

        if let (Ok(mwin), Ok(mgc)) = (conn.generate_id(), conn.generate_id()) {
            let _ = conn.create_window(
                screen.root_depth,
                mwin,
                screen.root,
                mx,
                my,
                w,
                h,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new()
                    .override_redirect(1)
                    .background_pixel(screen.white_pixel)
                    .event_mask(
                        EventMask::EXPOSURE
                            | EventMask::BUTTON_PRESS
                            | EventMask::POINTER_MOTION
                            | EventMask::LEAVE_WINDOW,
                    ),
            );
            let _ = conn.create_gc(mgc, mwin, &CreateGCAux::new().graphics_exposures(0));
            let _ = conn.map_window(mwin);
            let _ = conn.configure_window(mwin, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
            self.menu_win = Some(mwin);
            self.menu_gc = Some(mgc);
            self.menu_hover = -1;
            self.draw_menu(conn, screen);
        }
    }

    pub fn draw_menu(&self, conn: &impl Connection, screen: &Screen) {
        let (Some(mwin), Some(mgc)) = (self.menu_win, self.menu_gc) else {
            return;
        };
        let w: i32 = 150;
        let h: i32 = 68;
        let mut canvas = PixelCanvas::new(w as u16, h as u16);
        canvas.draw_rounded_box(0, 0, w, h, PK_CARD, PK_CARD_BORDER, 4);

        // Item 0: Open Resky
        let bg0 = if self.menu_hover == 0 { 0x00E7EDE5 } else { PK_CARD };
        canvas.fill_rect(2, 4, w - 4, 28, bg0);
        canvas.draw_text(14, 10, "Open Resky", 13.0, true, PK_TEXT, Some(w - 24));

        // Divider
        canvas.fill_rect(8, 34, w - 16, 1, PK_CARD_BORDER);

        // Item 1: Quit Resky
        let bg1 = if self.menu_hover == 1 { 0x00FDE8E8 } else { PK_CARD };
        canvas.fill_rect(2, 36, w - 4, 28, bg1);
        canvas.draw_text(14, 42, "Quit Resky", 13.0, true, PK_RED, Some(w - 24));

        canvas.present_to_x11(conn, mwin, mgc, screen.root_depth);
        let _ = conn.flush();
    }

    pub fn close_menu(&mut self, conn: &impl Connection) {
        if let Some(mgc) = self.menu_gc.take() {
            let _ = conn.free_gc(mgc);
        }
        if let Some(mwin) = self.menu_win.take() {
            let _ = conn.destroy_window(mwin);
            let _ = conn.flush();
        }
        self.menu_hover = -1;
    }

    /// Returns true if the X11 event belonged to the tray or its popup menu.
    pub fn handle_event(
        &mut self,
        conn: &impl Connection,
        screen: &Screen,
        event: &x11rb::protocol::Event,
    ) -> bool {
        match event {
            x11rb::protocol::Event::Expose(ex) => {
                if ex.window == self.tray_win {
                    if ex.count == 0 {
                        self.draw_icon(conn, screen);
                    }
                    return true;
                }
                if Some(ex.window) == self.menu_win {
                    if ex.count == 0 {
                        self.draw_menu(conn, screen);
                    }
                    return true;
                }
            }
            x11rb::protocol::Event::ButtonPress(bp) => {
                if bp.event == self.tray_win {
                    if bp.detail == 1 {
                        self.close_menu(conn);
                        bring_main_window_to_front();
                    } else if bp.detail == 3 {
                        self.show_menu(conn, screen, bp.root_x, bp.root_y);
                    }
                    return true;
                }
                if Some(bp.event) == self.menu_win {
                    if bp.event_y < 34 {
                        self.close_menu(conn);
                        bring_main_window_to_front();
                    } else {
                        self.close_menu(conn);
                        RUNNING.store(false, Ordering::SeqCst);
                    }
                    return true;
                }
                if self.menu_win.is_some() {
                    self.close_menu(conn);
                }
            }
            x11rb::protocol::Event::MotionNotify(mn) => {
                if Some(mn.event) == self.menu_win {
                    let new_hover = if mn.event_y < 34 { 0 } else { 1 };
                    if new_hover != self.menu_hover {
                        self.menu_hover = new_hover;
                        self.draw_menu(conn, screen);
                    }
                    return true;
                }
            }
            x11rb::protocol::Event::LeaveNotify(ln) => {
                if Some(ln.event) == self.menu_win {
                    self.menu_hover = -1;
                    self.draw_menu(conn, screen);
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}
