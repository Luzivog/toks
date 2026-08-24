use gpui::{px, Pixels};
#[cfg(test)]
use gpui::{Point, ResizeEdge, Size};

pub(super) const FRAME_INSET: Pixels = px(12.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameEdges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl FrameEdges {
    #[cfg(any(test, feature = "test-support"))]
    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };
}

#[cfg(test)]
pub(super) fn resize_edge_at(
    position: Point<Pixels>,
    size: Size<Pixels>,
    edges: FrameEdges,
) -> Option<ResizeEdge> {
    let top = edges.top && position.y >= px(0.0) && position.y < FRAME_INSET;
    let bottom =
        edges.bottom && position.y > size.height - FRAME_INSET && position.y <= size.height;
    let left = edges.left && position.x >= px(0.0) && position.x < FRAME_INSET;
    let right = edges.right && position.x > size.width - FRAME_INSET && position.x <= size.width;

    match (top, right, bottom, left) {
        (true, _, _, true) => Some(ResizeEdge::TopLeft),
        (true, true, _, _) => Some(ResizeEdge::TopRight),
        (_, true, true, _) => Some(ResizeEdge::BottomRight),
        (_, _, true, true) => Some(ResizeEdge::BottomLeft),
        (true, _, _, _) => Some(ResizeEdge::Top),
        (_, true, _, _) => Some(ResizeEdge::Right),
        (_, _, true, _) => Some(ResizeEdge::Bottom),
        (_, _, _, true) => Some(ResizeEdge::Left),
        _ => None,
    }
}
