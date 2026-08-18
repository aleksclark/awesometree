//! GPUI Projects browser backed by Switchboard Project Catalog.

use crate::log as dlog;
use crate::model::project::{definition_for_create, AwesometreeExt, ProjectSummary};
use crate::service_access;
use crate::text_input::TextInput;
use crate::theme::*;
use crate::ui_helpers::{self, button, chip_button, ButtonKind, ChipKind};
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

fn list_projects_blocking() -> Vec<ProjectSummary> {
    let svc = service_access::service_blocking();
    rt_block_on(svc.list_projects(None)).unwrap_or_default()
}

pub fn open_projects_window(cx: &mut App) {
    let projects = list_projects_blocking();
    crate::text_input::bind_text_input_keys(cx);
    let opts = ui_helpers::centered_window_options(cx, "awesometree-projects", 700., 500.);
    cx.open_window(
        opts,
        move |_window, cx| cx.new(move |cx| ProjectsView::new(projects, cx)),
    )
    .ok();
}

pub fn run_projects_ui() {
    let projects = list_projects_blocking();
    let app = Application::new();
    app.run(move |cx: &mut App| {
        crate::text_input::bind_text_input_keys(cx);
        cx.bind_keys([
            KeyBinding::new("escape", Dismiss, None),
            KeyBinding::new("enter", ConfirmAction, None),
            KeyBinding::new("tab", NextField, None),
            KeyBinding::new("shift-tab", PrevField, None),
        ]);
        let opts = ui_helpers::centered_window_options(cx, "awesometree-projects", 700., 500.);
        cx.open_window(
            opts,
            move |_window, cx| cx.new(move |cx| ProjectsView::new(projects, cx)),
        )
        .ok();
    });
}

