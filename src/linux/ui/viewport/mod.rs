//! Dedicated Viewport Remote Stream Canvas Window & Tabbed Session Manager (Linux X11)
//!
//! Subdivided into focused architectural components matching `src/windows/ui/viewport/`:
//! - `window.rs`: Native X11 window creation, double-buffered event dispatch & lifecycle
//! - `tabs.rs`: Multi-session tabs, switching & closing
//! - `renderer.rs`: Double-buffered video frame canvas & homepage compositing
//! - `session_status_view.rs`: Session connecting, waiting, password & error card renderer
//! - `toolbar.rs`: Top tab bar, relay ping badge & host session controls
//! - `input_injector.rs`: Coordinate translation & remote mouse/keyboard injection

pub mod input_injector;
pub mod renderer;
pub mod session_status_view;
pub mod tabs;
pub mod toolbar;
pub mod window;

#[allow(unused_imports)]
pub use input_injector::*;
#[allow(unused_imports)]
pub use renderer::*;
#[allow(unused_imports)]
pub use session_status_view::*;
pub use tabs::*;
#[allow(unused_imports)]
pub use toolbar::*;
pub use window::*;
