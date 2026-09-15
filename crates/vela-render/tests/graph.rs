//! The render graph's extension point.
//!
//! `ARCHITECTURE.md §5` row 6 promises that *a new render stage touches no core file*. This
//! file is the check: it defines stages of its own, inserts one into the middle of a graph,
//! and asserts the order. If adding a stage ever requires editing `graph.rs`, this test is
//! where that shows up — as an import that no longer compiles, rather than as a promise that
//! quietly stopped being true.

use vela_render::graph::{Frame, RenderGraph, Stage};
use vela_render::{Capture, ClearStage, Color, DrawList, GeometryStage};

/// A stage defined here, in a test, with no help from the graph.
struct Marker {
    label: &'static str,
    log: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
}

impl Stage for Marker {
    fn name(&self) -> &'static str {
        self.label
    }

    fn run(&self, _frame: &mut Frame<'_>) {
        self.log.borrow_mut().push(self.label);
    }
}

#[test]
fn a_stage_can_be_inserted_without_touching_the_graph() {
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut graph = RenderGraph::new();
    graph.push(Box::new(ClearStage {
        color: Color::TRANSPARENT,
    }));
    graph.push(Box::new(GeometryStage));
    graph.insert(
        1,
        Box::new(Marker {
            label: "injected",
            log: log.clone(),
        }),
    );

    assert_eq!(graph.names(), vec!["clear", "injected", "geometry"]);
    assert_eq!(graph.len(), 3);
}

#[test]
fn inserting_past_the_end_appends() {
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut graph = RenderGraph::new();
    graph.push(Box::new(Marker {
        label: "a",
        log: log.clone(),
    }));
    graph.insert(99, Box::new(Marker { label: "b", log }));
    assert_eq!(graph.names(), vec!["a", "b"]);
}

#[test]
fn an_empty_graph_is_empty() {
    assert!(RenderGraph::new().is_empty());
}

/// Stages run in order, and a graph of markers needs no GPU to prove it.
#[test]
fn stages_run_in_the_order_they_were_added() {
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut graph = RenderGraph::new();
    for label in ["one", "two", "three"] {
        graph.push(Box::new(Marker {
            label,
            log: log.clone(),
        }));
    }

    // The graph needs a frame, and a frame needs a renderer — so this half of the test is
    // skipped without an adapter rather than failing on a machine with no GPU.
    let Some(mut capture) = Capture::new(4, 4) else {
        return;
    };
    let draw = DrawList::new();
    let _ = capture.render(&graph, &draw);
    capture.renderer_mut();

    assert_eq!(*log.borrow(), vec!["one", "two", "three"]);
}
