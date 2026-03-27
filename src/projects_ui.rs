use crate::interop::{self, Project};
use crate::log as dlog;
use crate::text_input::TextInput;
use crate::theme;
use crate::ui_helpers;
use gpui::prelude::FluentBuilder;
use gpui::*;

pub fn open_projects_window(cx: &mut App) {
    let projects = interop::list().unwrap_or_default();

    crate::text_input::bind_text_input_keys(cx);

    let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-projects".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| cx.new(move |cx| ProjectsView::new(projects, cx)),
    )
    .ok();
}

pub fn run_projects_ui() {
    let projects = interop::list().unwrap_or_default();

    let app = Application::new();
    app.run(move |cx: &mut App| {
        crate::text_input::bind_text_input_keys(cx);
        cx.bind_keys([
            KeyBinding::new("escape", Dismiss, None),
            KeyBinding::new("enter", ConfirmAction, None),
            KeyBinding::new("tab", NextField, None),
            KeyBinding::new("shift-tab", PrevField, None),
        ]);

        let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                app_id: Some("awesometree-projects".into()),
                window_decorations: Some(WindowDecorations::Server),
                ..Default::default()
            },
            move |_window, cx| cx.new(move |cx| ProjectsView::new(projects, cx)),
        )
        .ok();
    });
}

actions!(projects_ui, [Dismiss, ConfirmAction, NextField, PrevField]);

fn bg() -> Rgba { theme::bg() }
fn bg_hover() -> Rgba { theme::bg_hover() }
fn bg_selected() -> Rgba { theme::bg_selected() }
fn fg() -> Rgba { theme::fg() }
fn fg_dim() -> Rgba { theme::fg_dim() }
fn accent() -> Rgba { theme::accent() }
fn border_color() -> Rgba { theme::border_color() }
fn border_focus() -> Rgba { theme::border_focus() }
fn btn_bg() -> Rgba { theme::btn_bg() }
fn btn_fg() -> Rgba { theme::btn_fg() }
fn btn_hover() -> Rgba { theme::btn_hover() }
fn danger() -> Rgba { theme::danger() }
fn success() -> Rgba { theme::success() }

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
    LaunchPrompt,
    McpUrl,
    AcpEnabled,
    App(usize),
}

struct ProjectsView {
    projects: Vec<Project>,
    mode: Mode,
    form_name: Entity<TextInput>,
    form_repo: Entity<TextInput>,
    form_branch: Entity<TextInput>,
    form_worktree_dir: Entity<TextInput>,
    form_launch_prompt: Entity<TextInput>,
    form_mcp_url: Entity<TextInput>,
    form_acp_enabled: bool,
    form_apps: Vec<Entity<TextInput>>,
    form_field: FormField,
    focus: FocusHandle,
}

impl ProjectsView {
    fn new(projects: Vec<Project>, cx: &mut Context<Self>) -> Self {
        Self {
            projects,
            mode: Mode::List,
            form_name: cx.new(|cx| TextInput::new("e.g. curri", cx)),
            form_repo: cx.new(|cx| TextInput::new("/path/to/git/repo", cx)),
            form_branch: cx.new(|cx| TextInput::new("master", cx)),
            form_worktree_dir: cx.new(|cx| TextInput::new("~/worktrees/project-name (default)", cx)),
            form_launch_prompt: cx.new(|cx| TextInput::new("system prompt for agent", cx)),
            form_mcp_url: cx.new(|cx| TextInput::new("http://localhost:8080/{project}", cx)),
            form_acp_enabled: false,
            form_apps: vec![],
            form_field: FormField::Name,
            focus: cx.focus_handle(),
        }
    }

    fn active_field_entity(&self) -> Option<&Entity<TextInput>> {
        match self.form_field {
            FormField::Name => Some(&self.form_name),
            FormField::Repo => Some(&self.form_repo),
            FormField::Branch => Some(&self.form_branch),
            FormField::WorktreeDir => Some(&self.form_worktree_dir),
            FormField::LaunchPrompt => Some(&self.form_launch_prompt),
            FormField::McpUrl => Some(&self.form_mcp_url),
            FormField::AcpEnabled => None,
            FormField::App(i) => self.form_apps.get(i),
        }
    }

