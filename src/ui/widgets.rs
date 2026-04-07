use super::constants::{
    BODY_TEXT, ButtonTone, CARD_BG, CARD_BORDER, DISABLED_BG, DISABLED_BORDER, DISABLED_TEXT,
    FILE_CARD_BG, LABEL_TEXT, MUTED_TEXT, TITLE_TEXT,
};
use gpui::*;

pub fn header_row(badge: Div, title: String, subtitle: String, percent_text: String) -> Div {
    div()
        .flex()
        .justify_between()
        .items_start()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(badge)
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(TITLE_TEXT))
                        .child(title),
                )
                .child(div().text_sm().text_color(rgb(MUTED_TEXT)).child(subtitle)),
        )
        .child(
            div()
                .text_right()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(LABEL_TEXT))
                        .child("Genel ilerleme"),
                )
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(BODY_TEXT))
                        .child(percent_text),
                ),
        )
}

pub fn panel_card() -> Div {
    div()
        .w_full()
        .rounded_lg()
        .bg(rgb(CARD_BG))
        .border_1()
        .border_color(rgb(CARD_BORDER))
}

pub fn progress_bar(percent: f32, accent: u32) -> Div {
    let ratio = (percent / 100.0).clamp(0.0, 1.0);

    panel_card().child(
        div()
            .w_full()
            .h(px(14.))
            .rounded_full()
            .bg(rgba(0x12243caa))
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(ratio))
                    .bg(rgb(accent))
                    .rounded_full(),
            ),
    )
}

pub fn metric_chip(label: &str, value: String, accent: u32) -> Div {
    panel_card().child(
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .px_3()
            .py_2()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(LABEL_TEXT))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(accent))
                    .child(value),
            ),
    )
}

pub fn controls_row(
    pause_button: impl IntoElement,
    stop_button: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .gap_3()
        .child(div().flex_1().child(pause_button))
        .child(div().flex_1().child(stop_button))
}

pub fn file_info_card(file_display: String, current_file_bytes: u64) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(rgb(FILE_CARD_BG))
        .border_1()
        .border_color(rgb(CARD_BORDER))
        .child(
            div()
                .flex()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(LABEL_TEXT))
                        .child("Guncel dosya"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(LABEL_TEXT))
                        .child(format_bytes(current_file_bytes)),
                ),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(BODY_TEXT))
                .child(file_display),
        )
}

pub fn message_banner(message: String) -> Div {
    div()
        .w_full()
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(rgb(0x2b1320))
        .border_1()
        .border_color(rgb(0x713246))
        .text_xs()
        .text_color(rgb(0xfda4af))
        .child(message)
}

pub fn status_badge(label: String, accent: u32) -> Div {
    div()
        .px_3()
        .py_1()
        .rounded_full()
        .bg(rgba((accent << 8) | 0x29))
        .border_1()
        .border_color(rgba((accent << 8) | 0x66))
        .text_xs()
        .text_color(rgb(accent))
        .child(label)
}

pub fn action_button(
    id: &'static str,
    label: &'static str,
    tone: ButtonTone,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let base = div()
        .id(id)
        .w_full()
        .h(px(44.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .border_1()
        .text_sm()
        .font_weight(FontWeight::BOLD)
        .child(label.to_string());

    if disabled {
        base.bg(rgb(DISABLED_BG))
            .border_color(rgb(DISABLED_BORDER))
            .text_color(rgb(DISABLED_TEXT))
            .cursor_default()
    } else {
        base.bg(rgb(tone.background()))
            .border_color(rgb(tone.border()))
            .text_color(gpui::white())
            .hover(move |this| this.bg(rgb(tone.hover_background())))
            .active(move |this| this.bg(rgb(tone.active_background())))
            .cursor_pointer()
            .on_click(on_click)
    }
}

pub fn truncate_middle(input: &str, max_chars: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max_chars {
        return input.to_string();
    }

    let left = max_chars / 2;
    let right = max_chars.saturating_sub(left + 3);
    let start: String = chars.iter().take(left).collect();
    let end: String = chars[chars.len().saturating_sub(right)..].iter().collect();

    format!("{}...{}", start, end)
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "boyut bilinmiyor".to_string();
    }

    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}
