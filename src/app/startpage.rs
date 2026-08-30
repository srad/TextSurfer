use crate::core::geom::Size;
use crate::core::style::{CellStyle, Rgb};
use crate::paint::{DisplayList, PaintedRow, PaintedSpan};

const ART_WIDTH: usize = 78;
const ART_HEIGHT: usize = 36;

#[derive(Clone, Copy)]
struct WaveGlyph {
    character: char,
    foreground: u8,
    background: u8,
}

const START_PAGE_PIXELS: [&str; 36] = [
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccGGGccccccGGGccc",
    "ccWWWWWcWWWWWcWcccWcWWWWWccWWWWcWcccWcWWWWccWWWWWcWWWWWcWWWWcccGGGGGcccGGGGccc",
    "cccmYmmmYmmmmmYmccYmcmYmmmWcmmmmYmccYmYmmmWcYmmmmmYmmmmmYmmmWGGGGGGGGccGGGGccc",
    "ccccYmccYmcccccWcWcmccYmccYmccccYmccYmYmccYmYmccccYmccccYmcGYmGGGGGGGGGGgGcGGG",
    "ccccYmccYYYYccccWcmcccYmcccYYYccYmccYmYYYYcmYYYYccYYYYccYYYYGmGGGGGGgGGGGGGGGG",
    "ccccYmccYmmmmccYcYccccYmccccmmYcYmccYmYmYmmcYmmmmcYmmmmcYmYmmGGGGGgGGGGGGGgGGG",
    "ccccYmccYmccccYcmcYcccYmccccccYmYmccYmYmcYccYmccccYmccccYmcYcccccGGGGGGGGGGGGc",
    "ccccYmccYYYYYcYmccYmccYmccYYYYcmcWWWcmYmccYcYmccccYYYYYcYmccYccGGGGGGMGGGGGGGc",
    "cccccmcccmmmmmcmcccmcccmcccmmmmcccmmmccmcccmcmCCcccmmmmmcmccGmGGGgGGGGGGMGgGGG",
    "ccccccccccccccccccccccccccccccccccccccccccccCCccccccYYYYYYGGGGGGGGGGccoooGGGGG",
    "cccccccccccCCCccccccccccccccccccccccccccccWWWWWWWWcYYYYYYYGGGGGGGcccccRooccGGG",
    "cccccccccCCcccWWWWWWWWWWWWWWccccccccccccccccccccccYYYYYYYYGGGGccccccccoooccccc",
    "ccccccWWWWWWWWWWWWWWCWWCWCWWCWWcccccccccccccccccccYYYYYYYYYYYcccccccccoooccccc",
    "cccccccWWWWWWWCCCCCCCCCCCCCWWWWCWWcWcccccccccccccMMMMMMMMMMMMMcccccccooooccccc",
    "cccccWWWWWCCCCCCCCCCCCCCCCCCCCWWWCWWcccccccccccccYYYYYYYYYYYYYcccccccRoooccccc",
    "ccccWWWWCCCCCCCCCCCCCCCCCCCCCCCCCWWWWcccccccccccccYYYYYYYYYYYccccccccooocccccc",
    "cccWWWWCCCCCCCCcccccccccccCCCCCCCCooWWCccccccccccMMMMMMMMMMMMMcccccccooocccccc",
    "ccWWWCCCCCCCcccccccccccccccccCCCCCYYYWWcWcccccccccYYYYYYYYYYYcccccccoooocccccc",
    "cWWWCCCCCCccccccBBBBBBBBBccccccCCCYYYWWCcccccccccccYYYYYYYYYccccccccoRoocccccc",
    "WWWWCCCCCccccBCCCCCCCCCCCCCCccccCCYYYYWWWcccccccccRcRYRYRYRcRcccccccoooccccccc",
    "CCCCCCCCCCCCCCcccccccccccccBBccccCYYYYYYWccccccccccccccYccccccccccccoooccccccc",
    "WWWCCCCccccBBcccWcccccccWcccBBYYYYCYYYYWYYYYccccWcccccccWcccccccWccoooocWccccc",
    "WWCCCCCcCCBCWCccccccccccccccWCBCCcCYYYYWWWCCWCCCCCCCWCCCCCCCWCCCCCCoRooCCCCCWY",
    "ccccccccccccccbbbBbbbbBbbbbBbbBBBccYYYYYYWBbbbbBbbbbBbbbbBbbbbBbbbboooYYYYYYYY",
    "WWCCCbcbbBbbbbbbbbbbbbbbbbbbbbbBbbYYYYYYYYYbbbbbbbbbbbbbbbbbbbbYYYYoooYYYYYYYY",
    "WWCCCbcbbBbbbbCCCCCCCCCCCCCCbbbBbYYYYYYYYYYYbbbbbbbbbbbbbbWoYoYYYYooooYYYoYoYY",
    "CCCCCCCCCCCCCCbBbbbbBCbbbBbbbbBCYYYYYCCYYYYWbBbbbbBCbbbBbWYYYYoYoYoRooYoYYYYoY",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbMMMMMMMMMMMMMMMMMMMMMbbbbbbbbWYoYoYYYYooooYYYoYoYYY",
    "bbbbbbbbbbbbbbcccccccccccccYMMMMCMMMCMMMCMMMMMMYbbbbbbWWoYYYYoYoYYoooYoYYYYoYo",
    "cccccccCcccccCbbbBbCbbBbbCbMMMMMMMMMMMMMMMMMMMMMbbbbBWYYYoYoYYYYoYoYYYYoYoYYYY",
    "bbbbCbbbbbCbbbbbCbbbbbCbbbbbCbbbbbbbbbbbbbbbbbbbbbbbWMWoYYYYoYoYYMWoYoYYYYoYoM",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbWYRYYoYoYYYYoYoYYYYoYoYYYYR",
    "BCbbbBbbbbBCbbbBbbbbBCbbbBbbbbBCbbbBbbbbBCbbbBbbbbWYoYoYYMWoYoYYYYoYoMWYYoYoYY",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbWWoYYYYoYoYYYYoYoYYYYRYoYYYYoY",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbWYYYoYoYYYYoYoMWYYoYoYYYYoMWYYY",
    "bbBbbbbBbbbbBbbbbBbbbbBbbbbBbbbbBbbbbBbbbbBbbbWBbbbbBbbbbBbbbRBbbbbBbbbbBbbbbB",
];