actions!(projects_ui, [Dismiss, ConfirmAction, NextField, PrevField]);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    List,
    Adding,
    Editing(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum FormField {
    Name,
    Repo,
    Branch,
    WorktreeDir,
    McpUrl,
    App(usize),
}

struct ProjectsView {
    projects: Vec<ProjectSummary>,
    /// CAS tokens for edits: project_id → source_revision
    source_revisions: std::collections::HashMap<String, String>,
    mode: Mode,
    form_name: Entity<TextInput>,
    form_repo: Entity<TextInput>,
    form_branch: Entity<TextInput>,
    form_worktree_dir: Entity<TextInput>,
    form_mcp_url: Entity<TextInput>,
    form_apps: Vec<Entity<TextInput>>,
    form_field: FormField,
    form_error: Option<String>,
    focus: FocusHandle,
}

impl ProjectsView {
    fn new(projects: Vec<ProjectSummary>, cx: &mut Context<Self>) -> Self {
        let mut source_revisions = std::collections::HashMap::new();
        for p in &projects {
            if let Some(rev) = &p.source_revision {
                source_revisions.insert(p.project_id.clone(), rev.clone());
            }
        }
        Self {
            projects,
            source_revisions,
            mode: Mode::List,
            form_name: cx.new(|cx| TextInput::new("project id", cx)),
            form_repo: cx.new(|cx| TextInput::new("/path/to/git/repo", cx)),
            form_branch: cx.new(|cx| TextInput::new("master", cx)),
            form_worktree_dir: cx.new(|cx| TextInput::new("optional worktree base", cx)),
            form_mcp_url: cx.new(|cx| TextInput::new("optional mcp url", cx)),
            form_apps: vec![cx.new(|cx| TextInput::new("e.g. zeditor -n {dir}", cx))],
            form_field: FormField::Name,
            form_error: None,
            focus: cx.focus_handle(),
        }
    }

    fn reload(&mut self) {
        self.projects = list_projects_blocking();
        self.source_revisions.clear();
        for p in &self.projects {
            if let Some(rev) = &p.source_revision {
                self.source_revisions
                    .insert(p.project_id.clone(), rev.clone());
            }
        }
    }

    fn read_field(&self, field: FormField, cx: &App) -> String {
        match field {
            FormField::Name => self.form_name.read(cx).value().to_string(),
            FormField::Repo => self.form_repo.read(cx).value().to_string(),
            FormField::Branch => self.form_branch.read(cx).value().to_string(),
            FormField::WorktreeDir => self.form_worktree_dir.read(cx).value().to_string(),
            FormField::McpUrl => self.form_mcp_url.read(cx).value().to_string(),
            FormField::App(i) => self
                .form_apps
                .get(i)
                .map(|e| e.read(cx).value().to_string())
                .unwrap_or_default(),
        }
    }

    fn clear_form(&mut self, cx: &mut Context<Self>) {
        for e in [
            &self.form_name,
            &self.form_repo,
            &self.form_branch,
            &self.form_worktree_dir,
            &self.form_mcp_url,
        ] {
            e.update(cx, |input, cx| input.set_value("", cx));
        }
        self.form_apps = vec![cx.new(|cx| TextInput::new("e.g. zeditor -n {dir}", cx))];
        self.form_field = FormField::Name;
        self.form_error = None;
    }

    fn start_add(&mut self, cx: &mut Context<Self>) {
        self.clear_form(cx);
        self.mode = Mode::Adding;
    }

    fn start_edit(&mut self, idx: usize, cx: &mut Context<Self>) {
        let p = &self.projects[idx];
        let svc = service_access::service_blocking();
        let env = match rt_block_on(svc.get_project(&p.project_id)) {
            Ok(e) => e,
            Err(e) => {
                self.form_error = Some(format!("load project: {e}"));
                return;
            }
        };
        self.source_revisions
            .insert(env.project_id.clone(), env.source_revision.clone());
        let ext = env.awesometree_ext();
        self.form_name.update(cx, |i, cx| {
            i.set_value(&env.project_id, cx);
        });
        self.form_repo.update(cx, |i, cx| {
            i.set_value(&env.primary_repo().unwrap_or_default(), cx);
        });
        self.form_branch.update(cx, |i, cx| {
            i.set_value(&env.branch().unwrap_or_else(|| "master".into()), cx);
        });
        self.form_worktree_dir.update(cx, |i, cx| {
            i.set_value(&ext.worktree_dir.clone().unwrap_or_default(), cx);
        });
        self.form_mcp_url.update(cx, |i, cx| {
            i.set_value(&ext.mcp.clone().unwrap_or_default(), cx);
        });
        self.form_apps = if ext.apps.is_empty() {
            vec![cx.new(|cx| TextInput::new("e.g. zeditor -n {dir}", cx))]
        } else {
            ext.apps
                .iter()
                .map(|app| {
                    cx.new(|cx| {
                        let mut input = TextInput::new("e.g. zeditor -n {dir}", cx);
                        input.set_value(app, cx);
                        input
                    })
                })
                .collect()
        };
        self.form_field = FormField::Name;
        self.form_error = None;
        self.mode = Mode::Editing(idx);
    }

    fn save_form(&mut self, cx: &mut Context<Self>) {
        let name = self.read_field(FormField::Name, cx);
        let repo = self.read_field(FormField::Repo, cx);
        let branch_val = self.read_field(FormField::Branch, cx);
        let worktree_dir = self.read_field(FormField::WorktreeDir, cx);
        let mcp_url = self.read_field(FormField::McpUrl, cx);
        let apps: Vec<String> = self
            .form_apps
            .iter()
            .map(|e| e.read(cx).value().to_string())
            .filter(|s| !s.trim().is_empty())
            .collect();
        if name.is_empty() {
            return;
        }
        let branch = if branch_val.is_empty() {
            None
        } else {
            Some(branch_val.as_str())
        };
        let ext = AwesometreeExt {
            mcp: if mcp_url.is_empty() {
                None
            } else {
                Some(mcp_url)
            },
            apps,
            layout: String::new(),
            worktree_dir: if worktree_dir.is_empty() {
                None
            } else {
                Some(worktree_dir)
            },
        };
        let def = definition_for_create(
            &name,
            None,
            if repo.is_empty() { None } else { Some(&repo) },
            branch,
            Some(&ext),
        );
        let svc = service_access::service_blocking();
        match self.mode {
            Mode::Adding => {
                dlog::log(format!("Adding project via Switchboard: {name}"));
                match rt_block_on(svc.create_project(def)) {
                    Ok(_) => {
                        self.reload();
                        self.mode = Mode::List;
                        self.clear_form(cx);
                    }
                    Err(e) => self.form_error = Some(e.to_string()),
                }
            }
            Mode::Editing(_) => {
                let expected = self
                    .source_revisions
                    .get(&name)
                    .cloned()
                    .unwrap_or_default();
                if expected.is_empty() {
                    self.form_error =
                        Some("missing sourceRevision for CAS update; reload and retry".into());
                    return;
                }
                dlog::log(format!("Updating project via Switchboard: {name}"));
                // Full definition replace via patch with definition body.
                let patch = serde_json::json!({ "definition": def });
                match rt_block_on(svc.update_project(&name, &expected, patch)) {
                    Ok(sum) => {
                        if let Some(rev) = sum.source_revision {
                            self.source_revisions.insert(name, rev);
                        }
                        self.reload();
                        self.mode = Mode::List;
                        self.clear_form(cx);
                    }
                    Err(e) => self.form_error = Some(e.to_string()),
                }
            }
            Mode::List => {}
        }
    }

    fn delete_project(&mut self, idx: usize, cx: &mut Context<Self>) {
        let p = &self.projects[idx];
        let id = p.project_id.clone();
        let expected = self
            .source_revisions
            .get(&id)
            .cloned()
            .or_else(|| p.source_revision.clone())
            .unwrap_or_default();
        dlog::log(format!("Deleting project via Switchboard: {id}"));
        let svc = service_access::service_blocking();
        match rt_block_on(svc.delete_project(&id, &expected)) {
            Ok(()) => {
                self.reload();
                if let Mode::Editing(ei) = self.mode {
                    if ei == idx {
                        self.mode = Mode::List;
                        self.clear_form(cx);
                    }
                }
            }
            Err(e) => self.form_error = Some(e.to_string()),
        }
    }

    fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, _cx: &mut Context<Self>) {
        match self.mode {
            Mode::List => window.remove_window(),
            _ => {
                self.mode = Mode::List;
                self.form_error = None;
            }
        }
    }

    fn on_confirm(&mut self, _: &ConfirmAction, _window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.mode, Mode::Adding | Mode::Editing(_)) {
            self.save_form(cx);
            cx.notify();
        }
    }

    fn on_next_field(&mut self, _: &NextField, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.mode, Mode::List) {
            return;
        }
        self.form_field = match self.form_field {
            FormField::Name => FormField::Repo,
            FormField::Repo => FormField::Branch,
            FormField::Branch => FormField::WorktreeDir,
            FormField::WorktreeDir => FormField::McpUrl,
            FormField::McpUrl => FormField::App(0),
            FormField::App(i) if i + 1 < self.form_apps.len() => FormField::App(i + 1),
            FormField::App(_) => FormField::Name,
        };
        self.focus_active(window, cx);
        cx.notify();
    }

    fn on_prev_field(&mut self, _: &PrevField, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.mode, Mode::List) {
            return;
        }
        self.form_field = match self.form_field {
            FormField::Name => FormField::App(self.form_apps.len().saturating_sub(1)),
            FormField::Repo => FormField::Name,
            FormField::Branch => FormField::Repo,
            FormField::WorktreeDir => FormField::Branch,
            FormField::McpUrl => FormField::WorktreeDir,
            FormField::App(0) => FormField::McpUrl,
            FormField::App(i) => FormField::App(i - 1),
        };
        self.focus_active(window, cx);
        cx.notify();
    }

    fn focus_active(&self, window: &mut Window, cx: &App) {
        let entity = match self.form_field {
            FormField::Name => &self.form_name,
            FormField::Repo => &self.form_repo,
            FormField::Branch => &self.form_branch,
            FormField::WorktreeDir => &self.form_worktree_dir,
            FormField::McpUrl => &self.form_mcp_url,
            FormField::App(i) => self.form_apps.get(i).unwrap_or(&self.form_name),
        };
        window.focus(&entity.read(cx).focus_handle(cx));
    }
}

