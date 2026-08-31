use ratatui::backend::Backend;
use ratatui::buffer::{Buffer, Cell, CellDiffOption};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use ratatui_image::picker::Picker;
use ratatui_image::sliced::{SignedPosition, SlicedImage, SlicedProtocol};

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use crate::core::frame::{ChromeDamage, FrameDamage, RowDamage};

use super::chrome::{
    ChromeView, compose, compose_content_rows, compose_flash, compose_menu_bar, compose_scrollbar,
    compose_status, compose_toolbar, content_rect, cursor_position,
};

pub struct FrameComposer {
    area: Rect,
    current: Buffer,
    initialized: bool,
    image_worker: Option<ImageProtocolWorker>,
    image_protocols: HashMap<(u64, u64, u16, u16, usize), SlicedProtocol>,
    /// The cells the last `present` actually handed the backend, which is what a test needs to
    /// see: a cell composed but withheld is the whole point of the skip rule.
    #[cfg(test)]
    last_drawn: Vec<(u16, u16)>,
}

type ImageProtocolKey = (u64, u64, u16, u16, usize);

pub struct ImageWorkSignal {
    pending: AtomicUsize,
    ready: AtomicBool,
}

impl ImageWorkSignal {
    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::Acquire) > 0
    }

    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

struct ImageProtocolRequest {
    key: ImageProtocolKey,
    image: crate::core::image::DecodedImage,
    width: u16,
    height: u16,
}

struct ImageProtocolResult {
    key: ImageProtocolKey,
    protocol: Option<SlicedProtocol>,
}

struct ImageProtocolWorker {
    requests: SyncSender<ImageProtocolRequest>,
    results: Receiver<ImageProtocolResult>,
    signal: Arc<ImageWorkSignal>,
    pending: HashSet<ImageProtocolKey>,
}

impl ImageProtocolWorker {
    fn new(picker: Picker) -> Self {
        let (requests, request_rx) = sync_channel::<ImageProtocolRequest>(128);
        let (result_tx, results) = sync_channel(128);
        let signal = Arc::new(ImageWorkSignal {
            pending: AtomicUsize::new(0),
            ready: AtomicBool::new(false),
        });
        let worker_signal = Arc::clone(&signal);
        std::thread::spawn(move || {
            while let Ok(request) = request_rx.recv() {
                let protocol = image::RgbaImage::from_raw(
                    request.image.width,
                    request.image.height,
                    request.image.rgba.as_ref().to_vec(),
                )
                .and_then(|buffer| {
                    SlicedProtocol::new(
                        &picker,
                        image::DynamicImage::ImageRgba8(buffer),
                        Some(ratatui::layout::Size::new(request.width, request.height)),
                    )
                    .ok()
                });
                if result_tx
                    .send(ImageProtocolResult {
                        key: request.key,
                        protocol,
                    })
                    .is_err()
                {
                    break;
                }
                worker_signal.ready.store(true, Ordering::Release);
            }
        });
        Self {
            requests,
            results,
            signal,
            pending: HashSet::new(),
        }
    }

    fn submit(
        &mut self,
        key: ImageProtocolKey,
        image: crate::core::image::DecodedImage,
        width: u16,
        height: u16,
    ) {
        if !self.pending.insert(key) {
            return;
        }
        match self.requests.try_send(ImageProtocolRequest {
            key,
            image,
            width,
            height,
        }) {
            Ok(()) => {
                self.signal.pending.fetch_add(1, Ordering::AcqRel);
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.pending.remove(&key);
            }
        }
    }

    fn poll(&mut self, protocols: &mut HashMap<ImageProtocolKey, SlicedProtocol>) {
        self.signal.ready.store(false, Ordering::Release);
        while let Ok(result) = self.results.try_recv() {
            self.pending.remove(&result.key);
            self.signal.pending.fetch_sub(1, Ordering::AcqRel);
            if let Some(protocol) = result.protocol {
                protocols.insert(result.key, protocol);
            }
        }
    }
}

