use crate::interop;
use crate::log as dlog;
use crate::state;
use crate::theme;
use crate::workspace;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn open_cleanup_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(800.), px(550.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-cleanup".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| {
            cx.new(|cx| {
                let mut view = CleanupView::new(cx);
                view.start_size_scan(cx);
                view
            })
        },
    )
    .ok();
}

actions!(cleanup_ui, [DismissCleanup, SelectAll, DeselectAll]);

fn bg() -> Rgba { theme::bg() }
fn bg_hover() -> Rgba { theme::bg_hover() }
fn fg() -> Rgba { theme::fg() }
fn fg_dim() -> Rgba { theme::fg_dim() }
fn accent() -> Rgba { theme::accent() }
fn border_color() -> Rgba { theme::border_color() }
fn btn_bg() -> Rgba { theme::btn_bg() }
fn btn_fg() -> Rgba { theme::btn_fg() }
fn btn_hover() -> Rgba { theme::btn_hover() }
fn danger() -> Rgba { theme::danger() }
fn success() -> Rgba { theme::success() }
fn active_dot() -> Rgba { theme::active_dot() }

#[derive(Clone)]
struct WorkspaceEntry {
    name: String,
    project: String,
    active: bool,
    dir: String,
    disk_size: Option<u64>,
    scanning: bool,
}

