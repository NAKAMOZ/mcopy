use super::theme::{ACTION_BUTTON_WIDTH, ButtonTone, LOGO_ACCENT, Palette};
use gpui::*;

pub fn surface_card(palette: &Palette) -> Div {
    div().bg(rgb(palette.card_bg)).rounded_xl()
}

pub fn brand_mark(palette: &Palette) -> Div {
    logo_mark(18., 27., palette.logo_ink)
}

/// The mcopy mark: four ink bars plus one accent bar.
///
/// `ink` is themed (black in light mode, white in dark) while the accent bar is
/// always [`LOGO_ACCENT`]. Because each bar is its own filled rectangle, the
/// theme switch recolors only the bars it is given and can never shift the
/// green — unlike a whole-element filter, which would.
pub fn logo_mark(width: f32, height: f32, ink: u32) -> Div {
    // The source artwork (logo.svg) is authored on a 200x300 grid; scale the
    // bar geometry from that grid to the requested size.
    let sx = width / 200.;
    let sy = height / 300.;
    let radius = (width * 0.06).max(1.);

    div()
        .relative()
        .w(px(width))
        .h(px(height))
        .flex_none()
        .overflow_hidden()
        .child(logo_bar(0., 50., 25., 200., sx, sy, radius, ink))
        .child(logo_bar(34., 25., 23., 250., sx, sy, radius, ink))
        .child(logo_bar(66., 0., 68., 300., sx, sy, radius, ink))
        .child(logo_bar(100., 25., 66., 250., sx, sy, radius, ink))
        .child(logo_bar(134., 50., 66., 200., sx, sy, radius, LOGO_ACCENT))
}

#[allow(clippy::too_many_arguments)]
fn logo_bar(
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    sx: f32,
    sy: f32,
    radius: f32,
    color: u32,
) -> Div {
    div()
        .absolute()
        .left(px(left * sx))
        .top(px(top * sy))
        .w(px(width * sx))
        .h(px(height * sy))
        .rounded(px(radius))
        .bg(rgb(color))
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

pub fn header_row(
    status: impl IntoElement,
    counter: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(status)
        .child(counter)
}

pub fn progress_bar(percent: f32, fill_color: u32, palette: &Palette) -> Div {
    let ratio = (percent / 100.0).clamp(0.0, 1.0);

    div()
        .w_full()
        .h(px(4.))
        .rounded_full()
        .bg(rgb(palette.progress_track))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(ratio))
                .bg(rgb(fill_color))
                .rounded_full(),
        )
}

pub fn file_name_row(file_display: String, palette: &Palette) -> Div {
    div()
        .w_full()
        .truncate()
        .text_sm()
        .text_color(rgb(palette.muted_text))
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

/// A single-line banner under the progress bar.
///
/// `tone` lets callers surface an actionable failure reason (see
/// [`crate::copy::CopyErrorKind`]) in the error color rather than the muted one,
/// so "permission denied" reads as a problem instead of a footnote.
pub fn message_banner(
    message: String,
    is_error: bool,
    palette: &Palette,
) -> Div {
    let color = if is_error {
        palette.error_text
    } else {
        palette.soft_text
    };

    div()
        .w_full()
        .h(px(16.))
        .text_xs()
        .truncate()
        .text_color(rgb(color))
        .child(message)
}

pub fn action_button(
    id: &'static str,
    label: &'static str,
    tone: ButtonTone,
    disabled: bool,
    palette: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let weight = if matches!(tone, ButtonTone::Outline) {
        FontWeight::MEDIUM
    } else {
        FontWeight::BOLD
    };

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
        .font_weight(weight)
        .child(label.to_string());

    if disabled {
        base.bg(rgb(palette.disabled_bg))
            .border_color(rgb(palette.disabled_border))
            .text_color(rgb(palette.disabled_text))
            .cursor_default()
    } else {
        let (hover_bg, active_bg, border) = (
            tone.hover_background(palette),
            tone.active_background(palette),
            tone.border(palette),
        );

        base.bg(rgb(tone.background(palette)))
            .border_color(rgb(border))
            .text_color(rgb(tone.text(palette)))
            .hover(move |this| this.bg(rgb(hover_bg)).border_color(rgb(border)))
            .active(move |this| {
                this.bg(rgb(active_bg)).border_color(rgb(border))
            })
            .cursor_pointer()
            .on_click(on_click)
    }
}
