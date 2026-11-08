//! Lucide Monitor Icon
//! Standard 24x24 vector: display screen, stand, and base.

use super::{draw_line, draw_polyline, draw_rect, IconContext, PenScope};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;

pub unsafe fn draw_monitor(hdc: HDC, x: i32, y: i32, size: i32, color: COLORREF, stroke_width: i32) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Screen border: rect (2, 3) to (22, 17) r=2
    draw_rect(hdc, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);
    // Vertical stand line: (12, 17) -> (12, 21)
    draw_line(hdc, &ctx, 12.0, 17.0, 12.0, 21.0);
    // Horizontal base line: (8, 21) -> (16, 21)
    draw_line(hdc, &ctx, 8.0, 21.0, 16.0, 21.0);
}

pub unsafe fn draw_monitor_num(
    hdc: HDC,
    x: i32,
    y: i32,
    size: i32,
    num: u8,
    color: COLORREF,
    stroke_width: i32,
) {
    let ctx = IconContext::new(x, y, size);
    let _pen = PenScope::new(hdc, color, stroke_width);

    // Screen border: rect (2, 3) to (22, 17) r=2
    draw_rect(hdc, &ctx, 2.0, 3.0, 20.0, 14.0, 2.0);
    // Vertical stand line: (12, 17) -> (12, 21)
    draw_line(hdc, &ctx, 12.0, 17.0, 12.0, 21.0);
    // Horizontal base line: (8, 21) -> (16, 21)
    draw_line(hdc, &ctx, 8.0, 21.0, 16.0, 21.0);

    // Vector numeral inside screen
    match num {
        1 => {
            draw_line(hdc, &ctx, 10.0, 8.0, 12.0, 6.0);
            draw_line(hdc, &ctx, 12.0, 6.0, 12.0, 14.0);
            draw_line(hdc, &ctx, 10.0, 14.0, 14.0, 14.0);
        }
        2 => {
            draw_polyline(
                hdc,
                &ctx,
                &[
                    (9.5, 8.0),
                    (11.0, 6.5),
                    (13.0, 6.5),
                    (14.5, 8.0),
                    (14.5, 9.5),
                    (9.5, 14.0),
                    (14.5, 14.0),
                ],
            );
        }
        3 => {
            draw_polyline(
                hdc,
                &ctx,
                &[
                    (9.5, 6.5),
                    (14.5, 6.5),
                    (12.0, 10.0),
                    (14.5, 11.5),
                    (14.5, 13.0),
                    (13.0, 14.0),
                    (9.5, 14.0),
                ],
            );
        }
        _ => {}
    }
}