struct CleanupView {
    entries: Vec<WorkspaceEntry>,
    selected: HashMap<String, bool>,
    status_message: Option<String>,
    focus: FocusHandle,
    spin_frame: usize,
    size_results: Arc<Mutex<Vec<(String, u64)>>>,
}

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl CleanupView {
    fn new(cx: &mut Context<Self>) -> Self {
        let st = state::load().unwrap_or_default();

        let mut entries: Vec<WorkspaceEntry> = Vec::new();
        for (ws_name, ws) in &st.workspaces {
            entries.push(WorkspaceEntry {
                name: ws_name.clone(),
                project: ws.project.clone(),
                active: ws.active,
                dir: ws.dir.clone(),
                disk_size: None,
                scanning: true,
            });
        }
        entries.sort_by(|a, b| a.project.cmp(&b.project).then(a.name.cmp(&b.name)));

        Self {
            entries,
            selected: HashMap::new(),
            status_message: None,
            focus: cx.focus_handle(),
            spin_frame: 0,
            size_results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn start_size_scan(&mut self, cx: &mut Context<Self>) {
        let projects = interop::list().unwrap_or_default();

        let jobs: Vec<(String, PathBuf)> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let dir = if !entry.dir.is_empty() {
                    let d = PathBuf::from(&entry.dir);
                    if d.exists() { d } else { return None }
                } else {
                    let proj = projects.iter().find(|p| p.name == entry.project)?;
                    let d = workspace::resolve_dir(&entry.name, proj);
                    if d.exists() { d } else { return None }
                };
                Some((entry.name.clone(), dir))
            })
            .collect();

        let has_dir: std::collections::HashSet<&str> =
            jobs.iter().map(|(n, _)| n.as_str()).collect();
        for entry in &mut self.entries {
            if !has_dir.contains(entry.name.as_str()) {
                entry.scanning = false;
            }
        }

        if jobs.is_empty() {
            return;
        }

        let results = self.size_results.clone();
        std::thread::spawn(move || {
            let sem = Arc::new((Mutex::new(0u32), std::sync::Condvar::new()));
            let mut handles = Vec::new();

            for (name, dir) in jobs {
                let results = results.clone();
                let sem = sem.clone();

                handles.push(std::thread::spawn(move || {
                    {
                        let (lock, cvar) = &*sem;
                        let mut count = lock.lock().unwrap();
                        while *count >= 3 {
                            count = cvar.wait(count).unwrap();
                        }
                        *count += 1;
                    }

                    let size = dir_size(&dir);
                    results.lock().unwrap().push((name, size));

                    {
                        let (lock, cvar) = &*sem;
                        let mut count = lock.lock().unwrap();
                        *count -= 1;
                        cvar.notify_one();
                    }
                }));
            }

            for h in handles {
                let _ = h.join();
            }
        });

        self.schedule_poll(cx);
    }

    fn schedule_poll(&self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this: WeakEntity<CleanupView>, cx: &mut AsyncApp| {
            loop {
                executor.timer(std::time::Duration::from_millis(100)).await;
                let should_continue = cx.update(|cx| {
                    this.update(cx, |view: &mut CleanupView, cx: &mut Context<CleanupView>| {
                        view.drain_results();
                        view.spin_frame = (view.spin_frame + 1) % SPINNER.len();
                        cx.notify();
                        view.entries.iter().any(|e| e.scanning)
                    })
                    .unwrap_or(false)
                });
                if !should_continue.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_results(&mut self) {
        let mut results = self.size_results.lock().unwrap();
        for (name, size) in results.drain(..) {
            if let Some(entry) = self.entries.iter_mut().find(|e| e.name == name) {
                entry.disk_size = Some(size);
                entry.scanning = false;
            }
        }
    }

    fn is_selected(&self, name: &str) -> bool {
        self.selected.get(name).copied().unwrap_or(false)
    }

    fn toggle_selected(&mut self, name: &str) {
        let current = self.is_selected(name);
        self.selected.insert(name.to_string(), !current);
    }

    fn selected_count(&self) -> usize {
        self.selected.values().filter(|&&v| v).count()
    }

    fn selected_names(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter(|(_, v)| **v)
            .map(|(k, _)| k.clone())
            .collect()
    }

    fn on_dismiss(&mut self, _: &DismissCleanup, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn on_select_all(&mut self, _: &SelectAll, _window: &mut Window, cx: &mut Context<Self>) {
        for entry in &self.entries {
            self.selected.insert(entry.name.clone(), true);
        }
        cx.notify();
    }

    fn on_deselect_all(&mut self, _: &DeselectAll, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected.clear();
        cx.notify();
    }

    fn bring_down_selected(&mut self, cx: &mut Context<Self>) {
        let names = self.selected_names();
        if names.is_empty() {
            return;
        }

        let count = names.len();
        let mut errors: Vec<String> = Vec::new();

        for name in &names {
            dlog::log(format!("Cleanup: bringing down workspace {name}"));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let st = state::load().map_err(|e| format!("load state: {e}"))?;
                let wm = crate::wm::platform_adapter();
                let mut mgr = crate::workspace::Manager::new(st, wm);
                mgr.down(
                    name,
                    &crate::workspace::DownOptions {
                        manage_tag: true,
                        keep_worktree: true,
                    },
                )
            }));
            match result {
                Ok(Ok(())) => {
                    if let Some(entry) = self.entries.iter_mut().find(|e| e.name == *name) {
                        entry.active = false;
                    }
                }
                Ok(Err(e)) => errors.push(format!("{name}: {e}")),
                Err(_) => errors.push(format!("{name}: panic")),
            }
        }

        self.selected.clear();
        if errors.is_empty() {
            self.status_message = Some(format!("{count} workspace{} brought down", if count == 1 { "" } else { "s" }));
        } else {
            self.status_message = Some(format!("Errors: {}", errors.join(", ")));
        }
        cx.notify();
    }

    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let names = self.selected_names();
        if names.is_empty() {
            return;
        }

        let count = names.len();
        let mut errors: Vec<String> = Vec::new();

        for name in &names {
            dlog::log(format!("Cleanup: destroying workspace {name}"));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let st = state::load().map_err(|e| format!("load state: {e}"))?;
                let wm = crate::wm::platform_adapter();
                let mut mgr = crate::workspace::Manager::new(st, wm);
                mgr.destroy(
                    name,
                    &crate::workspace::DownOptions {
                        manage_tag: true,
                        keep_worktree: false,
                    },
                )
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => errors.push(format!("{name}: {e}")),
                Err(_) => errors.push(format!("{name}: panic")),
            }
        }

        self.entries.retain(|e| !names.contains(&e.name));
        self.selected.clear();
        if errors.is_empty() {
            self.status_message = Some(format!("{count} workspace{} deleted", if count == 1 { "" } else { "s" }));
        } else {
            self.status_message = Some(format!("Errors: {}", errors.join(", ")));
        }
        cx.notify();
    }
}

