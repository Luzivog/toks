use std::time::Instant;

use gpui::{div, prelude::*, px, Context, Render, Window};
use gpui_component::ActiveTheme;

use crate::{app::sidebar_open_for_layout, title_bar::title_bar, ui, ToksApp};

const SIDEBAR_OVERLAY_BREAKPOINT: f32 = 1100.0;
const SIDEBAR_WIDTH: f32 = 250.0;

impl Render for ToksApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < px(SIDEBAR_OVERLAY_BREAKPOINT);
        let first_layout = self.compact_layout.is_none();
        if self.compact_layout != Some(compact) {
            self.sidebar_open =
                sidebar_open_for_layout(self.sidebar_open, self.compact_layout, compact);
            self.compact_layout = Some(compact);
        }
        let frame =
            self.sidebar_motion
                .update(self.sidebar_open, compact, first_layout, Instant::now());
        if frame.active {
            window.request_animation_frame();
        }
        let sidebar_width = if compact {
            px(0.)
        } else {
            px(SIDEBAR_WIDTH * frame.panel)
        };
        let detail_width = window.viewport_size().width - sidebar_width;

        div()
            .flex()
            .flex_row()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .when(!compact, |shell| {
                shell.child(
                    div()
                        .debug_selector(|| "sidebar-rail".into())
                        .h_full()
                        .w(px(SIDEBAR_WIDTH * frame.panel))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .when(frame.panel > 0.0, |rail| {
                            rail.child(
                                div()
                                    .relative()
                                    .left(px(-SIDEBAR_WIDTH * (1.0 - frame.panel)))
                                    .w(px(SIDEBAR_WIDTH))
                                    .h_full()
                                    .child(ui::sidebar(self, cx, false)),
                            )
                        }),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(title_bar(self, window, cx))
                    .child(ui::detail(self, detail_width, cx)),
            )
            .when(
                compact && (self.sidebar_open || frame.panel > 0.0 || frame.scrim > 0.0),
                |shell| {
                    shell
                        .child(
                            div()
                                .id("sidebar-dismiss")
                                .debug_selector(|| "sidebar-dismiss".into())
                                .absolute()
                                .top_0()
                                .right_0()
                                .bottom_0()
                                .left_0()
                                .cursor_pointer()
                                .occlude()
                                .bg(gpui::hsla(0.0, 0.0, 0.0, 0.45 * frame.scrim))
                                .when(self.sidebar_open, |dismiss| {
                                    dismiss.on_click(cx.listener(|app, _, _, cx| {
                                        app.sidebar_open = false;
                                        cx.notify();
                                    }))
                                }),
                        )
                        .child(
                            div()
                                .debug_selector(|| "sidebar-overlay-panel".into())
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .w(px(250.))
                                .occlude()
                                .shadow_xl()
                                .child(ui::sidebar(self, cx, true))
                                .left(px(-SIDEBAR_WIDTH * (1.0 - frame.panel))),
                        )
                },
            )
    }
}