    fn focus_active_field(&self, window: &mut Window, cx: &App) {
        if let Some(entity) = self.active_field_entity() {
            window.focus(&entity.read(cx).focus_handle(cx));
        }
    }

    fn read_field(&self, field: FormField, cx: &App) -> String {
        match field {
            FormField::Name => self.form_name.read(cx).value().to_string(),
            FormField::Repo => self.form_repo.read(cx).value().to_string(),
            FormField::Branch => self.form_branch.read(cx).value().to_string(),
            FormField::WorktreeDir => self.form_worktree_dir.read(cx).value().to_string(),
            FormField::LaunchPrompt => self.form_launch_prompt.read(cx).value().to_string(),
            FormField::McpUrl => self.form_mcp_url.read(cx).value().to_string(),
            FormField::AcpEnabled => String::new(),
            FormField::App(i) => self
                .form_apps
                .get(i)
                .map(|e| e.read(cx).value().to_string())
                .unwrap_or_default(),
        }
    }

    fn clear_form(&mut self, cx: &mut Context<Self>) {
        self.form_name.update(cx, |input, cx| input.clear(cx));
        self.form_repo.update(cx, |input, cx| input.clear(cx));
        self.form_branch.update(cx, |input, cx| input.clear(cx));
        self.form_worktree_dir.update(cx, |input, cx| input.clear(cx));
        self.form_launch_prompt.update(cx, |input, cx| input.clear(cx));
        self.form_mcp_url.update(cx, |input, cx| input.clear(cx));
        self.form_acp_enabled = false;
        self.form_apps.clear();
        self.form_field = FormField::Name;
    }

    fn start_add(&mut self, cx: &mut Context<Self>) {
        self.clear_form(cx);
        self.form_field = FormField::Name;
        self.mode = Mode::Adding;
    }

    fn start_edit(&mut self, idx: usize, cx: &mut Context<Self>) {
        let p = &self.projects[idx];
        self.form_name.update(cx, |input, icx| input.set_value(&p.name, icx));
        self.form_repo.update(cx, |input, icx| {
            input.set_value(p.repo.as_deref().unwrap_or(""), icx);
        });
        self.form_branch.update(cx, |input, icx| {
            input.set_value(p.branch.as_deref().unwrap_or(""), icx);
        });
        self.form_launch_prompt.update(cx, |input, icx| {
            input.set_value(
                p.launch
                    .as_ref()
                    .and_then(|l| l.prompt.as_deref())
                    .unwrap_or(""),
                icx,
            );
        });
        let ext = p.awesometree_ext();
        self.form_worktree_dir.update(cx, |input, icx| {
            input.set_value(ext.worktree_dir.as_deref().unwrap_or(""), icx);
        });
        self.form_mcp_url.update(cx, |input, icx| {
            input.set_value(ext.mcp.as_deref().unwrap_or(""), icx);
        });
        self.form_acp_enabled = ext.acp.as_ref().map(|a| a.enabled).unwrap_or(false);
        self.form_apps = ext
            .apps
            .iter()
            .map(|app| cx.new(|cx| {
                let mut input = TextInput::new("e.g. zeditor -n {dir}", cx);
                input.set_value(app, cx);
                input
            }))
            .collect();
        self.form_field = FormField::Name;
        self.mode = Mode::Editing(idx);
    }

