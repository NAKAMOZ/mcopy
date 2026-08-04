//! A one-message window.
//!
//! mcopy's paste command is almost always launched from a file manager, which
//! attaches no console — and on Windows the binary is linked with
//! `windows_subsystem = "windows"`, so there is not even a stream to write to.
//! Version 0.2 responded to "nothing to paste" and to unreadable sources by
//! returning `Ok(())`, which the user experienced as the menu item doing
//! literally nothing.
//!
//! This window is the smallest thing that turns those silent exits into an
//! explanation.

use crate::ui::assets::register_fonts;
use crate::ui::shutdown::{self, quit_when_last_window_closes};
use crate::ui::theme::Palette;
use crate::ui::widgets::logo_mark;
use gpui::*;

const NOTICE_WIDTH: f32 = 380.0;
const NOTICE_HEIGHT: f32 = 150.0;
const SIDE_PADDING: f32 = 20.0;

struct NoticeWindow {
    title: SharedString,
    message: SharedString,
    appearance_observer: Option<Subscription>,
}

impl NoticeWindow {
    fn ensure_appearance_observer(&mut self, window: &mut Window) {
        if self.appearance_observer.is_some() {
            return;
        }
        self.appearance_observer = Some(
            window.observe_window_appearance(|window, _| window.refresh()),
        );
    }
}

impl Render for NoticeWindow {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_appearance_observer(window);
        let palette = Palette::for_appearance(window.appearance());

        let dismiss = cx.listener(|_, _: &ClickEvent, window, _| {
            shutdown::close(window);
        });

        div()
            .relative()
            .w(px(NOTICE_WIDTH))
            .h(px(NOTICE_HEIGHT))
            .bg(rgb(palette.card_bg))
            .rounded(px(12.))
            .font_family("Inter")
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .w_full()
                    .h(px(NOTICE_HEIGHT - 52.))
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(SIDE_PADDING))
                    .child(logo_mark(18., 27., palette.logo_ink)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING + 30.))
                    .top(px(SIDE_PADDING + 4.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(palette.title_text))
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(64.))
                    .w(px(NOTICE_WIDTH - SIDE_PADDING * 2.))
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(rgb(palette.muted_text))
                    .child(self.message.clone()),
            )
            .child(
                div()
                    .id("dismiss-notice")
                    .absolute()
                    .right(px(SIDE_PADDING))
                    .bottom(px(SIDE_PADDING))
                    .w(px(84.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .bg(rgb(palette.neutral_fill))
                    .text_color(rgb(palette.on_fill_text))
                    .hover(|this| this.bg(rgb(palette.neutral_hover)))
                    .active(|this| this.bg(rgb(palette.neutral_active)))
                    .cursor_pointer()
                    .on_click(dismiss)
                    .child("OK"),
            )
    }
}

pub(crate) fn notice_window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("mcopy".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_resizable: false,
        is_minimizable: true,
        app_id: Some(crate::APP_ID.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    }
}

/// Show `message` and block until the user dismisses it.
pub fn show_notice_window(title: &str, message: &str) {
    let title: SharedString = title.to_string().into();
    let message: SharedString = message.to_string().into();

    Application::new().run(move |cx| {
        register_fonts(cx);
        quit_when_last_window_closes(cx);

        let bounds = Bounds::centered(
            None,
            size(px(NOTICE_WIDTH), px(NOTICE_HEIGHT)),
            cx,
        );

        let opened = cx.open_window(notice_window_options(bounds), {
            let title = title.clone();
            let message = message.clone();
            move |_, cx| {
                cx.new(move |_| NoticeWindow {
                    title: title.clone(),
                    message: message.clone(),
                    appearance_observer: None,
                })
            }
        });

        if opened.is_err() {
            // Nothing more we can do; the caller already wrote to stderr.
            cx.quit();
            return;
        }

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    // `use gpui::*` above pulls in gpui's own `test` attribute macro, which
    // would shadow the built-in one and expand recursively.
    use core::prelude::v1::test;

    #[test]
    fn the_notice_is_reachable_from_the_taskbar() {
        let options = notice_window_options(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(NOTICE_WIDTH), px(NOTICE_HEIGHT)),
        });

        assert_eq!(options.kind, WindowKind::Normal);
        assert!(options.is_minimizable);
    }
}
