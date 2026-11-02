#![windows_subsystem = "windows"]
mod app;
mod clipboard_smoke;
mod platform;
mod tray;
mod ui;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let diagnostic = arguments
        .iter()
        .any(|a| matches!(a.as_str(), "--smoke-test" | "--clipboard-self-test"));
    let result = (|| -> Result<(), String> {
        // This child diagnostic must switch window stations before COM or any
        // UI creates a window, so it cannot access the user's clipboard.
        if let Some(index) = arguments.iter().position(|a| a == "--clipboard-self-test") {
            return clipboard_smoke::run(std::path::Path::new(
                arguments
                    .get(index + 1)
                    .ok_or("--clipboard-self-test 后需要空输出目录。")?,
            ));
        }
        let _com = platform::ComApartment::new()?;
        if let Some(index) = arguments.iter().position(|a| a == "--smoke-test") {
            return app::smoke::run(std::path::Path::new(
                arguments
                    .get(index + 1)
                    .ok_or("--smoke-test 后需要空输出目录。")?,
            ));
        }
        let mut settings_directory = None;
        if let Some(index) = arguments.iter().position(|a| a == "--profile") {
            settings_directory = Some(std::path::PathBuf::from(
                arguments
                    .get(index + 1)
                    .ok_or("--profile 后需要配置目录。")?,
            ));
        }
        app::App::run(app::Options {
            show_settings: arguments.iter().any(|a| a == "--settings"),
            startup: arguments.iter().any(|a| a == "--startup"),
            settings_directory,
            smoke: false,
        })
    })();
    if let Err(error) = result {
        if !diagnostic {
            platform::log_error(&error);
            ui::common::error(windows::Win32::Foundation::HWND::default(), &error);
        }
        std::process::exit(1);
    }
}