    fn save_form(&mut self, cx: &mut Context<Self>) {
        let name = self.read_field(FormField::Name, cx);
        let repo = self.read_field(FormField::Repo, cx);
        let branch_val = self.read_field(FormField::Branch, cx);
        let worktree_dir = self.read_field(FormField::WorktreeDir, cx);
        let launch_prompt = self.read_field(FormField::LaunchPrompt, cx);
        let mcp_url = self.read_field(FormField::McpUrl, cx);
        let apps = self.read_apps(cx);

        match self.mode {
            Mode::Adding => {
                if name.is_empty() || repo.is_empty() {
                    return;
                }
                dlog::log(format!("Adding project: {name}"));
                let branch = if branch_val.is_empty() {
                    "master".to_string()
                } else {
                    branch_val
                };
                let mut proj = Project::new(&name, &repo, &branch);
                if !launch_prompt.is_empty() {
                    proj.launch = Some(interop::Launch {
                        prompt: Some(launch_prompt),
                        ..Default::default()
                    });
                }
                {
                    let mut ext = proj.awesometree_ext();
                    if !mcp_url.is_empty() {
                        ext.mcp = Some(mcp_url);
                    }
                    if !worktree_dir.is_empty() {
                        ext.worktree_dir = Some(worktree_dir);
                    }
                    if self.form_acp_enabled {
                        ext.acp = Some(interop::AcpConfig {
                            enabled: true,
                            ..ext.acp.unwrap_or_default()
                        });
                    } else {
                        ext.acp = None;
                    }
                    ext.apps = clean_apps(&apps);
                    proj.set_awesometree_ext(&ext);
                }
                let _ = interop::save(&proj);
                self.projects.push(proj);
                self.mode = Mode::List;
                self.clear_form(cx);
            }
            Mode::Editing(idx) => {
                if name.is_empty() || repo.is_empty() {
                    return;
                }
                dlog::log(format!("Editing project: {name}"));
                let p = &mut self.projects[idx];
                p.name = name;
                p.repo = Some(repo);
                p.branch = Some(if branch_val.is_empty() {
                    "master".to_string()
                } else {
                    branch_val
                });
                if launch_prompt.is_empty() {
                    if let Some(launch) = &mut p.launch {
                        launch.prompt = None;
                    }
                } else {
                    let launch = p.launch.get_or_insert_with(interop::Launch::default);
                    launch.prompt = Some(launch_prompt);
                }
                let mut ext = p.awesometree_ext();
                if mcp_url.is_empty() {
                    ext.mcp = None;
                } else {
                    ext.mcp = Some(mcp_url);
                }
                if worktree_dir.is_empty() {
                    ext.worktree_dir = None;
                } else {
                    ext.worktree_dir = Some(worktree_dir);
                }
                ext.apps = clean_apps(&apps);
                if self.form_acp_enabled {
                    ext.acp = Some(interop::AcpConfig {
                        enabled: true,
                        ..ext.acp.unwrap_or_default()
                    });
                } else {
                    ext.acp = None;
                }
                p.set_awesometree_ext(&ext);
                let _ = interop::save(p);
                self.mode = Mode::List;
                self.clear_form(cx);
            }
            Mode::List => {}
        }
    }

    fn read_apps(&self, cx: &App) -> Vec<String> {
        self.form_apps
            .iter()
            .map(|e| e.read(cx).value().to_string())
            .collect()
    }

    fn delete_project(&mut self, idx: usize, cx: &mut Context<Self>) {
        let name = self.projects[idx].name.clone();
        dlog::log(format!("Deleting project: {name}"));
        let _ = interop::delete(&name);
        self.projects.remove(idx);
        if let Mode::Editing(ei) = self.mode {
            if ei == idx {
                self.mode = Mode::List;
                self.clear_form(cx);
            }
        }
    }

    fn add_app_row(&mut self, cx: &mut Context<Self>) {
        let new_input = cx.new(|cx| TextInput::new("e.g. zeditor -n {dir}", cx));
        self.form_apps.push(new_input);
        self.form_field = FormField::App(self.form_apps.len() - 1);
    }

    fn remove_app_row(&mut self, idx: usize) {
        if idx < self.form_apps.len() {
            self.form_apps.remove(idx);
        }
        if self.form_apps.is_empty() {
            self.form_field = FormField::AcpEnabled;
        } else {
            let new_idx = idx.min(self.form_apps.len().saturating_sub(1));
            self.form_field = FormField::App(new_idx);
        }
    }

