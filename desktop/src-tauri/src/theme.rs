use tauri::{AppHandle, Manager as _, Runtime, Theme, window::Color};

use crate::tray::MAIN_WINDOW;

const DARK_PAGE: Color = Color(0x0a, 0x0c, 0x0e, 0xff);
const LIGHT_PAGE: Color = Color(0xf2, 0xf4, 0xf6, 0xff);

pub fn page(theme: Theme) -> Color {
  match theme {
    Theme::Light => LIGHT_PAGE,
    _ => DARK_PAGE,
  }
}

pub fn paint<R: Runtime>(app: &AppHandle<R>, theme: Theme) {
  let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
    return;
  };
  let _ = window.set_background_color(Some(page(theme)));
}

pub fn sync<R: Runtime>(app: &AppHandle<R>) {
  let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
    return;
  };
  let theme = window.theme().unwrap_or(Theme::Dark);
  let _ = window.set_background_color(Some(page(theme)));
}
