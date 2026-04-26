use super::constants::{
    ACTION_BUTTON_WIDTH, ButtonTone, CARD_BG, DISABLED_BG, DISABLED_BORDER, DISABLED_TEXT,
    MUTED_TEXT, PROGRESS_TRACK, SOFT_TEXT,
};
use gpui::*;

pub fn surface_card() -> Div {
    div().bg(rgb(CARD_BG)).rounded_xl()
}

pub fn brand_mark() -> Div {
    div()
        .w(px(18.))
        .h(px(27.))
        .flex_none()
        .child(img("logo.svg").w_full().h_full())
}

pub fn drag_region(content: impl IntoElement) -> impl IntoElement {
    div()
        .w_full()
        .window_control_area(WindowControlArea::Drag)
        .child(content)
}

pub fn status_text(label: String, color: u32) -> Div {
    div()
        .text_lg()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(color))
        .child(label)
}

pub fn counter_display(
    processed: usize,
    total: usize,
    processed_color: u32,
    secondary_color: u32,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_sm()
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(processed_color))
                .child(processed.to_string()),
        )
        .child(div().text_color(rgb(secondary_color)).child("/"))
        .child(
            div()
                .text_color(rgb(secondary_color))
                .child(total.to_string()),
        )
}

pub fn header_row(status: impl IntoElement, counter: impl IntoElement) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(status)
        .child(counter)
}

pub fn progress_bar(percent: f32, fill_color: u32) -> Div {
    let ratio = (percent / 100.0).clamp(0.0, 1.0);

    div()
        .w_full()
        .h(px(4.))
        .rounded_full()
        .bg(rgb(PROGRESS_TRACK))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(ratio))
                .bg(rgb(fill_color))
                .rounded_full(),
        )
}

pub fn file_name_row(file_display: String) -> Div {
    div()
        .w_full()
        .truncate()
        .text_sm()
        .text_color(rgb(MUTED_TEXT))
        .child(file_display)
}

pub fn controls_row(
    cancel_button: impl IntoElement,
    primary_button: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .justify_end()
        .gap_2()
        .child(cancel_button)
        .child(primary_button)
}

pub fn message_banner(message: String) -> Div {
    div()
        .w_full()
        .h(px(16.))
        .text_xs()
        .text_color(rgb(SOFT_TEXT))
        .child(message)
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
        .w(px(ACTION_BUTTON_WIDTH))
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .border_1()
        .font_family("Inter")
        .text_sm()
        .child(label.to_string());

    if disabled {
        base.bg(rgb(DISABLED_BG))
            .border_color(rgb(DISABLED_BORDER))
            .text_color(rgb(DISABLED_TEXT))
            .font_weight(if matches!(tone, ButtonTone::Outline) {
                FontWeight::MEDIUM
            } else {
                FontWeight::BOLD
            })
            .cursor_default()
    } else {
        base.bg(rgb(tone.background()))
            .border_color(rgb(tone.border()))
            .text_color(rgb(tone.text()))
            .font_weight(if matches!(tone, ButtonTone::Outline) {
                FontWeight::MEDIUM
            } else {
                FontWeight::BOLD
            })
            .hover(move |this| {
                this.bg(rgb(tone.hover_background()))
                    .border_color(rgb(tone.border()))
            })
            .active(move |this| {
                this.bg(rgb(tone.active_background()))
                    .border_color(rgb(tone.border()))
            })
            .cursor_pointer()
            .on_click(on_click)
    }
}