    fn on_dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        match self.mode {
            Mode::List => window.remove_window(),
            _ => {
                self.mode = Mode::List;
                self.clear_form(cx);
                cx.notify();
            }
        }
    }

    fn on_confirm(&mut self, _: &ConfirmAction, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::List {
            self.save_form(cx);
            cx.notify();
        }
    }

    fn next_field(&self) -> FormField {
        match self.form_field {
            FormField::Name => FormField::Repo,
            FormField::Repo => FormField::Branch,
            FormField::Branch => FormField::WorktreeDir,
            FormField::WorktreeDir => FormField::LaunchPrompt,
            FormField::LaunchPrompt => FormField::McpUrl,
            FormField::McpUrl => FormField::AcpEnabled,
            FormField::AcpEnabled => {
                if self.form_apps.is_empty() {
                    FormField::Name
                } else {
                    FormField::App(0)
                }
            }
            FormField::App(i) => {
                if i + 1 < self.form_apps.len() {
                    FormField::App(i + 1)
                } else {
                    FormField::Name
                }
            }
        }
    }

    fn prev_field(&self) -> FormField {
        match self.form_field {
            FormField::Name => {
                if self.form_apps.is_empty() {
                    FormField::AcpEnabled
                } else {
                    FormField::App(self.form_apps.len() - 1)
                }
            }
            FormField::Repo => FormField::Name,
            FormField::Branch => FormField::Repo,
            FormField::WorktreeDir => FormField::Branch,
            FormField::LaunchPrompt => FormField::WorktreeDir,
            FormField::McpUrl => FormField::LaunchPrompt,
            FormField::AcpEnabled => FormField::McpUrl,
            FormField::App(i) => {
                if i > 0 {
                    FormField::App(i - 1)
                } else {
                    FormField::AcpEnabled
                }
            }
        }
    }

    fn on_next_field(&mut self, _: &NextField, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::List {
            self.form_field = self.next_field();
            self.focus_active_field(window, cx);
            cx.notify();
        }
    }

    fn on_prev_field(&mut self, _: &PrevField, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::List {
            self.form_field = self.prev_field();
            self.focus_active_field(window, cx);
            cx.notify();
        }
    }
}

fn clean_apps(apps: &[String]) -> Vec<String> {
    apps.iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn render_field(
    label: &str,
    input: &Entity<TextInput>,
    focused: bool,
    field: FormField,
    cx: &mut Context<'_, ProjectsView>,
) -> Stateful<Div> {
    let input_entity = input.clone();
    ui_helpers::render_form_field(label, input, focused, move |view: &mut ProjectsView, window, cx| {
        view.form_field = field;
        window.focus(&input_entity.read(cx).focus_handle(cx));
    }, cx)
}

fn render_apps_section(
    apps: &[Entity<TextInput>],
    field: FormField,
    cx: &mut Context<'_, ProjectsView>,
) -> Div {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(6.));

    section = section.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(
                        if matches!(field, FormField::App(_)) { accent() } else { fg_dim() },
                    )
                    .child("APPS ({dir} = worktree path)"),
            )
            .child(
                div()
                    .id("add-app-btn")
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(3.))
                    .bg(bg_selected())
                    .text_color(accent())
                    .text_size(px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(btn_bg()).text_color(btn_fg()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, window, cx| {
                            view.add_app_row(cx);
                            view.focus_active_field(window, cx);
                            cx.notify();
                        }),
                    )
                    .child("+ Add"),
            ),
    );

    for (i, app_input) in apps.iter().enumerate() {
        let focused = field == FormField::App(i);
        let input_entity = app_input.clone();

        section = section.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .id(ElementId::Name(format!("app-field-{i}").into()))
                        .flex_1()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(if focused { border_focus() } else { border_color() })
                        .bg(bg_hover())
                        .cursor(CursorStyle::IBeam)
                        .text_size(px(14.))
                        .text_color(fg())
                        .font_family("monospace")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _, window, cx| {
                                view.form_field = FormField::App(i);
                                window.focus(&input_entity.read(cx).focus_handle(cx));
                                cx.notify();
                            }),
                        )
                        .child(app_input.clone()),
                )
                .child(
                    div()
                        .id(ElementId::Name(format!("rm-app-{i}").into()))
                        .px(px(6.))
                        .py(px(4.))
                        .rounded(px(3.))
                        .bg(bg_selected())
                        .text_color(danger())
                        .text_size(px(14.))
                        .cursor_pointer()
                        .hover(|s| s.bg(danger()).text_color(btn_fg()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _, _, cx| {
                                view.remove_app_row(i);
                                cx.notify();
                            }),
                        )
                        .child("×"),
                ),
        );
    }

    if apps.is_empty() {
        section = section.child(
            div()
                .text_size(px(12.))
                .text_color(fg_dim())
                .child("No apps configured (defaults to zeditor)"),
        );
    }

    section
}

