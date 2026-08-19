use gpui::{div, prelude::*, AnyElement, CursorStyle, MouseButton, ResizeEdge};

use super::geometry::{FrameEdges, FRAME_INSET};

pub(super) fn resize_zones(edges: FrameEdges) -> Vec<AnyElement> {
    let mut zones = Vec::with_capacity(8);
    if edges.top {
        zones.push(horizontal_zone("resize-top", ResizeEdge::Top, true).into_any_element());
    }
    if edges.bottom {
        zones.push(horizontal_zone("resize-bottom", ResizeEdge::Bottom, false).into_any_element());
    }
    if edges.left {
        zones.push(vertical_zone("resize-left", ResizeEdge::Left, true).into_any_element());
    }
    if edges.right {
        zones.push(vertical_zone("resize-right", ResizeEdge::Right, false).into_any_element());
    }
    if edges.top && edges.left {
        zones.push(
            corner_zone("resize-top-left", ResizeEdge::TopLeft, true, true).into_any_element(),
        );
    }
    if edges.top && edges.right {
        zones.push(
            corner_zone("resize-top-right", ResizeEdge::TopRight, true, false).into_any_element(),
        );
    }
    if edges.bottom && edges.left {
        zones.push(
            corner_zone("resize-bottom-left", ResizeEdge::BottomLeft, false, true)
                .into_any_element(),
        );
    }
    if edges.bottom && edges.right {
        zones.push(
            corner_zone("resize-bottom-right", ResizeEdge::BottomRight, false, false)
                .into_any_element(),
        );
    }
    zones
}

fn resize_zone(
    id: &'static str,
    edge: ResizeEdge,
    cursor: CursorStyle,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .absolute()
        .cursor(cursor)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            window.start_window_resize(edge);
        })
}

fn horizontal_zone(id: &'static str, edge: ResizeEdge, top: bool) -> gpui::Stateful<gpui::Div> {
    resize_zone(id, edge, CursorStyle::ResizeUpDown)
        .left(FRAME_INSET)
        .right(FRAME_INSET)
        .h(FRAME_INSET)
        .when(top, |zone| zone.top_0())
        .when(!top, |zone| zone.bottom_0())
}

fn vertical_zone(id: &'static str, edge: ResizeEdge, left: bool) -> gpui::Stateful<gpui::Div> {
    resize_zone(id, edge, CursorStyle::ResizeLeftRight)
        .top(FRAME_INSET)
        .bottom(FRAME_INSET)
        .w(FRAME_INSET)
        .when(left, |zone| zone.left_0())
        .when(!left, |zone| zone.right_0())
}

fn corner_zone(
    id: &'static str,
    edge: ResizeEdge,
    top: bool,
    left: bool,
) -> gpui::Stateful<gpui::Div> {
    let cursor = if top == left {
        CursorStyle::ResizeUpLeftDownRight
    } else {
        CursorStyle::ResizeUpRightDownLeft
    };
    resize_zone(id, edge, cursor)
        .size(FRAME_INSET)
        .when(top, |zone| zone.top_0())
        .when(!top, |zone| zone.bottom_0())
        .when(left, |zone| zone.left_0())
        .when(!left, |zone| zone.right_0())
}
