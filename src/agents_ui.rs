use crate::agent_supervisor;
use crate::log as dlog;
use crate::state::{self, AgentInstanceState, AgentStatus};
use crate::theme;
use gpui::prelude::FluentBuilder;
use gpui::*;

pub fn open_agents_window(cx: &mut App) {
    let agents = load_agents();

    let bounds = Bounds::centered(None, size(px(750.), px(500.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-agents".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| cx.new(move |cx| AgentsView::new(agents, cx)),
    )
    .ok();
}

actions!(agents_ui, [DismissAgents, RefreshAgents, StopAgent]);

fn bg() -> Rgba { theme::bg() }
fn bg_hover() -> Rgba { theme::bg_hover() }
fn bg_selected() -> Rgba { theme::bg_selected() }
fn fg() -> Rgba { theme::fg() }
fn fg_dim() -> Rgba { theme::fg_dim() }
fn accent() -> Rgba { theme::accent() }
fn border_color() -> Rgba { theme::border_color() }
fn btn_bg() -> Rgba { theme::btn_bg() }
fn btn_fg() -> Rgba { theme::btn_fg() }
fn btn_hover() -> Rgba { theme::btn_hover() }
fn danger() -> Rgba { theme::danger() }
fn success() -> Rgba { theme::success() }

#[derive(Clone)]
struct AgentRow {
    agent: AgentInstanceState,
    workspace: String,
    project: String,
    supervisor_running: bool,
}

fn load_agents() -> Vec<AgentRow> {
    let st = match state::load() {
        Ok(s) => s,
        Err(e) => {
            dlog::log(format!("Agents UI: failed to load state: {e}"));
            return Vec::new();
        }
    };

    let mut rows = Vec::new();
    for (ws_name, ws) in &st.workspaces {
        for agent in &ws.agents {
            let supervisor_running = agent_supervisor::get()
                .map(|sup| sup.is_running(&agent.id))
                .unwrap_or(false);
            rows.push(AgentRow {
                agent: agent.clone(),
                workspace: ws_name.clone(),
                project: ws.project.clone(),
                supervisor_running,
            });
        }
    }
    rows.sort_by(|a, b| {
        a.workspace.cmp(&b.workspace)
            .then(a.agent.name.cmp(&b.agent.name))
    });
    rows
}

struct AgentsView {
    agents: Vec<AgentRow>,
    focus: FocusHandle,
}

impl AgentsView {
    fn new(agents: Vec<AgentRow>, cx: &mut Context<Self>) -> Self {
        Self {
            agents,
            focus: cx.focus_handle(),
        }
    }

    fn refresh(&mut self, _: &RefreshAgents, _window: &mut Window, cx: &mut Context<Self>) {
        self.agents = load_agents();
        cx.notify();
    }

    fn on_dismiss(&mut self, _: &DismissAgents, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn stop_agent_by_id(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        dlog::log(format!("Agents UI: stopping agent {agent_id}"));
        agent_supervisor::stop_agent(agent_id);
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.agents = load_agents();
        cx.notify();
    }
}

fn status_color(status: &AgentStatus) -> Rgba {
    match status {
        AgentStatus::Ready => success(),
        AgentStatus::Busy => theme::new_badge(),
        AgentStatus::Starting => accent(),
        AgentStatus::Error => danger(),
        AgentStatus::Stopping | AgentStatus::Stopped => fg_dim(),
    }
}

fn status_label(row: &AgentRow) -> String {
    let state_str = row.agent.status.to_string();
    if row.supervisor_running && row.agent.status == AgentStatus::Ready {
        state_str
    } else if !row.supervisor_running
        && (row.agent.status == AgentStatus::Ready || row.agent.status == AgentStatus::Busy)
    {
        format!("{state_str} (no process)")
    } else {
        state_str
    }
}

impl Render for AgentsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let agents: Vec<AgentRow> = self.agents.clone();
        let ready_count = agents.iter().filter(|a| a.agent.status == AgentStatus::Ready).count();
        let busy_count = agents.iter().filter(|a| a.agent.status == AgentStatus::Busy).count();
        let total = agents.len();

        div()
            .key_context("Agents")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_dismiss))
            .on_action(cx.listener(Self::refresh))
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
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .text_color(accent())
                                    .child("Agents"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(fg_dim())
                                    .child(format!(
                                        "{total} total  ·  {ready_count} ready  ·  {busy_count} busy"
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("refresh-btn")
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
                                    view.agents = load_agents();
                                    cx.notify();
                                }),
                            )
                            .child(div().text_size(px(13.)).child("Refresh")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .id("agents-scroll")
                    .overflow_y_scroll()
                    .when(!agents.is_empty(), |this: Stateful<Div>| {
                        this.child(
                            div()
                                .px(px(20.))
                                .py(px(8.))
                                .flex()
                                .gap(px(8.))
                                .border_b_1()
                                .border_color(border_color())
                                .child(
                                    div()
                                        .w(px(140.))
                                        .text_size(px(11.))
                                        .text_color(fg_dim())
                                        .child("AGENT"),
                                )
                                .child(
                                    div()
                                        .w(px(100.))
                                        .text_size(px(11.))
                                        .text_color(fg_dim())
                                        .child("STATUS"),
                                )
                                .child(
                                    div()
                                        .w(px(120.))
                                        .text_size(px(11.))
                                        .text_color(fg_dim())
                                        .child("WORKSPACE"),
                                )
                                .child(
                                    div()
                                        .w(px(100.))
                                        .text_size(px(11.))
                                        .text_color(fg_dim())
                                        .child("TEMPLATE"),
                                )
                                .child(
                                    div()
                                        .w(px(80.))
                                        .text_size(px(11.))
                                        .text_color(fg_dim())
                                        .child("PORT"),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(11.))
                                        .text_color(fg_dim())
                                        .child(""),
                                ),
                        )
                        .children(agents.into_iter().enumerate().map(|(idx, row)| {
                            let agent_id = row.agent.id.clone();
                            let can_stop = row.agent.status == AgentStatus::Ready
                                || row.agent.status == AgentStatus::Busy
                                || row.agent.status == AgentStatus::Starting;
                            let status_clr = status_color(&row.agent.status);
                            let label = status_label(&row);

                            div()
                                .id(ElementId::Name(format!("agent-{idx}").into()))
                                .px(px(20.))
                                .py(px(10.))
                                .border_b_1()
                                .border_color(border_color())
                                .hover(|s| s.bg(bg_hover()))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .w(px(140.))
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(14.))
                                                .text_color(accent())
                                                .child(row.agent.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(fg_dim())
                                                .child(truncate_id(&row.agent.id, 20)),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(100.))
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .child(
                                            div()
                                                .size(px(8.))
                                                .rounded(px(4.))
                                                .bg(status_clr),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .text_color(status_clr)
                                                .child(label),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(120.))
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .text_color(fg())
                                                .child(row.workspace.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(fg_dim())
                                                .child(row.project.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .w(px(100.))
                                        .text_size(px(13.))
                                        .text_color(fg_dim())
                                        .child(row.agent.template.clone()),
                                )
                                .child(
                                    div()
                                        .w(px(80.))
                                        .text_size(px(13.))
                                        .text_color(fg_dim())
                                        .child(format!(":{}", row.agent.port)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .justify_end()
                                        .when(can_stop, |this: Div| {
                                            this.child(
                                                div()
                                                    .id(ElementId::Name(
                                                        format!("stop-{idx}").into(),
                                                    ))
                                                    .px(px(12.))
                                                    .py(px(4.))
                                                    .rounded(px(3.))
                                                    .bg(bg_selected())
                                                    .text_color(danger())
                                                    .text_size(px(12.))
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(danger()).text_color(btn_fg())
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            move |view, _, _, cx| {
                                                                view.stop_agent_by_id(
                                                                    &agent_id, cx,
                                                                );
                                                            },
                                                        ),
                                                    )
                                                    .child("Stop"),
                                            )
                                        }),
                                )
                        }))
                    })
                    .when(self.agents.is_empty(), |this: Stateful<Div>| {
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
                                        .child(
                                            "No agents registered. Agents appear when spawned via the ARP API or workspace configuration.",
                                        ),
                                ),
                        )
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
                            .child("Esc to close  ·  Click Refresh to reload"),
                    ),
            )
    }
}

