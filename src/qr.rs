use crate::auth;
use crate::theme;
use gpui::*;
use qrcode::QrCode;

pub fn qr_data() -> String {
    auth::token_only()
}

pub fn qr_matrix(data: &str) -> Vec<Vec<bool>> {
    let code = QrCode::new(data.as_bytes()).expect("QR encode");
    let matrix = code.to_colors();
    let width = code.width();
    matrix
        .chunks(width)
        .map(|row| row.iter().map(|c| *c == qrcode::Color::Dark).collect())
        .collect()
}

actions!(qr, [DismissQr]);

pub fn show_qr_window(cx: &mut App) {
    let data = qr_data();
    let matrix = qr_matrix(&data);
    let ip = auth::get_local_ip();

    let bounds = Bounds::centered(None, size(px(420.), px(480.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            app_id: Some("awesometree-qr".into()),
            window_decorations: Some(WindowDecorations::Server),
            ..Default::default()
        },
        move |_window, cx| {
            cx.new(move |cx| QrView {
                matrix,
                server_info: format!("{ip}:9099"),
                focus: cx.focus_handle(),
            })
        },
    )
    .ok();
}

struct QrView {
    matrix: Vec<Vec<bool>>,
    server_info: String,
    focus: FocusHandle,
}

impl QrView {
    fn on_dismiss(&mut self, _: &DismissQr, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }
}

impl Render for QrView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cell_size = 4.0_f32;
        let rows = self.matrix.len();
        let cols = if rows > 0 { self.matrix[0].len() } else { 0 };
        let qr_width = cols as f32 * cell_size;

        let mut qr_rows: Vec<Div> = Vec::new();
        for row in &self.matrix {
            let mut cells: Vec<Div> = Vec::new();
            for &dark in row {
                let color = if dark {
                    gpui::rgba(0x000000ff)
                } else {
                    gpui::rgba(0xffffffff)
                };
                cells.push(
                    div()
                        .size(px(cell_size))
                        .bg(color),
                );
            }
            qr_rows.push(div().flex().flex_row().children(cells));
        }

        div()
            .key_context("QR")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_dismiss))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::fg())
            .font_family("monospace")
            .child(
                div()
                    .px(px(20.))
                    .py(px(14.))
                    .border_b_1()
                    .border_color(theme::border_color())
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(theme::accent())
                            .child("Mobile Connection"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        div()
                            .p(px(8.))
                            .bg(gpui::rgba(0xffffffff))
                            .rounded(px(8.))
                            .size(px(qr_width + 16.))
                            .child(
                                div()
                                    .size(px(qr_width))
                                    .overflow_hidden()
                                    .children(qr_rows),
                            ),
                    ),
            )
            .child(
                div()
                    .px(px(20.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(theme::border_color())
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::fg_dim())
                                    .child("Scan to authenticate the mobile app"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::fg())
                                    .child(format!("Server: {}", self.server_info)),
                            ),
                    )
                    .child(
                        div()
                            .id("close-qr")
                            .px(px(24.))
                            .py(px(6.))
                            .rounded(px(4.))
                            .bg(theme::btn_bg())
                            .text_color(theme::btn_fg())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::btn_hover()))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_view, _, window, _cx| {
                                    window.remove_window();
                                }),
                            )
                            .child(div().text_size(px(13.)).child("Close")),
                    ),
            )
    }
}

impl Focusable for QrView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::auth;
    use crate::qr::{qr_data, qr_matrix};

    #[test]
    fn qr_data_is_valid_token() {
        let data = qr_data();
        assert!(auth::validate_token(&data));
    }

    #[test]
    fn qr_matrix_is_square() {
        let data = qr_data();
        let matrix = qr_matrix(&data);
        assert!(!matrix.is_empty());
        let rows = matrix.len();
        for row in &matrix {
            assert_eq!(row.len(), rows);
        }
    }

    #[test]
    fn qr_matrix_has_dark_cells() {
        let data = qr_data();
        let matrix = qr_matrix(&data);
        let dark_count: usize = matrix.iter().flat_map(|r| r.iter()).filter(|&&c| c).count();
        assert!(dark_count > 0);
    }
}