fn dir_size(path: &PathBuf) -> u64 {
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                total += dir_size(&entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.1} MB");
    }
    let gb = mb / 1024.0;
    format!("{gb:.2} GB")
}

fn render_toolbar(selected_count: usize, total_size: u64, cx: &mut Context<'_, CleanupView>) -> AnyElement {
    div()
        .px(px(20.))
        .py(px(14.))
        .border_b_1()
        .border_color(border_color())
        .flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .text_size(px(18.))
                        .text_color(accent())
                        .child("Workspace Cleanup"),
                )
                .when(selected_count > 0, |s: Div| {
                    s.child(
                        div()
                            .text_size(px(12.))
                            .text_color(fg_dim())
                            .child(format!(
                                "{selected_count} selected{}",
                                if total_size > 0 {
                                    format!(" · {}", format_size(total_size))
                                } else {
                                    String::new()
                                }
                            )),
                    )
                }),
        )
        .child(render_action_buttons(selected_count, cx))
        .into_any_element()
}

fn render_action_buttons(selected_count: usize, cx: &mut Context<'_, CleanupView>) -> AnyElement {
    div()
        .flex()
        .gap(px(8.))
        .when(selected_count > 0, |s: Div| {
            s.child(
                div()
                    .id("down-btn")
                    .px(px(14.))
                    .py(px(6.))
                    .rounded(px(4.))
                    .bg(btn_bg())
                    .text_color(btn_fg())
                    .cursor_pointer()
                    .hover(|s| s.bg(btn_hover()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            view.bring_down_selected(cx);
                        }),
                    )
                    .child(div().text_size(px(13.)).child("↓ Bring Down")),
            )
            .child(
                div()
                    .id("delete-btn")
                    .px(px(14.))
                    .py(px(6.))
                    .rounded(px(4.))
                    .bg(danger())
                    .text_color(btn_fg())
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            view.delete_selected(cx);
                        }),
                    )
                    .child(div().text_size(px(13.)).child("✕ Delete")),
            )
        })
        .into_any_element()
}

fn render_ws_row(entry: &WorkspaceEntry, checked: bool, spin_frame: usize, cx: &mut Context<'_, CleanupView>) -> AnyElement {
    let name = entry.name.clone();
    let name_for_click = name.clone();

    div()
        .id(ElementId::Name(format!("ws-{name}").into()))
        .px(px(20.))
        .py(px(6.))
        .border_b_1()
        .border_color(border_color())
        .hover(|s| s.bg(bg_hover()))
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                view.toggle_selected(&name_for_click);
                cx.notify();
            }),
        )
        .flex()
        .items_center()
        .gap(px(10.))
        .child(render_checkbox(checked))
        .child(render_ws_info(&name, entry, checked, spin_frame))
        .into_any_element()
}

fn render_checkbox(checked: bool) -> AnyElement {
    div()
        .size(px(16.))
        .rounded(px(3.))
        .border_1()
        .border_color(if checked { accent() } else { fg_dim() })
        .when(checked, |s: Div| s.bg(accent()))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |s: Div| {
            s.child(
                div()
                    .text_size(px(11.))
                    .text_color(btn_fg())
                    .child("✓"),
            )
        })
        .into_any_element()
}

