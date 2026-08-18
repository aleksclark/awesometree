//! GPUI cleanup UI for orphan host-local WorkSession runtime resources.

use crate::log as dlog;
use crate::model::lifecycle::WorkSessionState;
use crate::model::runtime::RealizationStatus;
use crate::runtime_store;
use crate::service_access;
use crate::theme::*;
use crate::ui_helpers::{self, button, chip_button, ButtonKind, ChipKind};
use crate::wm;
use gpui::prelude::FluentBuilder;
use gpui::*;

fn rt_block_on<F: std::future::Future>(f: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(h) => tokio::task::block_in_place(|| h.block_on(f)),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new().expect("tokio");
            rt.block_on(f)
        }
    }
}

#[derive(Clone)]
struct CleanupItem {
    work_session_id: String,
    project_id: String,
    lifecycle: String,
    path: String,
    orphan: bool,
}

pub fn open_cleanup_window(cx: &mut App) {
    let items = gather_items();
    let opts = ui_helpers::centered_window_options(cx, "awesometree-cleanup", 640., 420.);
    cx.open_window(
        opts,
        move |_window, cx| cx.new(move |_cx| CleanupView { items }),
    )
    .ok();
}

fn gather_items() -> Vec<CleanupItem> {
    let svc = service_access::service_blocking();
    let sessions = rt_block_on(svc.list_work_sessions(None, None)).unwrap_or_default();
    let runtimes = runtime_store::load_all().unwrap_or_default();
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for v in sessions {
        let id = v.work_session.work_session_id.clone();
        seen.insert(id.clone());
        let path = v
            .runtime
            .as_ref()
            .and_then(|r| r.workspace.as_ref())
            .map(|w| w.path.clone())
            .unwrap_or_default();
        items.push(CleanupItem {
            work_session_id: id,
            project_id: v.work_session.project_id.clone().unwrap_or_default(),
            lifecycle: v.work_session.state.to_string(),
            path,
            orphan: false,
        });
    }
    for (id, rt) in runtimes {
        if seen.contains(&id) {
            continue;
        }
        if matches!(
            rt.realization_status,
            RealizationStatus::Cleaned | RealizationStatus::Pending
        ) {
            continue;
        }
        items.push(CleanupItem {
            work_session_id: id,
            project_id: String::new(),
            lifecycle: "orphan-local".into(),
            path: rt.workspace.map(|w| w.path).unwrap_or_default(),
            orphan: true,
        });
    }
    items.sort_by(|a, b| a.work_session_id.cmp(&b.work_session_id));
    items
}

actions!(cleanup_ui, [DismissCleanup]);

struct CleanupView {
    items: Vec<CleanupItem>,
}

impl CleanupView {
    fn on_dismiss(&mut self, _: &DismissCleanup, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn destroy(&mut self, idx: usize, cx: &mut Context<Self>) {
        let item = self.items[idx].clone();
        dlog::log(format!("Cleanup destroy {}", item.work_session_id));
        let svc = service_access::service_blocking();
        let _ = wm::platform_adapter().restore_previous_tag();
        if item.orphan {
            let _ = runtime_store::remove(&item.work_session_id);
        } else {
            let _ = rt_block_on(svc.destroy(&item.work_session_id, false));
        }
        self.items = gather_items();
        cx.notify();
    }

    fn pause(&mut self, idx: usize, cx: &mut Context<Self>) {
        let id = self.items[idx].work_session_id.clone();
        let svc = service_access::service_blocking();
        let _ = rt_block_on(svc.transition(&id, WorkSessionState::Paused));
        self.items = gather_items();
        cx.notify();
    }
}

impl Render for CleanupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.items.clone();
        div()
            .key_context("Cleanup")
            .on_action(cx.listener(Self::on_dismiss))
            .flex()
            .flex_col()
            .size_full()
            .bg(bg())
            .text_color(fg())
            .font_family("monospace")
            .child(
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(border_color())
                    .text_size(px(16.))
                    .text_color(accent())
                    .child("Cleanup WorkSessions"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .p(px(12.))
                    .gap(px(6.))
                    .children(items.into_iter().enumerate().map(|(idx, item)| {
                        let orphan = item.orphan;
                        div()
                            .id(SharedString::from(format!("cu-{idx}")))
                            .flex()
                            .justify_between()
                            .items_center()
                            .px(px(10.))
                            .py(px(8.))
                            .rounded(px(4.))
                            .bg(bg_selected())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .child(item.work_session_id.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(fg_dim())
                                            .child(format!(
                                                "{} · {} · {}",
                                                item.project_id, item.lifecycle, item.path
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .when(!orphan, |s| {
                                        s.child(chip_button(
                                            SharedString::from(format!("pause-{idx}")),
                                            "Pause",
                                            ChipKind::Neutral,
                                            move |view, _w, cx| {
                                                view.pause(idx, cx);
                                            },
                                            cx,
                                        ))
                                    })
                                    .child(chip_button(
                                        SharedString::from(format!("destroy-{idx}")),
                                        "Destroy",
                                        ChipKind::Danger,
                                        move |view, _w, cx| {
                                            view.destroy(idx, cx);
                                        },
                                        cx,
                                    )),
                            )
                    })),
            )
            .child(
                div().px(px(16.)).py(px(10.)).child(button(
                    "close-cleanup",
                    "Close",
                    ButtonKind::Secondary,
                    |_v, window, _cx| {
                        window.remove_window();
                    },
                    cx,
                )),
            )
    }
}
