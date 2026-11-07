use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static RUNNING: AtomicBool = AtomicBool::new(true);
static POPUP_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_assigned_desky_id(id: &str) {
    if let Ok(mut guard) = LOCAL_DESKY_ID.lock() {
        *guard = id.to_string();
    }
    info!("✅ [LINUX HOST] Permanent Desky ID Approved: {}", id);
    println!("\n\x1b[1;32m==================================================\x1b[0m");
    println!("\x1b[1;32m  ⚡ DESKY LINUX ONLINE • pkasoft.com\x1b[0m");
    println!("\x1b[1;37m  YOUR PERMANENT DESKY ID: \x1b[1;32m{}\x1b[0m", id);
    println!("\x1b[1;37m  STATUS: \x1b[1;36m● Ready for incoming connections\x1b[0m");
    println!("\x1b[1;32m==================================================\x1b[0m\n");
}

/// Spawns a native X11 approval popup dialog when a remote client requests to connect
pub fn spawn_linux_approval_popup(client_id: String) {
    if POPUP_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        println!("\n\x1b[1;33m==================================================\x1b[0m");
        println!("\x1b[1;33m  🔔 INCOMING DESKY CONNECTION REQUEST!\x1b[0m");
        println!("\x1b[1;37m  Remote Client ID: \x1b[1;32m{}\x1b[0m", client_id);
        println!("\x1b[1;37m  Click [ACCEPT] or [DECLINE] in the popup window!\x1b[0m");
        println!("\x1b[1;33m==================================================\x1b[0m\n");

        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            let screen = &conn.setup().roots[screen_num];
            let Ok(win_id) = conn.generate_id() else {
                POPUP_ACTIVE.store(false, Ordering::SeqCst);
                return;
            };
            let win_w = 440u16;
            let win_h = 210u16;
            let pos_x = ((screen.width_in_pixels.saturating_sub(win_w)) / 2) as i16;
            let pos_y = ((screen.height_in_pixels.saturating_sub(win_h)) / 2) as i16;

            let win_aux = CreateWindowAux::new()
                .background_pixel(0x00F5F7F5)
                .override_redirect(1)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::BUTTON_PRESS
                        | EventMask::KEY_PRESS
                        | EventMask::STRUCTURE_NOTIFY,
                );

            let _ = conn.create_window(
                screen.root_depth,
                win_id,
                screen.root,
                pos_x,
                pos_y,
                win_w,
                win_h,
                2,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &win_aux,
            );

            let title = "Desky Connection Request";
            let _ = conn.change_property8(
                PropMode::REPLACE,
                win_id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            );

            let _ = conn.map_window(win_id);
            let _ = conn.configure_window(
                win_id,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            );
            let _ = conn.flush();

            let gc_card = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(
                gc_card,
                win_id,
                &CreateGCAux::new().foreground(0x00FFFFFF).background(0x00FFFFFF),
            );

            let gc_green = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(
                gc_green,
                win_id,
                &CreateGCAux::new().foreground(0x002D6A4F).background(0x002D6A4F),
            );

            let gc_red = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(
                gc_red,
                win_id,
                &CreateGCAux::new().foreground(0x00C53030).background(0x00C53030),
            );

            let gc_text_dark = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(
                gc_text_dark,
                win_id,
                &CreateGCAux::new().foreground(0x001D2420).background(0x00FFFFFF),
            );

            let gc_text_accept = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(
                gc_text_accept,
                win_id,
                &CreateGCAux::new().foreground(0x00FFFFFF).background(0x002D6A4F),
            );

            let gc_text_decline = conn.generate_id().unwrap_or(0);
            let _ = conn.create_gc(
                gc_text_decline,
                win_id,
                &CreateGCAux::new().foreground(0x00FFFFFF).background(0x00C53030),
            );

            while crate::p2p::get_pending_approval().is_some() {
                if let Ok(Some(ev)) = conn.poll_for_event() {
                    match ev {
                        x11rb::protocol::Event::Expose(_) => {
                            let card = Rectangle { x: 12, y: 12, width: 416, height: 186 };
                            let _ = conn.poly_fill_rectangle(win_id, gc_card, &[card]);
                            let _ = conn.poly_rectangle(win_id, gc_green, &[card]);

                            let line1 = "INCOMING DESKY SESSION REQUEST";
                            let line2 = format!("Remote Client ID: {} wants to connect.", client_id);
                            let line3 = "Allow this peer to view and control your desktop?";
                            let _ = conn.image_text8(win_id, gc_text_dark, 30, 46, line1.as_bytes());
                            let _ = conn.image_text8(win_id, gc_text_dark, 30, 78, line2.as_bytes());
                            let _ = conn.image_text8(win_id, gc_text_dark, 30, 104, line3.as_bytes());

                            // [ ACCEPT ] button (x: 30..210, y: 132..178)
                            let btn_acc = Rectangle { x: 30, y: 132, width: 180, height: 46 };
                            let _ = conn.poly_fill_rectangle(win_id, gc_green, &[btn_acc]);
                            let _ = conn.image_text8(
                                win_id,
                                gc_text_accept,
                                68,
                                160,
                                b"ACCEPT (Click / Y)",
                            );

                            // [ DECLINE ] button (x: 230..410, y: 132..178)
                            let btn_dec = Rectangle { x: 230, y: 132, width: 180, height: 46 };
                            let _ = conn.poly_fill_rectangle(win_id, gc_red, &[btn_dec]);
                            let _ = conn.image_text8(
                                win_id,
                                gc_text_decline,
                                266,
                                160,
                                b"DECLINE (Click / N)",
                            );

                            let _ = conn.flush();
                        }
                        x11rb::protocol::Event::ButtonPress(bp) => {
                            let bx = bp.event_x;
                            let by = bp.event_y;
                            if by >= 132 && by <= 178 {
                                if bx >= 30 && bx <= 210 {
                                    crate::p2p::respond_to_pending_approval(true);
                                    break;
                                } else if bx >= 230 && bx <= 410 {
                                    crate::p2p::respond_to_pending_approval(false);
                                    break;
                                }
                            }
                        }
                        x11rb::protocol::Event::KeyPress(kp) => {
                            // 36 = Return, 29 = Y, 57 = N, 9 = Escape
                            if kp.detail == 36 || kp.detail == 29 {
                                crate::p2p::respond_to_pending_approval(true);
                                break;
                            } else if kp.detail == 57 || kp.detail == 9 {
                                crate::p2p::respond_to_pending_approval(false);
                                break;
                            }
                        }
                        _ => {}
                    }
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
            }

            let _ = conn.unmap_window(win_id);
            let _ = conn.destroy_window(win_id);
            let _ = conn.flush();
        }
        POPUP_ACTIVE.store(false, Ordering::SeqCst);
    });
}

