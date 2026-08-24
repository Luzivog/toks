use super::geometry::{resize_edge_at, FrameEdges};
use gpui::{point, px, size, ResizeEdge};

#[test]
fn resize_geometry_is_limited_to_edges() {
    let size = size(px(940.0), px(620.0));
    assert_eq!(
        resize_edge_at(point(px(5.0), px(5.0)), size, FrameEdges::ALL),
        Some(ResizeEdge::TopLeft)
    );
    assert_eq!(
        resize_edge_at(point(px(935.0), px(615.0)), size, FrameEdges::ALL),
        Some(ResizeEdge::BottomRight)
    );
    assert_eq!(
        resize_edge_at(point(px(470.0), px(310.0)), size, FrameEdges::ALL),
        None
    );
}

#[test]
fn tiled_edges_do_not_resize() {
    let size = size(px(940.0), px(620.0));
    let edges = FrameEdges {
        top: false,
        ..FrameEdges::ALL
    };
    assert_eq!(resize_edge_at(point(px(470.0), px(5.0)), size, edges), None);
    assert_eq!(
        resize_edge_at(point(px(5.0), px(5.0)), size, edges),
        Some(ResizeEdge::Left)
    );
}
