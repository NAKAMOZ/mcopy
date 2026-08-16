//! "A new version is available" — the only window a bare launch can open.
//!
//! Deliberately a prompt and not a progress report: mcopy downloads and
//! verifies the artifact, then hands it to the platform's own installer, which
//! has its own UI. This window exists to ask permission and to say what
//! happened, nothing more.

use crate::ui::assets::register_fonts;
use crate::ui::notice::notice_window_options;
use crate::ui::shutdown::{self, quit_when_last_window_closes};
use crate::ui::theme::{ButtonTone, Palette};
use crate::ui::widgets::{action_button, logo_mark};
use crate::update::{self, UpdateInfo, UpdateStyle};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::{Arc, Mutex};

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 170.0;
const SIDE_PADDING: f32 = 20.0;

/// Where the prompt is in its one-way sequence.
#[derive(Clone)]
enum Phase {
    /// Waiting on the user.
    Asking,
    /// Downloading and verifying. Buttons are inert.
    Working,
    /// Finished; the message explains what to do next.
    Done(SharedString),
    /// Failed; the message explains why.
    Failed(SharedString),
}

/// The phase lives behind a lock rather than in the view because the download
/// task writes it from outside the render loop, exactly as the progress window
/// shares its snapshot. The task then asks the window to repaint.
type SharedPhase = Arc<Mutex<Phase>>;

struct UpdatePrompt {
    info: Arc<UpdateInfo>,
    phase: SharedPhase,
    appearance_observer: Option<Subscription>,
}

impl UpdatePrompt {
    fn ensure_appearance_observer(&mut self, window: &mut Window) {
        if self.appearance_observer.is_some() {
            return;
        }
        self.appearance_observer = Some(
            window.observe_window_appearance(|window, _| window.refresh()),
        );
    }

    fn phase(&self) -> Phase {
        self.phase.lock().expect("phase lock poisoned").clone()
    }

    /// The line under the title.
    fn detail(&self, phase: &Phase) -> SharedString {
        match phase {
            Phase::Asking => match self.info.style {
                UpdateStyle::Automatic => {
                    "Download and install it now? mcopy verifies the download \
                     before running it."
                        .into()
                },
                UpdateStyle::Manual => {
                    "This install has to be updated by hand. Open the releases \
                     page to download it."
                        .into()
                },
            },
            Phase::Working => "Downloading and verifying…".into(),
            Phase::Done(message) | Phase::Failed(message) => message.clone(),
        }
    }

    /// Start the download. Runs on the window's own executor, so closing the
    /// window drops the future and cancels the transfer.
    fn begin_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        *self.phase.lock().expect("phase lock poisoned") = Phase::Working;
        cx.notify();

        let info = self.info.clone();
        let phase = self.phase.clone();

        window
            .spawn(cx, async move |cx| {
                let result = update::download_and_install(&info).await;

                let should_exit = match result {
                    Ok(outcome) => {
                        *phase.lock().expect("phase lock poisoned") =
                            Phase::Done(outcome.message.into());
                        outcome.should_exit
                    },
                    Err(error) => {
                        *phase.lock().expect("phase lock poisoned") =
                            Phase::Failed(error.to_string().into());
                        false
                    },
                };

                _ = cx.update(|window, _| {
                    if should_exit {
                        // The external installer has taken over and needs our
                        // files; staying open would only get in its way.
                        shutdown::close(window);
                    } else {
                        window.refresh();
                    }
                });
            })
            .detach();
    }
}

impl Render for UpdatePrompt {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_appearance_observer(window);
        let palette = Palette::for_appearance(window.appearance());

        let title: SharedString =
            format!("mcopy {} is available", self.info.version).into();
        let subtitle: SharedString =
            format!("You have {}.", self.info.current).into();
        let phase = self.phase();
        let detail = self.detail(&phase);

        let working = matches!(phase, Phase::Working);
        let finished = matches!(phase, Phase::Done(_) | Phase::Failed(_));
        let manual = self.info.style == UpdateStyle::Manual;

        let primary_label = if manual { "Releases" } else { "Update" };
        let dismiss_label = if finished { "Close" } else { "Not now" };

        let primary = cx.listener(
            move |prompt: &mut Self, _: &ClickEvent, window, cx| {
                if manual {
                    match update::open_releases_page() {
                        Ok(()) => shutdown::close(window),
                        Err(error) => {
                            *prompt
                                .phase
                                .lock()
                                .expect("phase lock poisoned") =
                                Phase::Failed(error.to_string().into());
                            cx.notify();
                        },
                    }
                    return;
                }
                prompt.begin_update(window, cx);
            },
        );

        let dismiss = cx.listener(|_: &mut Self, _: &ClickEvent, window, _| {
            shutdown::close(window);
        });

        div()
            .relative()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .bg(rgb(palette.card_bg))
            .rounded(px(12.))
            .font_family("Inter")
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .w_full()
                    .h(px(HEIGHT - 52.))
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
                    .child(title),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING + 30.))
                    .top(px(SIDE_PADDING + 22.))
                    .text_size(px(11.))
                    .text_color(rgb(palette.soft_text))
                    .child(subtitle),
            )
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(78.))
                    .w(px(WIDTH - SIDE_PADDING * 2.))
                    .text_size(px(12.))
                    .line_height(px(17.))
                    .text_color(rgb(palette.muted_text))
                    .child(detail),
            )
            .child(
                div()
                    .absolute()
                    .right(px(SIDE_PADDING))
                    .bottom(px(SIDE_PADDING))
                    .flex()
                    .gap(px(8.))
                    // Once the work is finished there is nothing left to
                    // confirm, so only the dismiss button remains.
                    .when(!finished, |row| {
                        row.child(action_button(
                            "update-now",
                            primary_label,
                            ButtonTone::Success,
                            working,
                            &palette,
                            primary,
                        ))
                    })
                    .child(action_button(
                        "update-dismiss",
                        dismiss_label,
                        ButtonTone::Outline,
                        working,
                        &palette,
                        dismiss,
                    )),
            )
    }
}

/// Show the prompt and block until the user closes it.
pub fn show_update_prompt(info: UpdateInfo) {
    let info = Arc::new(info);

    Application::new().run(move |cx| {
        register_fonts(cx);
        quit_when_last_window_closes(cx);

        let bounds = Bounds::centered(None, size(px(WIDTH), px(HEIGHT)), cx);

        let opened = cx.open_window(notice_window_options(bounds), {
            let info = info.clone();
            move |_, cx| {
                cx.new(move |_| UpdatePrompt {
                    info: info.clone(),
                    phase: Arc::new(Mutex::new(Phase::Asking)),
                    appearance_observer: None,
                })
            }
        });

        if opened.is_err() {
            cx.quit();
            return;
        }

        cx.activate(true);
    });
}
