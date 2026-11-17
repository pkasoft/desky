//! Dedicated Viewport Remote Stream Canvas Window & Tabbed Session Manager (Linux X11)
//!
//! Subdivided into focused architectural components matching `src/windows/ui/viewport/`:
//! - `window.rs`: Native X11 window creation, double-buffered event dispatch & lifecycle
//! - `tabs.rs`: Multi-session tabs, switching & closing
//! - `renderer.rs`: Double-buffered video frame canvas & homepage compositing
//! - `session_status_view.rs`: Session connecting, waiting, password & error card renderer
//! - `chat_hud.rs`: Real-time in-session encrypted text messaging panel
//! - `toolbar.rs`: Top tab bar, vector icons & session tools
//! - `whiteboard.rs`: Interactive screen pen & polyline annotations
//! - `quick_actions.rs`: Instant system hotkeys & admin shortcuts dropdown
//! - `overflow_menu.rs`: Secondary tools overflow menu
//! - `settings_modal.rs`: Fullscreen display, FPS & visual quality configuration
//! - `file_hud.rs`: In-canvas file transfer progress HUD
//! - `input_injector.rs`: Coordinate translation & remote mouse/keyboard injection

pub mod chat_hud;
pub mod file_hud;
pub mod input_injector;
pub mod overflow_menu;
pub mod quick_actions;
pub mod renderer;
pub mod session_status_view;
pub mod settings_modal;
pub mod tabs;
pub mod toolbar;
pub mod whiteboard;
pub mod window;

#[allow(unused_imports)]
pub use chat_hud::*;
#[allow(unused_imports)]
pub use file_hud::*;
#[allow(unused_imports)]
pub use input_injector::*;
#[allow(unused_imports)]
pub use overflow_menu::*;
#[allow(unused_imports)]
pub use quick_actions::*;
#[allow(unused_imports)]
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
