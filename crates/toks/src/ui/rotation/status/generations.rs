use gpui::{div, prelude::*, px, SharedString};
use gpui_component::{h_flex, tooltip::Tooltip, ActiveTheme, StyledExt};
use toks_core::codex_router::{RouterGenerationRole, RouterGenerationSummary};

use crate::ToksApp;

pub(super) fn rows(app: &ToksApp, cx: &gpui::App) -> Option<gpui::Div> {
    if !app.rotation.install.service_installed {
        return None;
    }
    let generations = app
        .rotation
        .deployment
        .generations
        .iter()
        .filter_map(|generation| role_label(generation.role).map(|label| (generation, label)))
        .collect::<Vec<_>>();
    if generations.is_empty() {
        return None;
    }
    let waiting_generation = if app.rotation.deployment.update_waiting {
        generations
            .iter()
            .find(|(generation, _)| generation.role == RouterGenerationRole::Draining)
            .or_else(|| {
                generations
                    .iter()
                    .find(|(generation, _)| generation.role == RouterGenerationRole::Pending)
            })
            .or_else(|| generations.first())
            .map(|(generation, _)| generation.generation)
    } else {
        None
    };
    let mut rows = gpui_component::v_flex()
        .debug_selector(|| "rotation-router-generations".into())
        .border_t_1()
        .border_color(cx.theme().border);
    for (generation, label) in generations {
        rows = rows.child(generation_row(
            app,
            generation,
            label,
            waiting_generation == Some(generation.generation),
            cx,
        ));
    }
    Some(rows)
}

fn generation_row(
    app: &ToksApp,
    generation: &RouterGenerationSummary,
    role_label: &'static str,
    update_waiting: bool,
    cx: &gpui::App,
) -> gpui::Div {
    let selector = format!("rotation-router-generation-{}", generation.generation);
    let build_selector = format!("rotation-router-build-{}", generation.generation);
    let workload_selector = format!("rotation-router-workload-{}", generation.generation);
    let exact_build = generation.build.clone();
    let oldest_exact = generation
        .oldest_task_at
        .map(super::super::format::exact_time);
    h_flex()
        .debug_selector(move || selector.clone())
        .min_h(px(36.))
        .gap_2()
        .px_4()
        .py_1p5()
        .child(
            div()
                .size(px(6.))
                .flex_shrink_0()
                .rounded_full()
                .bg(role_color(app.rotation.install.service_active, cx)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .font_medium()
                .child(role_label),
        )
        .child(
            div()
                .id(SharedString::from(build_selector.clone()))
                .debug_selector(move || build_selector.clone())
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} · generation {}",
                    short_build(&generation.build),
                    generation.generation
                ))
                .tooltip(move |window, cx| {
                    let build = exact_build.clone();
                    Tooltip::element(move |_, _| div().child(build.clone())).build(window, cx)
                }),
        )
        .child(div().flex_1())
        .when(update_waiting, |row| {
            row.child(
                div()
                    .debug_selector(|| "rotation-router-update-waiting".into())
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .child("Update waiting"),
            )
        })
        .child(
            div()
                .id(SharedString::from(workload_selector.clone()))
                .debug_selector(move || workload_selector.clone())
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(workload_label(app, generation))
                .when_some(oldest_exact, |label, exact| {
                    label.tooltip(move |window, cx| {
                        let exact = exact.clone();
                        Tooltip::element(move |_, _| div().child(exact.clone())).build(window, cx)
                    })
                }),
        )
}

fn role_label(role: RouterGenerationRole) -> Option<&'static str> {
    match role {
        RouterGenerationRole::Active => None,
        RouterGenerationRole::Pending => Some("Pending build"),
        RouterGenerationRole::Draining => Some("Draining build"),
    }
}

fn role_color(service_active: bool, cx: &gpui::App) -> gpui::Hsla {
    if !service_active {
        return cx.theme().muted_foreground;
    }
    cx.theme().warning
}

fn workload_label(app: &ToksApp, generation: &RouterGenerationSummary) -> String {
    let tasks = match generation.task_count {
        0 => return "0 tasks".into(),
        1 => "1 task".into(),
        count => format!("{count} tasks"),
    };
    let Some(at) = generation.oldest_task_at else {
        return tasks;
    };
    format!(
        "{tasks} · oldest {}",
        super::super::format::age(app.now, at)
    )
}

fn short_build(build: &str) -> String {
    const MAX_CHARS: usize = 12;
    if build.chars().count() <= MAX_CHARS {
        return build.to_owned();
    }
    format!("{}…", build.chars().take(MAX_CHARS).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::short_build;

    #[test]
    fn build_label_is_compact_without_losing_short_names() {
        assert_eq!(short_build("build-a"), "build-a");
        assert_eq!(short_build("0123456789abcdef"), "0123456789ab…");
    }
}