fn truncate_id(id: &str, max: usize) -> String {
    if id.len() <= max {
        id.to_string()
    } else {
        format!("{}...", &id[..max - 3])
    }
}

impl Focusable for AgentsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::state::{AgentInstanceState, AgentStatus};

    fn make_row(status: AgentStatus, supervisor_running: bool) -> super::AgentRow {
        super::AgentRow {
            agent: AgentInstanceState {
                id: "a-1".into(),
                template: "echo".into(),
                name: "echo".into(),
                workspace: "ws".into(),
                status,
                port: 9200,
                host: None,
                pid: None,
                started_at: String::new(),
                ..Default::default()
            },
            workspace: "ws".into(),
            project: "proj".into(),
            supervisor_running,
        }
    }

    #[test]
    fn truncate_id_short() {
        assert_eq!(super::truncate_id("abc", 10), "abc");
    }

    #[test]
    fn truncate_id_exact() {
        assert_eq!(super::truncate_id("abcdefghij", 10), "abcdefghij");
    }

    #[test]
    fn truncate_id_long() {
        assert_eq!(super::truncate_id("abcdefghijk", 10), "abcdefg...");
    }

    #[test]
    fn status_label_ready_running() {
        let row = make_row(AgentStatus::Ready, true);
        assert_eq!(super::status_label(&row), "ready");
    }

    #[test]
    fn status_label_ready_no_process() {
        let row = make_row(AgentStatus::Ready, false);
        assert_eq!(super::status_label(&row), "ready (no process)");
    }

    #[test]
    fn status_label_stopped() {
        let row = make_row(AgentStatus::Stopped, false);
        assert_eq!(super::status_label(&row), "stopped");
    }
}
