//! `vadctl` console entry point.
//!
//! Shares the library with the Tauri GUI; no window is created.

fn main() {
    #[cfg(windows)]
    visual_automation_designer_lib::platform::uia::enable_dpi_awareness();

    let code = visual_automation_designer_lib::cli::run(std::env::args());
    std::process::exit(code);
}
