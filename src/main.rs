mod app;
mod deezer;
mod http_image_client;
mod protocol;
mod scrub_bar;
mod settings;
mod smooth_scroll;
mod text_input;
mod theme;
mod time_util;
mod ui;
mod webview;
mod win_icon;

use std::sync::Arc;

use gpui::{
    App, Application, Bounds, Focusable, TitlebarOptions, WindowBounds, WindowOptions, prelude::*,
    px, size,
};

use crate::app::{YtmTidalApp, register_app_keybindings};
use crate::http_image_client::ReqwestImageClient;
use crate::protocol::WebCommand;
use crate::text_input::register_search_keybindings;
use crate::webview::spawn_ytm_webview;

fn main() {
    let (controller, event_rx) =
        spawn_ytm_webview().expect("failed to initialize underlying YouTube Music WebView");
    let shutdown_ctrl = controller.clone();

    Application::new().run(move |cx: &mut App| {
        // Register real HTTP client so GPUI `img("https://...")` loads YouTube Music album artwork
        if let Ok(http_client) = ReqwestImageClient::new() {
            cx.set_http_client(Arc::new(http_client));
        }

        register_app_keybindings(cx);
        register_search_keybindings(cx);

        let bounds = Bounds::centered(None, size(px(1380.0), px(860.0)), cx);
        let window_title = "Riptide";
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(window_title.into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            focus: true,
            show: true,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            ..Default::default()
        };

        cx.open_window(options, move |window, cx| {
            let app = cx.new(|cx| YtmTidalApp::new(controller, event_rx, cx));
            let focus_handle = app.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            app
        })
        .expect("failed to open main GPUI window");

        win_icon::apply_window_icon_by_title(window_title);

        cx.on_window_closed(move |cx| {
            if cx.windows().is_empty() {
                shutdown_ctrl.send(WebCommand::Shutdown);
                cx.quit();
            }
        })
        .detach();

        cx.activate(true);
    });
}
