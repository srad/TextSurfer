use std::borrow::Cow;
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadProgress {
    pub phase: &'static str,
    pub amount: LoadProgressAmount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadProgressAmount {
    Determinate {
        completed: usize,
        total: usize,
    },
    Items {
        completed: usize,
        total: usize,
    },
    Indeterminate {
        pulse: u8,
        elapsed: Option<Duration>,
    },
}

pub struct LoadProgressBar<'a> {
    pub progress: &'a LoadProgress,
    pub theme: &'a Theme,
}

impl Widget for LoadProgressBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (bar, value) = match self.progress.amount {
            LoadProgressAmount::Determinate { completed, total } => {
                let percent = completed.saturating_mul(100) / total.max(1);
                let filled = percent.min(100) / 10;
                (
                    format!("{}{}", "#".repeat(filled), "-".repeat(10 - filled)),
                    format!("{:>3}%", percent.min(100)),
                )
            }
            LoadProgressAmount::Items { completed, total } => {
                let filled = completed.saturating_mul(10) / total.max(1);
                (
                    format!(
                        "{}{}",
                        "#".repeat(filled.min(10)),
                        "-".repeat(10usize.saturating_sub(filled))
                    ),
                    format!("{completed}/{total}"),
                )
            }
            LoadProgressAmount::Indeterminate { pulse, elapsed } => {
                let mut bar = [b'.'; 10];
                bar[usize::from(pulse) % bar.len()] = b'#';
                let value = elapsed.map_or_else(
                    || "working".to_string(),
                    |elapsed| format!("{:.1}s", elapsed.as_secs_f32()),
                );
                (String::from_utf8_lossy(&bar).into_owned(), value)
            }
        };
        let indicator = format!("{} [{}] {}", self.progress.phase, bar, value);
        Paragraph::new(Span::styled(
            indicator,
            Style::default()
                .bg(self.theme.bar_bg)
                .fg(self.theme.bar_text),
        ))
        .alignment(ratatui::layout::Alignment::Right)
        .render(area, buf);
    }
}

use crate::ui::theme::Theme;

pub struct StatusView<'a> {
    pub url: Cow<'a, str>,
    pub message: Cow<'a, str>,
    /// The link under the pointer. It replaces the message while it lasts, the way a
    /// browser previews a target, and leaves the message untouched underneath.
    pub hover: Option<Cow<'a, str>>,
    pub progress: Option<LoadProgress>,
}

pub struct StatusBar<'a> {
    pub view: &'a StatusView<'a>,
    pub theme: &'a Theme,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bar = Style::default()
            .bg(self.theme.bar_bg)
            .fg(self.theme.bar_text);
        if area.width < 2 {
            return;
        }
        buf.set_style(area, bar);
        let inner = usize::from(area.width);
        let message = self.view.hover.as_ref().unwrap_or(&self.view.message);
        if let Some(progress) = self.view.progress {
            Paragraph::new(Span::styled(message.as_ref(), bar)).render(area, buf);
            LoadProgressBar {
                progress: &progress,
                theme: self.theme,
            }
            .render(area, buf);
            return;
        }
        let url = &self.view.url;
        let message_width = unicode_width::UnicodeWidthStr::width(message.as_ref());
        let url_width = unicode_width::UnicodeWidthStr::width(url.as_ref());
        let pad = inner.saturating_sub(message_width + 1 + url_width);
        let line = Line::from(vec![
            Span::styled(format!("{message}{}", " ".repeat(pad)), bar),
            Span::styled(url.as_ref(), bar),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(url: &str, message: &str) -> String {
        render_view(url, message, None)
    }

    fn render_view(url: &str, message: &str, hover: Option<&str>) -> String {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let view = StatusView {
            url: url.to_string().into(),
            message: message.to_string().into(),
            hover: hover.map(|hover| hover.to_string().into()),
            progress: None,
        };
        terminal
            .draw(|frame| {
                StatusBar {
                    view: &view,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn message_left_url_right() {
        insta::assert_snapshot!(render("https://example.com", "Loaded"));
    }

    #[test]
    fn a_hovered_link_previews_in_place_of_the_message() {
        insta::assert_snapshot!(render_view(
            "https://example.com",
            "Loaded",
            Some("https://a.example/x")
        ));
    }

    #[test]
    fn long_message_squeezes_the_url_off_edge() {
        insta::assert_snapshot!(render(
            "https://example.com",
            "this message is much longer than the status row can possibly hold"
        ));
    }

    #[test]
    fn progress_occupies_the_bottom_right_edge() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let view = StatusView {
            url: "https://example.com".into(),
            message: "Loading".into(),
            hover: None,
            progress: Some(LoadProgress {
                phase: "parse",
                amount: LoadProgressAmount::Determinate {
                    completed: 42,
                    total: 100,
                },
            }),
        };
        terminal
            .draw(|frame| {
                StatusBar {
                    view: &view,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        assert!(
            crate::ui::test_util::buffer_string(terminal.backend().buffer())
                .trim_end()
                .ends_with("parse [####------]  42%")
        );
    }
}