pub fn start_page() -> DisplayList {
    start_page_for(Size { cols: 78, rows: 16 })
}

pub fn start_page_for(viewport: Size) -> DisplayList {
    let width = usize::from(viewport.cols);
    let height = usize::from(viewport.rows);
    if width == 0 || height == 0 {
        return DisplayList {
            rows: (0..height).map(|_| PaintedRow::default()).collect(),
            ..Default::default()
        };
    }
    let pixel_height = height * 2;
    let (art_width, art_height) = if width * ART_HEIGHT <= pixel_height * ART_WIDTH {
        (width, (width * ART_HEIGHT / ART_WIDTH).max(1))
    } else {
        ((pixel_height * ART_WIDTH / ART_HEIGHT).max(1), pixel_height)
    };
    let offset_col = (width - art_width) / 2;
    let offset_row = (pixel_height - art_height) / 2;
    let mut pixels = vec![vec![b'c'; width]; pixel_height];
    for row in 0..art_height {
        let source_row = row * ART_HEIGHT / art_height;
        for col in 0..art_width {
            let source_col = col * ART_WIDTH / art_width;
            pixels[offset_row + row][offset_col + col] = source_pixel(source_col, source_row);
        }
    }
    let mut glyphs = vec![vec![None; width]; height];
    add_letter_wave(&mut glyphs, offset_col, offset_row, art_width, art_height);
    let rows = pixels
        .chunks_exact(2)
        .zip(glyphs.iter())
        .map(|(pixels, glyphs)| painted_row(&pixels[0], &pixels[1], glyphs))
        .collect();
    DisplayList {
        rows,
        ..Default::default()
    }
}

