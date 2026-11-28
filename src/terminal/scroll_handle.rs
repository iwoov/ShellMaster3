use std::{cell::Cell, rc::Rc, sync::Arc};

use alacritty_terminal::{grid::Dimensions, grid::Scroll, sync::FairMutex, Term};
use gpui_kit::component::scroll::ScrollbarHandle;
use gpui_kit::{point, px, size, Bounds, Pixels, Point, Size};

use super::state::EventProxy;

#[derive(Clone)]
pub struct TerminalScrollHandle {
    term: Arc<FairMutex<Term<EventProxy>>>,
    line_height: Rc<Cell<Pixels>>,
    viewport_height: Rc<Cell<Pixels>>,
    viewport_bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl TerminalScrollHandle {
    pub(crate) fn new(
        term: Arc<FairMutex<Term<EventProxy>>>,
        line_height: Pixels,
        viewport_height: Pixels,
    ) -> Self {
        Self {
            term,
            line_height: Rc::new(Cell::new(line_height)),
            viewport_height: Rc::new(Cell::new(viewport_height)),
            viewport_bounds: Rc::new(Cell::new(Bounds::new(
                Point::default(),
                size(Pixels::ZERO, viewport_height),
            ))),
        }
    }

    pub(crate) fn set_line_height(&self, line_height: Pixels) {
        self.line_height.set(line_height);
    }

    pub(crate) fn set_viewport_height(&self, viewport_height: Pixels) {
        self.viewport_height.set(viewport_height);
        let mut bounds = self.viewport_bounds.get();
        bounds.size.height = viewport_height;
        self.viewport_bounds.set(bounds);
    }

    pub(crate) fn set_viewport_origin(&self, origin: Point<Pixels>) {
        let mut bounds = self.viewport_bounds.get();
        bounds.origin = origin;
        self.viewport_bounds.set(bounds);
    }

    pub(crate) fn set_viewport_width(&self, width: Pixels) {
        let mut bounds = self.viewport_bounds.get();
        bounds.size.width = width;
        self.viewport_bounds.set(bounds);
    }
}

impl ScrollbarHandle for TerminalScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        let term = self.term.lock();
        let line_height = self.line_height.get();

        let total_lines = term.total_lines();
        let viewport_lines = term.screen_lines();
        let display_offset = term.grid().display_offset();

        let max_display_offset = total_lines.saturating_sub(viewport_lines);
        let scroll_offset_lines = max_display_offset.saturating_sub(display_offset);

        point(Pixels::ZERO, -(line_height * scroll_offset_lines as f32))
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        let line_height = self.line_height.get();
        if line_height <= px(0.) {
            return;
        }

        let mut term = self.term.lock();
        let total_lines = term.total_lines();
        let viewport_lines = term.screen_lines();

        let max_display_offset = total_lines.saturating_sub(viewport_lines);
        if max_display_offset == 0 {
            return;
        }

        let offset_delta = (offset.y / line_height).round() as i32;
        let new_display_offset =
            (max_display_offset as i32 + offset_delta).clamp(0, max_display_offset as i32) as usize;

        let current_display_offset = term.grid().display_offset();
        let delta = new_display_offset as i32 - current_display_offset as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn content_size(&self) -> Size<Pixels> {
        let term = self.term.lock();
        let line_height = self.line_height.get();
        let history_lines = term.total_lines().saturating_sub(term.screen_lines());
        let viewport_height = self.viewport_height.get();
        size(
            Pixels::ZERO,
            history_lines as f32 * line_height + viewport_height,
        )
    }

    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.viewport_bounds.get()
    }
}
