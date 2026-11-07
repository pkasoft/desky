use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

pub static LOCAL_DESKY_ID: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static RUNNING: AtomicBool = AtomicBool::new(true);

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

        // pkasoft.com background paper soft color #f5f7f5 (24-bit RGB)
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

        // Set Window Title
        let title = "Desky • pkasoft.com";
        conn.change_property8(
            PropMode::REPLACE,
            win_id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )?;

        // Map window to screen
        conn.map_window(win_id)?;
        conn.flush()?;

        info!("Native X11 pkasoft window created and mapped (Window ID: 0x{:x})", win_id);

        // Graphics Contexts for drawing
        let gc_card = conn.generate_id()?;
        conn.create_gc(gc_card, win_id, &CreateGCAux::new().foreground(0x00FFFFFFu32))?; // Card background

        let gc_green = conn.generate_id()?;
        conn.create_gc(gc_green, win_id, &CreateGCAux::new().foreground(0x002D6A4Fu32))?; // pkasoft emerald

        let gc_border = conn.generate_id()?;
        conn.create_gc(gc_border, win_id, &CreateGCAux::new().foreground(0x00E2E7E3u32))?; // Card border

        while RUNNING.load(Ordering::Relaxed) {
            let event = conn.wait_for_event()?;
            match event {
                x11rb::protocol::Event::Expose(_) => {
                    // Draw Card 1: THIS DESK
                    let c1 = Rectangle { x: 24, y: 74, width: 340, height: 265 };
                    conn.poly_fill_rectangle(win_id, gc_card, &[c1])?;
                    conn.poly_rectangle(win_id, gc_green, &[c1])?;

                    // Draw Card 2: REMOTE DESK
                    let c2 = Rectangle { x: 382, y: 74, width: 340, height: 265 };
                    conn.poly_fill_rectangle(win_id, gc_card, &[c2])?;
                    conn.poly_rectangle(win_id, gc_border, &[c2])?;

                    // Draw Bottom Card: TELEMETRY
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