impl Render for ProjectsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let err = self.form_error.clone();
        div()
            .key_context("Projects")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next_field))
            .on_action(cx.listener(Self::on_prev_field))
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
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(accent())
                            .child("Projects (Switchboard)"),
                    )
                    .child(button(
                        "add-proj",
                        "+ Add",
                        ButtonKind::Primary,
                        |view, _w, cx| {
                            view.start_add(cx);
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .when_some(err, |s, e| {
                s.child(
                    div()
                        .px(px(16.))
                        .py(px(8.))
                        .text_color(new_badge_fg())
                        .text_size(px(12.))
                        .child(e),
                )
            })
            .child(match self.mode {
                Mode::List => self.render_list(cx),
                Mode::Adding | Mode::Editing(_) => self.render_form(cx),
            })
    }
}

impl ProjectsView {
    fn render_list(&self, cx: &mut Context<Self>) -> Div {
        let projects = self.projects.clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .p(px(12.))
            .gap(px(6.))
            .children(projects.into_iter().enumerate().map(|(idx, p)| {
                let id = p.project_id.clone();
                let title = if p.title.is_empty() {
                    id.clone()
                } else {
                    p.title.clone()
                };
                div()
                    .id(SharedString::from(format!("proj-{idx}")))
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
                            .child(div().text_size(px(14.)).child(title))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(fg_dim())
                                    .child(p.description.clone().unwrap_or_default()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(chip_button(
                                SharedString::from(format!("edit-{idx}")),
                                "Edit",
                                ChipKind::Neutral,
                                move |view, _w, cx| {
                                    view.start_edit(idx, cx);
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(chip_button(
                                SharedString::from(format!("del-{idx}")),
                                "Delete",
                                ChipKind::Danger,
                                move |view, _w, cx| {
                                    view.delete_project(idx, cx);
                                    cx.notify();
                                },
                                cx,
                            )),
                    )
            }))
    }

    fn render_form(&self, cx: &mut Context<Self>) -> Div {
        let field = self.form_field;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .p(px(16.))
            .gap(px(10.))
            .child(form_field("PROJECT ID", &self.form_name, field == FormField::Name, FormField::Name, cx))
            .child(form_field("REPO", &self.form_repo, field == FormField::Repo, FormField::Repo, cx))
            .child(form_field(
                "BRANCH",
                &self.form_branch,
                field == FormField::Branch,
                FormField::Branch,
                cx,
            ))
            .child(form_field(
                "WORKTREE DIR",
                &self.form_worktree_dir,
                field == FormField::WorktreeDir,
                FormField::WorktreeDir,
                cx,
            ))
            .child(form_field(
                "MCP URL",
                &self.form_mcp_url,
                field == FormField::McpUrl,
                FormField::McpUrl,
                cx,
            ))
            .children(self.form_apps.iter().enumerate().map(|(i, input)| {
                form_field(
                    if i == 0 { "APPS" } else { "" },
                    input,
                    field == FormField::App(i),
                    FormField::App(i),
                    cx,
                )
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button(
                        "save-proj",
                        "Save",
                        ButtonKind::Primary,
                        |view, _w, cx| {
                            view.save_form(cx);
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(button(
                        "cancel-proj",
                        "Cancel",
                        ButtonKind::Secondary,
                        |view, _w, cx| {
                            view.mode = Mode::List;
                            view.form_error = None;
                            cx.notify();
                        },
                        cx,
                    )),
            )
    }
}

fn form_field(
    label: &str,
    input: &Entity<TextInput>,
    focused: bool,
    field: FormField,
    cx: &mut Context<'_, ProjectsView>,
) -> Stateful<Div> {
    let input_entity = input.clone();
    ui_helpers::render_form_field(
        label,
        input,
        focused,
        move |view: &mut ProjectsView, window, cx| {
            view.form_field = field;
            window.focus(&input_entity.read(cx).focus_handle(cx));
        },
        cx,
    )
}