fn render_ws_info(name: &str, entry: &WorkspaceEntry, checked: bool, spin_frame: usize) -> AnyElement {
    let (label, color) = if entry.active {
        ("UP", active_dot())
    } else {
        ("DOWN", fg_dim())
    };

    let mut info = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .px(px(6.))
                .py(px(1.))
                .rounded(px(3.))
                .border_1()
                .border_color(color)
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(color)
                        .child(label),
                ),
        );
    if entry.scanning {
        info = info.child(
            div()
                .text_size(px(12.))
                .text_color(fg_dim())
                .child(SPINNER[spin_frame]),
        );
    } else if let Some(size) = entry.disk_size {
        info = info.child(
            div()
                .text_size(px(12.))
                .text_color(fg_dim())
                .child(format_size(size)),
        );
    }

    div()
        .flex()
        .flex_1()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(14.))
                .text_color(if checked { accent() } else { fg() })
                .child(name.to_string()),
        )
        .child(info)
        .into_any_element()
}

fn render_project_group(
    project: &str,
    ws_entries: &[WorkspaceEntry],
    selected: &HashMap<String, bool>,
    spin_frame: usize,
    cx: &mut Context<'_, CleanupView>,
) -> AnyElement {
    let project_total: u64 = ws_entries
        .iter()
        .filter_map(|e| e.disk_size)
        .sum();
    let any_scanning = ws_entries.iter().any(|e| e.scanning);

    let mut header = div()
        .px(px(20.))
        .pt(px(14.))
        .pb(px(6.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(fg_dim())
                .child(project.to_uppercase()),
        );
    if any_scanning {
        header = header.child(
            div()
                .text_size(px(11.))
                .text_color(fg_dim())
                .child(format!(
                    "({}{})",
                    if project_total > 0 { format!("{}+", format_size(project_total)) } else { String::new() },
                    SPINNER[spin_frame],
                )),
        );
    } else if project_total > 0 {
        header = header.child(
            div()
                .text_size(px(11.))
                .text_color(fg_dim())
                .child(format!("({})", format_size(project_total))),
        );
    }

    let mut group = div()
        .flex()
        .flex_col()
        .child(header);

    for entry in ws_entries {
        let checked = selected.get(&entry.name).copied().unwrap_or(false);
        group = group.child(render_ws_row(entry, checked, spin_frame, cx));
    }

    group.into_any_element()
}

fn render_footer(status: Option<String>) -> AnyElement {
    div()
        .px(px(20.))
        .py(px(10.))
        .border_t_1()
        .border_color(border_color())
        .flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .text_size(px(11.))
                .text_color(fg_dim())
                .child("Click rows to select  ·  Esc to close"),
        )
        .when_some(status, |s, msg| {
            s.child(
                div()
                    .text_size(px(12.))
                    .text_color(success())
                    .child(msg),
            )
        })
        .into_any_element()
}

impl Render for CleanupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries.clone();
        let selected_count = self.selected_count();
        let status = self.status_message.clone();
        let spin_frame = self.spin_frame;

        let mut groups: Vec<(String, Vec<WorkspaceEntry>)> = Vec::new();
        for entry in &entries {
            if let Some(last) = groups.last_mut() {
                if last.0 == entry.project {
                    last.1.push(entry.clone());
                    continue;
                }
            }
            groups.push((entry.project.clone(), vec![entry.clone()]));
        }

        let total_size: u64 = entries
            .iter()
            .filter(|e| self.is_selected(&e.name))
            .filter_map(|e| e.disk_size)
            .sum();

        let toolbar = render_toolbar(selected_count, total_size, cx);

        let mut list_children: Vec<AnyElement> = Vec::new();
        for (project, ws_entries) in &groups {
            list_children.push(render_project_group(project, ws_entries, &self.selected, spin_frame, cx));
        }

        if self.entries.is_empty() {
            list_children.push(
                div()
                    .px(px(20.))
                    .py(px(40.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(fg_dim())
                            .child("No workspaces found."),
                    )
                    .into_any_element(),
            );
        }

        let footer = render_footer(status);

        div()
            .key_context("Cleanup")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_deselect_all))
            .flex()
            .flex_col()
            .size_full()
            .bg(bg())
            .text_color(fg())
            .font_family("monospace")
            .child(toolbar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .id("cleanup-scroll")
                    .overflow_y_scroll()
                    .children(list_children),
            )
            .child(footer)
    }
}

impl Focusable for CleanupView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
