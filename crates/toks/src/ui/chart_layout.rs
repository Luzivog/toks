use gpui::{div, prelude::*, Div};

/// Give every summary-and-chart card one definite row width. Keeping the chart
/// as the direct flex item avoids percentage sizing against an unresolved
/// intermediate wrapper when an Overview card moves onto its own flex line.
pub(super) fn summary_chart_row(summary: Div, chart: Div) -> Div {
    div()
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .gap_6()
        .child(summary)
        .child(chart.flex_1().min_w_0())
}