pub struct LinuxApp {
    has_x11: bool,
}

impl LinuxApp {
    pub fn new() -> Result<Self> {
        let has_x11 = std::env::var("DISPLAY").is_ok();
        Ok(Self { has_x11 })
    }

    pub fn run(&self) {
        let current_id = {
            let guard = LOCAL_DESKY_ID.lock().unwrap();
            if !guard.is_empty() {
                guard.clone()
            } else {
                crate::identity::load_or_create_identity()
                    .desky_id
                    .unwrap_or_else(|| "Connecting...".into())
            }
        };

        println!("\x1b[1;32m==================================================\x1b[0m");
        println!("\x1b[1;32m  ⚡ DESKY REMOTE DESKTOP (Linux / Lubuntu)\x1b[0m");
        println!("\x1b[1;37m  Permanent Desky ID: \x1b[1;32m{}\x1b[0m", current_id);
        println!("\x1b[1;37m  Theme: \x1b[0;32mpkasoft.com White & Emerald\x1b[0m");
        println!("\x1b[1;37m  Input Injection: \x1b[0;36mPure Rust X11 / XTest Active\x1b[0m");
        println!("\x1b[1;37m  Display Server: \x1b[0;33m{}\x1b[0m", if self.has_x11 { "X11 Display Detected" } else { "Headless / Terminal" });
        println!("\x1b[1;32m==================================================\x1b[0m\n");

        if self.has_x11 {
            if let Err(e) = self.run_x11_window() {
                info!("X11 Window loop error ({:?}), falling back to terminal loop", e);
                self.run_terminal_loop();
            }
        } else {
            self.run_terminal_loop();
        }
    }

    /// Spawns a clean native X11 window adhering to pkasoft.com White Theme
    fn run_x11_window(&self) -> Result<()> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];

        let win_id = conn.generate_id()?;
        let win_w = 760u16;
        let win_h = 580u16;

        let bg_color = 0x00F5F7F5u32;

        let win_aux = CreateWindowAux::new()
            .background_pixel(bg_color)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::KEY_PRESS
                    | EventMask::BUTTON_PRESS
                    | EventMask::STRUCTURE_NOTIFY,
            );

        conn.create_window(
            screen.root_depth,
            win_id,
            screen.root,
            100,
            100,
            win_w,
            win_h,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &win_aux,
        )?;

        let title = "Desky • pkasoft.com";
        conn.change_property8(
            PropMode::REPLACE,
            win_id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )?;

        conn.map_window(win_id)?;
        conn.flush()?;

        info!("Native X11 pkasoft window created and mapped (Window ID: 0x{:x})", win_id);

        let gc_card = conn.generate_id()?;
        conn.create_gc(gc_card, win_id, &CreateGCAux::new().foreground(0x00FFFFFFu32))?;

        let gc_green = conn.generate_id()?;
        conn.create_gc(gc_green, win_id, &CreateGCAux::new().foreground(0x002D6A4Fu32))?;

        let gc_border = conn.generate_id()?;
        conn.create_gc(gc_border, win_id, &CreateGCAux::new().foreground(0x00E2E7E3u32))?;

        while RUNNING.load(Ordering::Relaxed) {
            let event = conn.wait_for_event()?;
            match event {
                x11rb::protocol::Event::Expose(_) => {
                    let c1 = Rectangle { x: 24, y: 74, width: 340, height: 265 };
                    conn.poly_fill_rectangle(win_id, gc_card, &[c1])?;
                    conn.poly_rectangle(win_id, gc_green, &[c1])?;

                    let c2 = Rectangle { x: 382, y: 74, width: 340, height: 265 };
                    conn.poly_fill_rectangle(win_id, gc_card, &[c2])?;
                    conn.poly_rectangle(win_id, gc_border, &[c2])?;

                    let c3 = Rectangle { x: 24, y: 355, width: 698, height: 200 };
                    conn.poly_fill_rectangle(win_id, gc_card, &[c3])?;
                    conn.poly_rectangle(win_id, gc_border, &[c3])?;

                    conn.flush()?;
                }
                x11rb::protocol::Event::DestroyNotify(_) => {
                    info!("X11 Window destroyed, shutting down.");
                    break;
                }
                x11rb::protocol::Event::ClientMessage(msg) => {
                    info!("X11 Client message received: {:?}", msg);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn run_terminal_loop(&self) {
        info!("Running Desky Linux host daemon. Press Ctrl+C to exit.");
        while RUNNING.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