impl Render for ProjectsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let projects: Vec<(usize, Project)> = self
            .projects
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.clone()))
            .collect();

        let mode = self.mode;
        let form_name = self.read_field(FormField::Name, cx);
        let form_repo = self.read_field(FormField::Repo, cx);
        let field = self.form_field;
        let can_save = !form_name.is_empty() && !form_repo.is_empty();

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
                    .px(px(20.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(border_color())
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(18.))
                            .text_color(accent())
                            .child("Projects"),
                    )
                    .when(mode == Mode::List, |this: Div| {
                        this.child(
                            div()
                                .id("add-btn")
                                .px(px(16.))
                                .py(px(6.))
                                .rounded(px(4.))
                                .bg(btn_bg())
                                .text_color(btn_fg())
                                .cursor_pointer()
                                .hover(|s| s.bg(btn_hover()))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, _, _, cx| {
                                        view.start_add(cx);
                                        cx.notify();
                                    }),
                                )
                                .child(div().text_size(px(13.)).child("+ Add Project")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_hidden()
                    .when(mode != Mode::List, |this: Div| {
                        let title = match mode {
                            Mode::Adding => "New Project",
                            Mode::Editing(_) => "Edit Project",
                            _ => "",
                        };
                        this.child(
                            div()
                                .px(px(20.))
                                .py(px(16.))
                                .flex()
                                .flex_col()
                                .gap(px(14.))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .text_color(fg())
                                        .child(title.to_string()),
                                )
                                .child(render_field(
                                    "NAME",
                                    &self.form_name,
                                    field == FormField::Name,
                                    FormField::Name,
                                    cx,
                                ))
                                .child(render_field(
                                    "REPO PATH",
                                    &self.form_repo,
                                    field == FormField::Repo,
                                    FormField::Repo,
                                    cx,
                                ))
                                .child(render_field(
                                    "SOURCE BRANCH",
                                    &self.form_branch,
                                    field == FormField::Branch,
                                    FormField::Branch,
                                    cx,
                                ))
                                .child(render_field(
                                    "WORKTREE DIR",
                                    &self.form_worktree_dir,
                                    field == FormField::WorktreeDir,
                                    FormField::WorktreeDir,
                                    cx,
                                ))
                                .child(render_field(
                                    "LAUNCH PROMPT",
                                    &self.form_launch_prompt,
                                    field == FormField::LaunchPrompt,
                                    FormField::LaunchPrompt,
                                    cx,
                                ))
                                .child(render_field(
                                    "MCP URL",
                                    &self.form_mcp_url,
                                    field == FormField::McpUrl,
                                    FormField::McpUrl,
                                    cx,
                                ))
                                .child({
                                    let acp_on = self.form_acp_enabled;
                                    let focused = field == FormField::AcpEnabled;
                                    div()
                                        .id("acp-toggle")
                                        .flex()
                                        .items_center()
                                        .gap(px(10.))
                                        .px(px(10.))
                                        .py(px(6.))
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(if focused { border_focus() } else { border_color() })
                                        .bg(bg_hover())
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _, _, cx| {
                                                view.form_acp_enabled = !view.form_acp_enabled;
                                                view.form_field = FormField::AcpEnabled;
                                                cx.notify();
                                            }),
                                        )
                                        .child(
                                            div()
                                                .size(px(16.))
                                                .rounded(px(3.))
                                                .border_1()
                                                .border_color(if acp_on { accent() } else { fg_dim() })
                                                .when(acp_on, |s: Div| s.bg(accent()))
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(if focused { accent() } else { fg_dim() })
                                                .child("ACP AGENT (crush serve)")
                                        )
                                })
                                .child(render_apps_section(&self.form_apps, field, cx))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(10.))
                                        .child(
                                            div()
                                                .id("save-btn")
                                                .px(px(20.))
                                                .py(px(6.))
                                                .rounded(px(4.))
                                                .cursor_pointer()
                                                .when(can_save, |s: Stateful<Div>| {
                                                    s.bg(success())
                                                        .text_color(btn_fg())
                                                        .hover(|s| s.bg(btn_hover()))
                                                })
                                                .when(!can_save, |s: Stateful<Div>| {
                                                    s.bg(bg_selected())
                                                        .text_color(fg_dim())
                                                })
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|view, _, _, cx| {
                                                        view.save_form(cx);
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(13.)).child("Save")),
                                        )
                                        .child(
                                            div()
                                                .id("cancel-btn")
                                                .px(px(20.))
                                                .py(px(6.))
                                                .rounded(px(4.))
                                                .bg(bg_selected())
                                                .text_color(fg())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(bg_hover()))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|view, _, _, cx| {
                                                        view.mode = Mode::List;
                                                        view.clear_form(cx);
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(13.)).child("Cancel")),
                                        ),
                                ),
                        )
                    })
                    .when(mode == Mode::List, |this: Div| {
                        this.children(projects.into_iter().map(|(idx, proj)| {
                            let ext = proj.awesometree_ext();
                            let mcp_label = ext.mcp.as_deref().unwrap_or("");
                            let worktree_label = ext.worktree_dir.as_deref().unwrap_or("");
                            let apps_count = ext.apps.len();
                            let acp_enabled = ext.acp.as_ref().map(|a| a.enabled).unwrap_or(false);

                            div()
                                .id(ElementId::Name(format!("proj-{idx}").into()))
                                .px(px(20.))
                                .py(px(12.))
                                .border_b_1()
                                .border_color(border_color())
                                .hover(|s| s.bg(bg_hover()))
                                .flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.))
                                        .child(
                                            div()
                                                .text_size(px(15.))
                                                .text_color(accent())
                                                .child(proj.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(fg_dim())
                                                .child(format!(
                                                    "{}  ·  branch: {}",
                                                    proj.repo.as_deref().unwrap_or(""),
                                                    proj.branch_or_default()
                                                )),
                                        )
                                        .when(!worktree_label.is_empty(), |s: Div| {
                                            s.child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(fg_dim())
                                                    .child(format!("worktrees: {worktree_label}")),
                                            )
                                        })
                                        .when(!mcp_label.is_empty(), |s: Div| {
                                            s.child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(fg_dim())
                                                    .child(format!("mcp: {mcp_label}")),
                                            )
                                        })
                                        .when(apps_count > 0, |s: Div| {
                                            s.child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(fg_dim())
                                                    .child(format!(
                                                        "{apps_count} app{}",
                                                        if apps_count == 1 { "" } else { "s" }
                                                    )),
                                            )
                                        })
                                        .when(acp_enabled, |s: Div| {
                                            s.child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(success())
                                                    .child("acp: enabled"),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.))
                                        .child(
                                            div()
                                                .id(ElementId::Name(format!("edit-{idx}").into()))
                                                .px(px(12.))
                                                .py(px(4.))
                                                .rounded(px(3.))
                                                .bg(bg_selected())
                                                .text_color(fg())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(btn_bg()).text_color(btn_fg()))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |view, _, _, cx| {
                                                        view.start_edit(idx, cx);
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(12.)).child("Edit")),
                                        )
                                        .child(
                                            div()
                                                .id(ElementId::Name(format!("del-{idx}").into()))
                                                .px(px(12.))
                                                .py(px(4.))
                                                .rounded(px(3.))
                                                .bg(bg_selected())
                                                .text_color(danger())
                                                .cursor_pointer()
                                                .hover(|s| s.bg(danger()).text_color(btn_fg()))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |view, _, _, cx| {
                                                        view.delete_project(idx, cx);
                                                        cx.notify();
                                                    }),
                                                )
                                                .child(div().text_size(px(12.)).child("Delete")),
                                        ),
                                )
                        }))
                        .when(self.projects.is_empty(), |this: Div| {
                            this.child(
                                div()
                                    .px(px(20.))
                                    .py(px(40.))
                                    .flex()
                                    .justify_center()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .text_color(fg_dim())
                                            .child("No projects configured. Click \"+ Add Project\" to get started."),
                                    ),
                            )
                        })
                    }),
            )
            .child(
                div()
                    .px(px(20.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(border_color())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(fg_dim())
                            .child("Esc to close  ·  Tab to switch fields  ·  Enter to save"),
                    ),
            )
    }
}

impl Focusable for ProjectsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