impl FrameComposer {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            current: Buffer::empty(area),
            initialized: false,
            image_worker: None,
            image_protocols: HashMap::new(),
            #[cfg(test)]
            last_drawn: Vec::new(),
        }
    }

    pub fn with_image_picker(area: Rect, picker: Picker) -> Self {
        Self {
            image_worker: Some(ImageProtocolWorker::new(picker)),
            ..Self::new(area)
        }
    }

    pub fn image_work_signal(&self) -> Option<Arc<ImageWorkSignal>> {
        self.image_worker
            .as_ref()
            .map(|worker| Arc::clone(&worker.signal))
    }

    pub fn present<B: Backend>(
        &mut self,
        backend: &mut B,
        view: &ChromeView<'_>,
        damage: &FrameDamage,
    ) -> Result<(), B::Error> {
        let area = Rect::new(0, 0, view.geometry.size.cols, view.geometry.size.rows);
        self.resize(area);
        let image_worker_ready = self
            .image_worker
            .as_ref()
            .is_some_and(|worker| worker.signal.ready());
        let image_rows_changed = repaint_intersects_image(view, &damage.content.repaint);
        let image_state_changed = !self.initialized
            || damage.full()
            || damage.content.full
            || damage.content.scroll_rows != 0
            || image_rows_changed
            || image_worker_ready;
        let mut regions = Vec::new();
        let cursor = if !self.initialized || damage.full() {
            self.current.reset();
            let cursor = compose(area, &mut self.current, view);
            regions.push(area);
            cursor
        } else {
            // A retained scroll moves the whole content band, and anything pinned on top
            // of it travels along: an image the frontend places itself, or a flash notice.
            // Those pages repaint instead of scrolling.
            let pinned_scroll = damage.content.scroll_rows != 0
                && (!view.content.painted.images.is_empty() || view.flash.is_some());
            if damage.content.scroll_rows != 0 && !pinned_scroll {
                let rows = self.scroll(backend, view, damage.content.scroll_rows)?;
                if let Some(rect) = compose_content_rows(&mut self.current, view, area, rows) {
                    regions.push(rect);
                }
            }
            if damage.content.full || pinned_scroll {
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
            // The scroll region is full-width, so the thumb travelled with the text it
            // measures; anything that touched the content owes the bar a repaint.
            let content_changed = damage.content.scroll_rows != 0
                || damage.content.full
                || damage.content.repaint != RowDamage::None;
            if content_changed && let Some(rect) = compose_scrollbar(&mut self.current, view, area)
            {
                regions.push(rect);
            }
            // The content rows this sits on were just repainted, so the notice has to go
            // back on top of them.
            if content_changed && let Some(rect) = compose_flash(&mut self.current, view, area) {
                regions.push(rect);
            }
            if damage.chrome.contains(ChromeDamage::MENU_BAR)
                && let Some(rect) = compose_menu_bar(&mut self.current, view, area)
            {
                regions.push(rect);
            }
            if damage.chrome.contains(ChromeDamage::TOOLBAR)
                && let Some(rect) = compose_toolbar(&mut self.current, view, area)
            {
                regions.push(rect);
            }
            if damage.chrome.contains(ChromeDamage::STATUS)
                && let Some(rect) = compose_status(&mut self.current, view, area)
            {
                regions.push(rect);
            }
            cursor_position(view, area)
        };
        if image_state_changed && self.compose_protocol_images(view) {
            let partial = image_rows_changed
                && !damage.full()
                && !damage.content.full
                && damage.content.scroll_rows == 0
                && !image_worker_ready;
            if partial {
                regions.extend(protocol_image_damage(view, area, &damage.content.repaint));
            } else if let Some(content) = content_rect(view, area)
                && !regions.contains(&content)
            {
                regions.push(content);
            }
        }
        // A graphics protocol puts its whole escape sequence in one cell and marks every other
        // cell of the picture `Skip`. Those cells still hold the halfblock fallback the content
        // widget painted, and they are exactly what must *not* reach the terminal: printing them
        // paints text over the picture the escape just placed. Worse, a sixel leaves the real
        // cursor below the image while the escape's cell claims to have advanced one column, so
        // the backend suppresses the `MoveTo` and the whole following run lands in the wrong
        // place. Dropping the skipped cells fixes both — ratatui's own diff drops them for the
        // same reason, and this composer does its own damage tracking, so it owes the same rule.
        let cells = regions
            .iter()
            .flat_map(|rect| {
                (rect.y..rect.bottom())
                    .flat_map(|row| (rect.x..rect.right()).map(move |col| (col, row)))
            })
            .filter(|&(col, row)| self.current[(col, row)].diff_option != CellDiffOption::Skip)
            .collect::<Vec<_>>();
        #[cfg(test)]
        {
            self.last_drawn = cells.clone();
        }
        let current = &self.current;
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
        self.last_drawn.len()
    }

    #[cfg(test)]
    fn drew(&self, cell: (u16, u16)) -> bool {
        self.last_drawn.contains(&cell)
    }

    fn resize(&mut self, area: Rect) {
        if self.area == area {
            return;
        }
        self.area = area;
        self.current = Buffer::empty(area);
        self.initialized = false;
        self.image_protocols.clear();
    }

    fn compose_protocol_images(&mut self, view: &ChromeView<'_>) -> bool {
        let Some(worker) = self.image_worker.as_mut() else {
            return false;
        };
        worker.poll(&mut self.image_protocols);
        let Some(content) = content_rect(view, self.area) else {
            return false;
        };
        let occlusions = super::chrome::occlusion_rects(view, self.area);
        let live = view
            .content
            .painted
            .images
            .iter()
            .filter_map(|placement| {
                let image = view.content.painted.image_assets.get(&placement.asset_id)?;
                Some((
                    placement.asset_id.0,
                    placement.revision,
                    u16::try_from(placement.rect.width).ok()?,
                    u16::try_from(placement.rect.height).ok()?,
                    image.rgba.as_ptr() as usize,
                ))
            })
            .collect::<HashSet<_>>();
        self.image_protocols.retain(|key, _| live.contains(key));
        let mut rendered = false;
        for (image_index, placement) in view.content.painted.images.iter().enumerate() {
            if placement.clip != placement.rect {
                continue;
            }
            let viewport_end = view
                .content
                .scroll
                .saturating_add(usize::from(content.height));
            if placement.rect.row >= viewport_end
                || placement.rect.row.saturating_add(placement.rect.height) <= view.content.scroll
                || placement.rect.col.saturating_add(placement.rect.width)
                    > usize::from(content.width)
            {
                continue;
            }
            let Some(image) = view.content.painted.image_assets.get(&placement.asset_id) else {
                continue;
            };
            let Ok(width) = u16::try_from(placement.rect.width) else {
                continue;
            };
            let Ok(height) = u16::try_from(placement.rect.height) else {
                continue;
            };
            if later_overlay_overlaps(view.content.painted, image_index) {
                continue;
            }
            let screen = Rect::new(
                content.x.saturating_add(placement.rect.col as u16),
                content
                    .y
                    .saturating_add(placement.rect.row.saturating_sub(view.content.scroll) as u16),
                width,
                height,
            );
            if occlusions
                .iter()
                .any(|occlusion| occlusion.intersects(screen))
            {
                continue;
            }
            let key = (
                placement.asset_id.0,
                placement.revision,
                width,
                height,
                image.rgba.as_ptr() as usize,
            );
            if let Entry::Vacant(_) = self.image_protocols.entry(key) {
                worker.submit(key, image.clone(), width, height);
            }
            let Some(protocol) = self.image_protocols.get(&key) else {
                continue;
            };
            let Ok(x) = i16::try_from(placement.rect.col) else {
                continue;
            };
            let y = placement.rect.row as isize - view.content.scroll as isize;
            let Ok(y) = i16::try_from(y) else {
                continue;
            };
            SlicedImage::new(protocol, SignedPosition::from((x, y)))
                .render(content, &mut self.current);
            rendered = true;
        }
        rendered
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

fn repaint_intersects_image(view: &ChromeView<'_>, damage: &RowDamage) -> bool {
    let RowDamage::Ranges(ranges) = damage else {
        return *damage == RowDamage::Full && !view.content.painted.images.is_empty();
    };
    view.content.painted.images.iter().any(|image| {
        let image_rows = image.rect.row_range(usize::MAX);
        ranges
            .iter()
            .any(|range| range.start < image_rows.end && image_rows.start < range.end)
    })
}

fn protocol_image_damage(view: &ChromeView<'_>, area: Rect, damage: &RowDamage) -> Vec<Rect> {
    let (Some(content), RowDamage::Ranges(ranges)) = (content_rect(view, area), damage) else {
        return Vec::new();
    };
    view.content
        .painted
        .images
        .iter()
        .filter(|image| {
            let image_rows = image.rect.row_range(usize::MAX);
            ranges
                .iter()
                .any(|range| range.start < image_rows.end && image_rows.start < range.end)
        })
        .filter_map(|image| {
            let visible_row = image.rect.row.max(view.content.scroll);
            let visible_end = image.rect.row.saturating_add(image.rect.height).min(
                view.content
                    .scroll
                    .saturating_add(usize::from(content.height)),
            );
            if visible_row >= visible_end {
                return None;
            }
            let col = u16::try_from(image.rect.col).ok()?;
            let row = u16::try_from(visible_row - view.content.scroll).ok()?;
            Some(
                Rect::new(
                    content.x.saturating_add(col),
                    content.y.saturating_add(row),
                    u16::try_from(image.rect.width).unwrap_or(u16::MAX),
                    u16::try_from(visible_end - visible_row).unwrap_or(u16::MAX),
                )
                .intersection(content),
            )
        })
        .filter(|rect| !rect.is_empty())
        .collect()
}

fn later_overlay_overlaps(painted: &crate::paint::DisplayList, image_index: usize) -> bool {
    let Some(position) = painted
        .overlays
        .iter()
        .position(|overlay| *overlay == crate::paint::PaintOverlay::Image(image_index))
    else {
        return true;
    };
    let Some(image) = painted.images.get(image_index) else {
        return true;
    };
    painted.overlays.iter().skip(position + 1).any(|overlay| {
        let rect = match *overlay {
            crate::paint::PaintOverlay::ScaledText(index) => {
                painted.scaled_text.get(index).map(|run| run.rect)
            }
            crate::paint::PaintOverlay::Image(index) => {
                painted.images.get(index).map(|image| image.clip)
            }
        };
        rect.is_some_and(|rect| layout_rects_intersect(image.clip, rect))
    })
}

fn layout_rects_intersect(a: crate::layout::LayoutRect, b: crate::layout::LayoutRect) -> bool {
    a.width > 0
        && a.height > 0
        && b.width > 0
        && b.height > 0
        && a.col < b.col.saturating_add(b.width)
        && b.col < a.col.saturating_add(a.width)
        && a.row < b.row.saturating_add(b.height)
        && b.row < a.row.saturating_add(a.height)
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
        self.chrome.is_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::DisplayList;
    use crate::ui::test_util::draft_view;

    #[test]
    fn later_overlays_force_terminal_protocol_fallback() {
        let mut document = crate::core::dom::Document::new();
        let node = document.insert_element(None, "img", crate::core::dom::ElementNs::Html, vec![]);
        let rect = crate::layout::LayoutRect {
            col: 2,
            row: 3,
            width: 4,
            height: 2,
        };
        let mut painted = DisplayList {
            images: vec![crate::paint::PaintedImage {
                node,
                asset_id: crate::core::image::ImageAssetId(1),
                revision: 1,
                rect,
                clip: rect,
                depth: 0,
            }],
            scaled_text: vec![crate::paint::ScaledTextRun {
                node,
                rect,
                text: "X".to_string(),
                style: crate::core::style::CellStyle::default(),
                depth: 0,
                ink: true,
            }],
            overlays: vec![
                crate::paint::PaintOverlay::Image(0),
                crate::paint::PaintOverlay::ScaledText(0),
            ],
            ..Default::default()
        };
        assert!(later_overlay_overlaps(&painted, 0));
        painted.overlays.reverse();
        assert!(!later_overlay_overlaps(&painted, 0));
    }
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
    fn a_retained_scroll_puts_the_flash_notice_back_on_top() {
        // The notice covers content rows, so a scroll that repaints them in place would
        // otherwise leave half a box behind until it expired.
        let lines = (0..40).map(|row| format!("row {row}")).collect::<Vec<_>>();
        let painted = DisplayList::from_lines(&lines);
        let size = crate::core::geom::Size { cols: 60, rows: 24 };
        let area = Rect::new(0, 0, size.cols, size.rows);
        let mut view = draft_view();
        view.geometry = crate::ui::mouse::ChromeGeometry::for_size(size);
        view.content.painted = &painted;
        view.flash = Some("saved screenshots/x.png");
        let mut backend = TestBackend::new(size.cols, size.rows);
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
    }

    #[test]
    fn a_retained_scroll_redraws_the_thumb_it_dragged_along() {
        // The scroll region is full-width, so without a repaint the thumb would ride up
        // with the text instead of measuring it.
        let lines = (0..100).map(|row| format!("row {row}")).collect::<Vec<_>>();
        let painted = DisplayList::from_lines(&lines);
        let size = crate::core::geom::Size { cols: 60, rows: 24 };
        let area = Rect::new(0, 0, size.cols, size.rows);
        let mut view = draft_view();
        view.geometry = crate::ui::mouse::ChromeGeometry::for_size(size);
        view.content.painted = &painted;
        let mut backend = TestBackend::new(size.cols, size.rows);
        let mut composer = FrameComposer::new(area);
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        let thumb_top = |backend: &TestBackend| {
            (0..size.rows).find(|row| backend.buffer()[(size.cols - 1, *row)].symbol() == "█")
        };
        let before = thumb_top(&backend).expect("a thumb on a document this long");

        view.content.scroll = 10;
        let mut damage = FrameDamage::default();
        damage.scroll(10);
        composer.present(&mut backend, &view, &damage).unwrap();

        assert_ne!(
            thumb_top(&backend),
            Some(before),
            "the thumb must follow the scroll it reports"
        );
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
        damage.damage_chrome(crate::core::frame::ChromeDamage::STATUS);
        composer.present(&mut backend, &view, &damage).unwrap();
        assert_eq!(
            composer.last_drawn_cells(),
            usize::from(view.geometry.size.cols)
        );
    }

    #[test]
    fn main_menu_title_hover_damage_draws_only_the_menu_bar() {
        let area = Rect::new(0, 0, 80, 24);
        let mut view = draft_view();
        let mut backend = TestBackend::new(area.width, area.height);
        let mut composer = FrameComposer::new(area);
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        view.main_menu.hovered_title = Some(1);
        let mut damage = FrameDamage::default();
        damage.damage_chrome(crate::core::frame::ChromeDamage::MENU_BAR);
        composer.present(&mut backend, &view, &damage).unwrap();
        assert_eq!(
            composer.last_drawn_cells(),
            usize::from(view.geometry.size.cols)
        );
    }

    #[test]
    fn toolbar_and_status_damage_draw_only_their_four_rows() {
        let area = Rect::new(0, 0, 80, 24);
        let view = draft_view();
        let mut backend = TestBackend::new(area.width, area.height);
        let mut composer = FrameComposer::new(area);
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        let mut damage = FrameDamage::default();
        damage.damage_chrome(crate::core::frame::ChromeDamage::TOOLBAR);
        damage.damage_chrome(crate::core::frame::ChromeDamage::STATUS);
        composer.present(&mut backend, &view, &damage).unwrap();
        assert_eq!(
            composer.last_drawn_cells(),
            usize::from(view.geometry.size.cols) * 4
        );
    }

    #[test]
    fn terminal_protocol_images_are_prepared_once_and_scroll_by_slices() {
        let mut document = crate::core::dom::Document::new();
        let node = document.insert_element(None, "img", crate::core::dom::ElementNs::Html, vec![]);
        let asset_id = crate::core::image::ImageAssetId(11);
        let mut painted = DisplayList {
            rows: vec![crate::paint::PaintedRow::default(); 4],
            images: vec![crate::paint::PaintedImage {
                node,
                asset_id,
                revision: 1,
                rect: crate::layout::LayoutRect {
                    col: 0,
                    row: 1,
                    width: 1,
                    height: 2,
                },
                clip: crate::layout::LayoutRect {
                    col: 0,
                    row: 1,
                    width: 1,
                    height: 2,
                },
                depth: 0,
            }],
            overlays: vec![crate::paint::PaintOverlay::Image(0)],
            ..Default::default()
        };
        painted.image_assets.insert(
            asset_id,
            crate::core::image::DecodedImage {
                asset_id,
                revision: 1,
                width: 1,
                height: 2,
                rgba: std::sync::Arc::from([255, 0, 0, 255, 0, 0, 255, 255]),
            },
        );
        let area = Rect::new(0, 0, 60, 10);
        let mut view = draft_view();
        view.content.painted = &painted;
        let mut backend = TestBackend::new(area.width, area.height);
        let mut composer =
            FrameComposer::with_image_picker(area, ratatui_image::picker::Picker::halfblocks());
        let signal = composer.image_work_signal().unwrap();
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        assert_eq!(composer.image_protocols.len(), 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !signal.ready() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        assert_eq!(composer.image_protocols.len(), 1);
        let mut unrelated = FrameDamage::default();
        unrelated.repaint_rows(3..4);
        composer.present(&mut backend, &view, &unrelated).unwrap();
        assert!(
            composer.last_drawn_cells()
                < usize::from(view.geometry.size.cols) * usize::from(view.geometry.size.rows)
        );
        let mut overlap = FrameDamage::default();
        overlap.repaint_rows(1..2);
        composer.present(&mut backend, &view, &overlap).unwrap();
        assert!(composer.last_drawn_cells() > 0);
        assert!(
            composer.last_drawn_cells()
                < usize::from(view.geometry.size.cols) * usize::from(view.geometry.size.rows)
        );
        view.content.scroll = 1;
        let mut damage = FrameDamage::default();
        damage.scroll(1);
        composer.present(&mut backend, &view, &damage).unwrap();
        assert_eq!(composer.image_protocols.len(), 1);
        assert_eq!(backend.buffer()[(1, 7)].symbol(), "▀");
    }

    /// A graphics protocol carries the whole picture in one cell's escape sequence and marks the
    /// rest of the image `Skip`. Those cells still hold the halfblock fallback the content widget
    /// painted, so handing them to the backend prints text over the picture that escape just
    /// placed — and, because the escape's cell claims a width of one, the backend also suppresses
    /// the `MoveTo` and lands the rest of the run wherever the sixel left the real cursor.
    #[test]
    fn a_protocol_image_withholds_the_halfblock_cells_it_covers() {
        let mut document = crate::core::dom::Document::new();
        let node = document.insert_element(None, "img", crate::core::dom::ElementNs::Html, vec![]);
        let asset_id = crate::core::image::ImageAssetId(23);
        let rect = crate::layout::LayoutRect {
            col: 0,
            row: 0,
            width: 4,
            height: 2,
        };
        let mut painted = DisplayList {
            rows: vec![crate::paint::PaintedRow::default(); 4],
            images: vec![crate::paint::PaintedImage {
                node,
                asset_id,
                revision: 1,
                rect,
                clip: rect,
                depth: 0,
            }],
            overlays: vec![crate::paint::PaintOverlay::Image(0)],
            ..Default::default()
        };
        painted.image_assets.insert(
            asset_id,
            crate::core::image::DecodedImage {
                asset_id,
                revision: 1,
                width: 4,
                height: 4,
                rgba: std::sync::Arc::from(vec![255u8; 4 * 4 * 4]),
            },
        );
        let area = Rect::new(0, 0, 60, 10);
        let mut view = draft_view();
        view.content.painted = &painted;
        // `halfblocks()` is only the seed for a deterministic offline picker: the protocol type is
        // then set explicitly, because halfblocks are exactly the case that cannot reproduce this.
        let mut picker = ratatui_image::picker::Picker::halfblocks();
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
        let mut backend = TestBackend::new(area.width, area.height);
        let mut composer = FrameComposer::with_image_picker(area, picker);
        let signal = composer.image_work_signal().unwrap();
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !signal.ready() {
            assert!(
                std::time::Instant::now() < deadline,
                "sixel encode timed out"
            );
            std::thread::yield_now();
        }
        composer
            .present(&mut backend, &view, &FrameDamage::full())
            .unwrap();
        assert_eq!(composer.image_protocols.len(), 1);

        let content = content_rect(&view, area).expect("a content area");
        let payload = (content.x, content.y);
        assert_ne!(
            composer.current[payload].diff_option,
            CellDiffOption::Skip,
            "the escape's own cell must still be drawn"
        );
        assert!(
            composer.current[payload].symbol().len() > 64,
            "the escape's cell should carry the sixel payload"
        );
        assert!(composer.drew(payload));
        for row in content.y..content.y + u16::try_from(rect.height).unwrap() {
            for col in content.x..content.x + u16::try_from(rect.width).unwrap() {
                if (col, row) == payload {
                    continue;
                }
                assert_eq!(
                    composer.current[(col, row)].diff_option,
                    CellDiffOption::Skip,
                    "the protocol should have reserved ({col},{row})"
                );
                assert!(
                    !composer.drew((col, row)),
                    "({col},{row}) is covered by the picture and must not be printed over"
                );
            }
        }
    }
}
