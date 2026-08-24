use ratatui::backend::Backend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;

use crate::core::frame::{ChromeDamage, FrameDamage, RowDamage};

use super::chrome::{
    ChromeView, compose, compose_content_rows, compose_status, content_rect, cursor_position,
};

pub struct FrameComposer {
    area: Rect,
    current: Buffer,
    initialized: bool,
    #[cfg(test)]
    last_drawn_cells: usize,
}

impl FrameComposer {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            current: Buffer::empty(area),
            initialized: false,
            #[cfg(test)]
            last_drawn_cells: 0,
        }
    }

    pub fn present<B: Backend>(
        &mut self,
        backend: &mut B,
        view: &ChromeView<'_>,
        damage: &FrameDamage,
    ) -> Result<(), B::Error> {
        let area = Rect::new(0, 0, view.geometry.size.cols, view.geometry.size.rows);
        self.resize(area);
        let mut regions = Vec::new();
        let cursor = if !self.initialized || damage.full() {
            self.current.reset();
            let cursor = compose(area, &mut self.current, view);
            regions.push(area);
            cursor
        } else {
            if damage.content.scroll_rows != 0 {
                let rows = self.scroll(backend, view, damage.content.scroll_rows)?;
                if let Some(rect) = compose_content_rows(&mut self.current, view, area, rows) {
                    regions.push(rect);
                }
            }
            if damage.content.full {
                if let Some(content) = content_rect(view, area)
                    && let Some(rect) =
                        compose_content_rows(&mut self.current, view, area, 0..content.height)
                {
                    regions.push(rect);
                }
            } else if let RowDamage::Ranges(ranges) = &damage.content.repaint {
                let scroll = view.content.scroll;
                let height = content_rect(view, area).map_or(0, |rect| rect.height);
                for range in ranges {
                    let visible_start = range.start.max(scroll);
                    let visible_end = range.end.min(scroll + usize::from(height));
                    if visible_start < visible_end
                        && let Some(rect) = compose_content_rows(
                            &mut self.current,
                            view,
                            area,
                            u16::try_from(visible_start - scroll).unwrap_or(u16::MAX)
                                ..u16::try_from(visible_end - scroll).unwrap_or(u16::MAX),
                        )
                    {
                        regions.push(rect);
                    }
                }
            } else if damage.content.repaint == RowDamage::Full
                && let Some(content) = content_rect(view, area)
                && let Some(rect) =
                    compose_content_rows(&mut self.current, view, area, 0..content.height)
            {
                regions.push(rect);
            }
            if damage.chrome == ChromeDamage::Status
                && let Some(rect) = compose_status(&mut self.current, view, area)
            {
                regions.push(rect);
            }
            cursor_position(view, area)
        };
        let cells = regions
            .iter()
            .flat_map(|rect| {
                (rect.y..rect.bottom())
                    .flat_map(|row| (rect.x..rect.right()).map(move |col| (col, row)))
            })
            .collect::<Vec<_>>();
        let current = &self.current;
        #[cfg(test)]
        {
            self.last_drawn_cells = cells.len();
        }
        backend.draw(
            cells
                .into_iter()
                .map(|(col, row)| (col, row, &current[(col, row)])),
        )?;
        match cursor {
            Some(position) => {
                backend.set_cursor_position(position)?;
                backend.show_cursor()?;
            }
            None => backend.hide_cursor()?,
        }
        backend.flush()?;
        self.initialized = true;
        Ok(())
    }

    #[cfg(test)]
    fn last_drawn_cells(&self) -> usize {
        self.last_drawn_cells
    }

    fn resize(&mut self, area: Rect) {
        if self.area == area {
            return;
        }
        self.area = area;
        self.current = Buffer::empty(area);
        self.initialized = false;
    }

    fn scroll<B: Backend>(
        &mut self,
        backend: &mut B,
        view: &ChromeView<'_>,
        rows: i32,
    ) -> Result<std::ops::Range<u16>, B::Error> {
        let Some(content) = content_rect(view, self.area) else {
            return Ok(0..0);
        };
        let amount = rows.unsigned_abs().min(u32::from(content.height)) as u16;
        if amount == 0 || amount >= content.height {
            return Ok(0..content.height);
        }
        let region = content.y..content.bottom();
        if rows > 0 {
            backend.scroll_region_up(region.clone(), amount)?;
        } else {
            backend.scroll_region_down(region.clone(), amount)?;
        }
        shift_rows(&mut self.current, region, amount, rows > 0);
        if rows > 0 {
            Ok(content.height - amount..content.height)
        } else {
            Ok(0..amount)
        }
    }
}

fn shift_rows(buffer: &mut Buffer, region: std::ops::Range<u16>, amount: u16, up: bool) {
    let width = buffer.area.width;
    let height = region.end.saturating_sub(region.start);
    if up {
        for row in region.start..region.end - amount {
            copy_row(buffer, row + amount, row, width);
        }
        clear_rows(buffer, region.end - amount..region.end, width);
    } else {
        for row in (region.start + amount..region.end).rev() {
            copy_row(buffer, row - amount, row, width);
        }
        clear_rows(buffer, region.start..region.start + amount, width);
    }
    debug_assert!(amount < height);
}

fn copy_row(buffer: &mut Buffer, source: u16, target: u16, width: u16) {
    for col in 0..width {
        let cell = buffer[(col, source)].clone();
        buffer[(col, target)] = cell;
    }
}

fn clear_rows(buffer: &mut Buffer, rows: std::ops::Range<u16>, width: u16) {
    for row in rows {
        for col in 0..width {
            buffer[(col, row)] = Cell::default();
        }
    }
}

trait FullDamage {
    fn full(&self) -> bool;
}

impl FullDamage for FrameDamage {
    fn full(&self) -> bool {
        matches!(self.chrome, crate::core::frame::ChromeDamage::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::DisplayList;
    use crate::ui::test_util::draft_view;
    use ratatui::backend::TestBackend;

    #[test]
    fn retained_scroll_matches_a_fresh_composition() {
        let lines = (0..20).map(|row| format!("row {row}")).collect::<Vec<_>>();
        let painted = DisplayList::from_lines(&lines);
        let area = Rect::new(0, 0, 60, 10);
        let mut view = draft_view();
        view.content.painted = &painted;
        let mut backend = TestBackend::new(area.width, area.height);
        let mut composer = FrameComposer::new(area);
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        view.content.scroll = 3;
        let mut damage = FrameDamage::default();
        damage.scroll(3);
        composer.present(&mut backend, &view, &damage).unwrap();
        let mut expected = Buffer::empty(area);
        compose(area, &mut expected, &view);
        assert_eq!(backend.buffer(), &expected);
        assert!(composer.last_drawn_cells() < usize::from(area.width * area.height));
    }

    #[test]
    fn status_damage_draws_one_row_instead_of_the_whole_frame() {
        let area = Rect::new(0, 0, 80, 24);
        let view = draft_view();
        let mut backend = TestBackend::new(area.width, area.height);
        let mut composer = FrameComposer::new(area);
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        let mut damage = FrameDamage::default();
        damage.damage_chrome(crate::core::frame::ChromeDamage::Status);
        composer.present(&mut backend, &view, &damage).unwrap();
        assert_eq!(
            composer.last_drawn_cells(),
            usize::from(view.geometry.size.cols)
        );
    }
}
