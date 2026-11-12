//! Dedicated Viewport Remote Stream Canvas Window & Tabbed Session Manager
//!
//! Subdivided into focused architectural components:
//! - `window.rs`: Native Win32 window procedure, message dispatch & lifecycle
//! - `tabs.rs`: Multi-session tabs, switching, closing & window detachment
//! - `renderer.rs`: Double-buffered 60-120 FPS video frame canvas renderer
//! - `session_status_view.rs`: Dedicated session connecting, waiting & error/alert card renderer
//! - `chat_hud.rs`: Real-time in-session encrypted text messaging panel
//! - `toolbar.rs`: Compact floating top toolbar pill & Lucide vector icons
//! - `whiteboard.rs`: Interactive screen pen & real-time GDI+ polyline annotations
//! - `quick_actions.rs`: Instant system hotkeys & admin shortcuts dropdown
//! - `settings_modal.rs`: Fullscreen display, FPS & visual quality configuration
//! - `file_hud.rs`: In-canvas drag-and-drop file transfer progress HUD
//! - `input_injector.rs`: High-precision coordinate translation & Win32 input injection

pub mod chat_hud;
pub mod file_hud;
pub mod input_injector;
pub mod quick_actions;
pub mod renderer;
pub mod session_status_view;
pub mod settings_modal;
pub mod tabs;
pub mod toolbar;
pub mod whiteboard;
pub mod window;

pub use crate::windows::ui::theme;
#[allow(unused_imports)]
pub use chat_hud::*;
#[allow(unused_imports)]
pub use file_hud::*;
#[allow(unused_imports)]
pub use input_injector::*;
#[allow(unused_imports)]
pub use quick_actions::*;
pub use renderer::*;
#[allow(unused_imports)]
pub use session_status_view::*;
#[allow(unused_imports)]
pub use settings_modal::*;
pub use tabs::*;
#[allow(unused_imports)]
pub use toolbar::*;
#[allow(unused_imports)]
pub use whiteboard::*;
pub use window::*;

pub use window::viewport_wnd_proc as viewport_wndproc;