fn source_pixel(col: usize, row: usize) -> u8 {
    let pixel = START_PAGE_PIXELS[row].as_bytes()[col];
    let surfer = (27..=47).contains(&col) && (15..=29).contains(&row);
    let wave =
        col < 52 && (9..=30).contains(&row) && matches!(pixel, b'W' | b'C' | b'B') && !surfer;
    if wave {
        if row >= 22 { b'b' } else { b'c' }
    } else {
        pixel
    }
}

fn add_letter_wave(
    glyphs: &mut [Vec<Option<WaveGlyph>>],
    offset_col: usize,
    offset_row: usize,
    art_width: usize,
    art_height: usize,
) {
    let height = glyphs.len();
    let width = glyphs.first().map_or(0, Vec::len);
    let left = offset_col;
    let back = offset_col + 28 * art_width / ART_WIDTH;
    let art_rows = art_height.div_ceil(2);
    let top = offset_row / 2;
    let board_row = top + 14 * art_rows / 18;
    let crest_row = top + 6 * art_rows / 18;
    let letters = b"TEXTSURFER";
    let span = back.saturating_sub(left).max(1);
    let depth = board_row.saturating_sub(crest_row).max(1);
    for (row, glyph_row) in glyphs
        .iter_mut()
        .enumerate()
        .take(board_row.min(height.saturating_sub(1)) + 1)
        .skip(crest_row)
    {
        for (col, glyph) in glyph_row
            .iter_mut()
            .enumerate()
            .take(back.min(width.saturating_sub(1)) + 1)
            .skip(left)
        {
            let x = (col - left) as f32 / span as f32;
            let y = (row - crest_row) as f32 / depth as f32;
            let distance = letter_wave_distance(x, y);
            if distance <= 0.14 {
                let index = (row - crest_row) * span + col - left;
                let foam = distance <= 0.065 && y < 0.82;
                *glyph = Some(WaveGlyph {
                    character: letters[index % letters.len()] as char,
                    foreground: if foam { b'W' } else { b'C' },
                    background: if foam { b'B' } else { b'b' },
                });
            }
        }
    }
}

fn letter_wave_distance(x: f32, y: f32) -> f32 {
    const PATH: [(f32, f32); 11] = [
        (1.00, 1.00),
        (0.68, 0.98),
        (0.30, 0.88),
        (0.08, 0.68),
        (0.04, 0.40),
        (0.18, 0.16),
        (0.46, 0.03),
        (0.74, 0.10),
        (0.91, 0.32),
        (0.84, 0.54),
        (0.65, 0.48),
    ];
    PATH.windows(2)
        .map(|segment| {
            let (start_x, start_y) = segment[0];
            let (end_x, end_y) = segment[1];
            let delta_x = end_x - start_x;
            let delta_y = end_y - start_y;
            let length_squared = delta_x * delta_x + delta_y * delta_y;
            let projection = (((x - start_x) * delta_x + (y - start_y) * delta_y) / length_squared)
                .clamp(0.0, 1.0);
            let nearest_x = start_x + projection * delta_x;
            let nearest_y = start_y + projection * delta_y;
            (x - nearest_x).hypot(y - nearest_y)
        })
        .fold(f32::INFINITY, f32::min)
}

fn painted_row(top: &[u8], bottom: &[u8], glyphs: &[Option<WaveGlyph>]) -> PaintedRow {
    let mut spans = Vec::new();
    let mut text = String::new();
    let mut start = 0;
    let mut previous = None;
    for (col, ((&top, &bottom), glyph)) in top.iter().zip(bottom).zip(glyphs).enumerate() {
        let (character, foreground, background) = match *glyph {
            Some(glyph) => (
                glyph.character,
                pixel_color(glyph.foreground),
                pixel_color(glyph.background),
            ),
            None => (
                if top == bottom { ' ' } else { '▀' },
                pixel_color(top),
                pixel_color(bottom),
            ),
        };
        let style = CellStyle {
            fg: Some(foreground.into()),
            bg: Some(background),
            ..Default::default()
        };
        if let Some(previous_style) = previous
            && previous_style != style
        {
            spans.push(PaintedSpan {
                col: start,
                text: std::mem::take(&mut text),
                style: previous_style,
            });
            start = col;
        }
        text.push(character);
        previous = Some(style);
    }
    if let Some(style) = previous {
        spans.push(PaintedSpan {
            col: start,
            text,
            style,
        });
    }
    PaintedRow { spans }
}

