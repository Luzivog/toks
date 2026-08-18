#[cfg(not(any(test, feature = "test-support")))]
use gpui::Decorations;
use gpui::{
    actions, div, prelude::*, AnyElement, AnyView, App, Context, CursorStyle, ParentElement,
    Render, RenderOnce, Window,
};
use gpui_component::ActiveTheme;

use super::geometry::{FrameEdges, FRAME_INSET};
use super::resize_zones::resize_zones;
use super::WindowAction;

actions!(tokscope_window, [Tab, TabPrev]);
const ROOT_CONTEXT: &str = "TokscopeRoot";

pub struct WindowFrame {
    view: AnyView,
    #[cfg(any(test, feature = "test-support"))]
    observed_action: Option<WindowAction>,
}

impl WindowFrame {
    pub fn new(view: impl Into<AnyView>) -> Self {
        Self {
            view: view.into(),
            #[cfg(any(test, feature = "test-support"))]
            observed_action: None,
        }
    }

    fn on_tab(&mut self, _: &Tab, window: &mut Window, _: &mut Context<Self>) {
        window.focus_next();
    }

    fn on_tab_prev(&mut self, _: &TabPrev, window: &mut Window, _: &mut Context<Self>) {
        window.focus_prev();
    }

    pub(crate) fn perform_window_action(action: WindowAction, window: &mut Window, cx: &mut App) {
        let Some(frame) = window.root::<Self>().flatten() else {
            return;
        };
        frame.update(cx, |frame, cx| frame.perform(action, window, cx));
    }

    fn perform(&mut self, action: WindowAction, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(any(test, feature = "test-support"))]
        {
            self.observed_action = Some(action);
            let _ = window;
            cx.notify();
        }
        #[cfg(not(any(test, feature = "test-support")))]
        {
            let _ = cx;
            match action {
                WindowAction::Minimize => window.minimize_window(),
                WindowAction::ToggleMaximize => window.zoom_window(),
                WindowAction::Close => window.remove_window(),
            }
        }
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "test-support"))]
    pub fn observed_action(&self) -> Option<WindowAction> {
        self.observed_action
    }
}

impl Render for WindowFrame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(cx.theme().font_size);
        ClientFrame::new().child(
            div()
                .id("tokscope-root")
                .key_context(ROOT_CONTEXT)
                .on_action(cx.listener(Self::on_tab))
                .on_action(cx.listener(Self::on_tab_prev))
                .relative()
                .size_full()
                .font_family(cx.theme().font_family.clone())
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .child(self.view.clone()),
        )
    }
}

#[derive(gpui::IntoElement, Default)]
struct ClientFrame {
    children: Vec<AnyElement>,
}

impl ClientFrame {
    fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for ClientFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ClientFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        #[cfg(any(test, feature = "test-support"))]
        let edges = {
            window.set_client_inset(FRAME_INSET);
            Some(FrameEdges::ALL)
        };
        #[cfg(not(any(test, feature = "test-support")))]
        let edges = match window.window_decorations() {
            Decorations::Server => None,
            Decorations::Client { tiling } => {
                window.set_client_inset(FRAME_INSET);
                Some(FrameEdges {
                    top: !tiling.top,
                    right: !tiling.right,
                    bottom: !tiling.bottom,
                    left: !tiling.left,
                })
            }
        };
        let border = cx.theme().window_border;
        let content = div()
            .cursor(CursorStyle::Arrow)
            .size_full()
            .overflow_hidden()
            .border_color(border)
            .when_some(edges, |content, edges| {
                content
                    .when(edges.top, |content| content.border_t_1())
                    .when(edges.right, |content| content.border_r_1())
                    .when(edges.bottom, |content| content.border_b_1())
                    .when(edges.left, |content| content.border_l_1())
            })
            .children(self.children);

        div()
            .id("tokscope-window-frame")
            .relative()
            .size_full()
            .bg(gpui::transparent_black())
            .when_some(edges, |frame, edges| {
                frame
                    .when(edges.top, |frame| frame.pt(FRAME_INSET))
                    .when(edges.right, |frame| frame.pr(FRAME_INSET))
                    .when(edges.bottom, |frame| frame.pb(FRAME_INSET))
                    .when(edges.left, |frame| frame.pl(FRAME_INSET))
            })
            .child(content)
            .when_some(edges, |frame, edges| frame.children(resize_zones(edges)))
    }
}
