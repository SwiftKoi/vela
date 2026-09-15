//! The render graph: an ordered list of stages, and the frame they share.
//!
//! `ARCHITECTURE.md §5` requires that *adding a render stage touches no core file*, and this
//! is where that is either true or a lie. A stage is a trait object with one method; the
//! graph is a `Vec` of them. Nothing in this file knows what a stage does, what a glyph is,
//! or how many passes a frame has — so a new stage is a new type in a new file, and the only
//! edit is the one line that pushes it.
//!
//! There is a test for exactly that, in `tests/graph.rs`: it defines a stage, inserts it in
//! the middle, and asserts the order. If that test ever needs a change to this file, the
//! extension point has regressed.

use wgpu::CommandEncoder;

use crate::draw::DrawList;
use crate::renderer::Renderer;

/// What a stage is given: everything a pass needs, and nothing about the stages around it.
pub struct Frame<'a> {
    /// The encoder to record into.
    pub encoder: &'a mut CommandEncoder,
    /// The view this frame is rendered into.
    pub target: &'a wgpu::TextureView,
    /// The frame size in pixels.
    pub size: (u32, u32),
    /// What to draw.
    pub draw: &'a DrawList,
    /// The device-side resources a stage draws with.
    pub renderer: &'a Renderer,
}

/// One step of rendering.
///
/// Takes `&self` rather than `&mut self` so a stage can be shared: a stage holds configuration,
/// not per-frame state, and per-frame state belongs in the `Frame` it is handed.
pub trait Stage {
    /// A name, for a render-doc dump and for a failing test to say *which* stage.
    fn name(&self) -> &'static str;

    /// Records this stage's drawing.
    fn run(&self, frame: &mut Frame<'_>);
}

/// An ordered sequence of stages.
#[derive(Default)]
pub struct RenderGraph {
    stages: Vec<Box<dyn Stage>>,
}

impl RenderGraph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a stage.
    pub fn push(&mut self, stage: Box<dyn Stage>) {
        self.stages.push(stage);
    }

    /// Inserts a stage at `index`, clamping to the end.
    pub fn insert(&mut self, index: usize, stage: Box<dyn Stage>) {
        let index = index.min(self.stages.len());
        self.stages.insert(index, stage);
    }

    /// The stage names, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.stages.iter().map(|stage| stage.name()).collect()
    }

    /// How many stages there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stages.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Runs every stage, in order, against one frame.
    pub fn run(&self, frame: &mut Frame<'_>) {
        for stage in &self.stages {
            stage.run(frame);
        }
    }
}
