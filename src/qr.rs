use crate::auth;
use crate::theme;
use crate::ui_helpers;
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

    let opts = ui_helpers::centered_window_options(cx, "awesometree-qr", 420., 480.);
    cx.open_window(
        opts,
        move |_window, cx| {
            let token_for_clipboard = data.clone();
            cx.new(move |cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(token_for_clipboard));
                QrView {
                    matrix,
                    token: data,
                    server_info: format!("{ip}:9099"),
                    focus: cx.focus_handle(),
                }
            })
        },
    )
    .ok();
}

struct QrView {
    matrix: Vec<Vec<bool>>,
    token: String,
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
                                    .child("Scan QR or paste token (copied to clipboard)"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::fg())
                                    .child(format!("Server: {}", self.server_info)),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme::fg_dim())
                                    .child(format!("Token: {}", self.token)),
                            ),
                    )
                    .child(ui_helpers::button(
                        "close-qr",
                        "Close",
                        ui_helpers::ButtonKind::Primary,
                        |_view, window, _cx| {
                            window.remove_window();
                        },
                        cx,
                    )),
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