fn pixel_color(code: u8) -> Rgb {
    match code {
        b'b' => Rgb::new(0, 0, 170),
        b'c' => Rgb::new(0, 170, 170),
        b'g' => Rgb::new(0, 170, 0),
        b'm' => Rgb::new(170, 0, 170),
        b'o' => Rgb::new(170, 85, 0),
        b'B' => Rgb::new(85, 85, 255),
        b'C' => Rgb::new(85, 255, 255),
        b'G' => Rgb::new(85, 255, 85),
        b'M' => Rgb::new(255, 85, 255),
        b'R' => Rgb::new(255, 85, 85),
        b'W' => Rgb::new(255, 255, 255),
        b'Y' => Rgb::new(255, 255, 85),
        _ => unreachable!("invalid start-page pixel"),
    }
}

pub fn content_for(url: &str) -> DisplayList {
    if url.is_empty() {
        return start_page();
    }
    DisplayList::from_lines(&[
        url.to_string(),
        String::new(),
        format!("  fetching {url}"),
        "  loading through the HTML, style, layout and paint pipeline".to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geom::Size;
    use crate::core::style::Rgb;
    use crate::ui::test_util::buffer_string;
    use crate::ui::theme::DEFAULT;
    use crate::ui::widgets::content::{Content, ContentLines};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Widget;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn start_page_is_the_exact_colored_default_terminal_canvas() {
        let page = start_page();

        assert_eq!(page.rows.len(), 16);
        assert!(
            page.text_lines()
                .iter()
                .all(|row| UnicodeWidthStr::width(row.as_str()) == 78)
        );
        assert!(page.text_lines().iter().any(|row| row.contains('▀')));
        let letters = page
            .text_lines()
            .iter()
            .flat_map(|row| row.chars())
            .filter(char::is_ascii_alphabetic)
            .count();
        assert!(letters >= 24);
        assert!(!page.text_lines().join("\n").contains("Enter an address"));

        let styles: Vec<_> = page
            .rows
            .iter()
            .flat_map(|row| row.spans.iter().map(|span| span.style))
            .collect();
        assert!(styles.iter().all(|style| style.bg.is_some()));
        assert!(
            styles
                .iter()
                .any(|style| style.fg == Some(Rgb::new(255, 255, 85).into()))
        );
        assert!(
            styles
                .iter()
                .any(|style| style.bg == Some(Rgb::new(0, 0, 170)))
        );
        let wave_spans: Vec<_> = page
            .rows
            .iter()
            .flat_map(|row| row.spans.iter())
            .filter(|span| {
                span.text
                    .chars()
                    .any(|character| character.is_ascii_alphabetic())
            })
            .collect();
        assert!(
            wave_spans
                .iter()
                .any(|span| span.style.bg == Some(Rgb::new(85, 85, 255)))
        );
    }

    #[test]
    fn start_page_renders_inside_the_default_content_frame() {
        let page = start_page();
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        painted: &page,
                        scroll: 0,
                        text_fields: Vec::new(),
                    },
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut());
            })
            .unwrap();

        insta::assert_snapshot!(buffer_string(terminal.backend().buffer()));
    }

    #[test]
    fn start_page_scales_to_fill_a_larger_content_viewport() {
        let page = start_page_for(Size {
            cols: 156,
            rows: 36,
        });

        assert_eq!(page.rows.len(), 36);
        assert!(
            page.text_lines()
                .iter()
                .all(|row| UnicodeWidthStr::width(row.as_str()) == 156)
        );
        assert_ne!(page, start_page());
        let default_letters = start_page()
            .text_lines()
            .iter()
            .flat_map(|row| row.chars())
            .filter(char::is_ascii_alphabetic)
            .count();
        let scaled_letters = page
            .text_lines()
            .iter()
            .flat_map(|row| row.chars())
            .filter(char::is_ascii_alphabetic)
            .count();
        assert!(scaled_letters > default_letters);
    }
}
