//! The built-in menu.
//!
//! A menu is drawn by the runtime rather than by a screen, and that is not an oversight: its
//! *length* is the story's. The choice list is data — two entries in one scene, five in the
//! next — and a screen body has no way to repeat a widget over a list (`SCREENS.md §3` has no
//! `for`). A menu is the one piece of presentation whose shape the runtime cannot know when it
//! compiles a screen.

use vela_text::TextEngine;

use crate::draw::{DrawList, RectQuad};
use crate::present::Style;
use crate::text::{self, Placement};

/// A menu awaiting an answer.
///
/// The choices are the story's, in the order the command offered them; an answer is an index
/// into this list. The *selection* is the host's — it lives here so the highlight is part of
/// the same state as the frame it highlights, and a redraw cannot lose it.
pub struct Menu {
    /// The prompt above the choices, if the script gave one.
    pub prompt: Option<String>,
    /// The choices, in order.
    pub choices: Vec<String>,
    /// Which choice is highlighted.
    pub selected: usize,
}

impl Menu {
    /// A menu of `choices`, with the first one selected.
    #[must_use]
    pub fn new(prompt: Option<String>, choices: Vec<String>) -> Self {
        Self {
            prompt,
            choices,
            selected: 0,
        }
    }

    /// How many choices there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.choices.len()
    }

    /// Whether there is nothing to choose.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.choices.is_empty()
    }

    /// Moves the selection, wrapping at both ends. Returns whether it moved.
    pub fn move_by(&mut self, delta: isize) -> bool {
        if self.choices.is_empty() {
            return false;
        }
        let count = self.choices.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
        true
    }

    /// Draws the menu, centred over whatever is behind it.
    pub(crate) fn build(
        &self,
        text: &mut TextEngine,
        font: &str,
        style: &Style,
        size: (u32, u32),
        draw: &mut DrawList,
    ) {
        if self.choices.is_empty() {
            return;
        }
        let (frame_width, frame_height) = (size.0 as f32, size.1 as f32);
        let pad = 28.0;
        let line = style.size * 1.8;
        let rows = self.choices.len() as f32 + if self.prompt.is_some() { 1.0 } else { 0.0 };

        // The box is as wide as its widest line, so a menu of short choices is a small box
        // rather than a full-width bar; it is centred, so it is a menu rather than a bar too.
        let widest = self.widest(text, font, style);
        let panel = Panel {
            left: (frame_width - (widest + pad * 2.0).min(frame_width - style.margin * 2.0))
                .max(0.0)
                / 2.0,
            top: (frame_height - (line * rows + pad * 2.0)) / 2.0,
            width: (widest + pad * 2.0).min(frame_width - style.margin * 2.0),
            height: line * rows + pad * 2.0,
            line,
            pad,
            font_size: style.size,
        };

        draw.push_rect(RectQuad::from_corners(
            panel.left,
            panel.top,
            panel.left + panel.width,
            panel.top + panel.height,
            style.box_fill,
        ));
        self.rows(text, font, style, &panel, draw);
    }

    /// The width of the menu's widest line: its prompt, if any, then every choice.
    fn widest(&self, text: &mut TextEngine, font: &str, style: &Style) -> f32 {
        let mut widest = 0.0f32;
        if let Some(prompt) = &self.prompt {
            widest = text
                .layout(font, style.name_size, prompt, None)
                .map_or(0.0, |layout| layout.width);
        }
        for choice in &self.choices {
            let width = text
                .layout(font, style.size, choice, None)
                .map_or(0.0, |layout| layout.width);
            widest = widest.max(width);
        }
        widest
    }

    /// Draws the prompt and the choices inside `panel`, highlighting the selection.
    fn rows(
        &self,
        text: &mut TextEngine,
        font: &str,
        style: &Style,
        panel: &Panel,
        draw: &mut DrawList,
    ) {
        let inner = panel.width - panel.pad * 2.0;
        let mut cursor = panel.top + panel.pad;
        if let Some(prompt) = &self.prompt {
            text::place(
                text,
                font,
                draw,
                prompt,
                Placement {
                    size: style.name_size,
                    left: panel.left + panel.pad,
                    top: cursor,
                    max_width: inner,
                },
                style.name,
            );
            cursor += panel.line;
        }
        for (index, choice) in self.choices.iter().enumerate() {
            let selected = index == self.selected;
            if selected {
                draw.push_rect(RectQuad::from_corners(
                    panel.left + 8.0,
                    cursor,
                    panel.left + panel.width - 8.0,
                    cursor + panel.line,
                    style.selection,
                ));
            }
            let color = if selected { style.name } else { style.body };
            text::place(
                text,
                font,
                draw,
                choice,
                Placement {
                    size: panel.font_size,
                    left: panel.left + panel.pad,
                    top: cursor + (panel.line - panel.font_size) * 0.25,
                    max_width: inner,
                },
                color,
            );
            cursor += panel.line;
        }
    }
}

/// Where a menu's rows go: the box, worked out once for the panel and every row in it.
struct Panel {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    /// One row's height.
    line: f32,
    pad: f32,
    font_size: f32,
}
