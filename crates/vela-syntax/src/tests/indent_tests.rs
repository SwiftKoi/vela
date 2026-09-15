use crate::{IndentAction, IndentError, IndentStack};

#[test]
fn the_first_line_of_a_file_opens_a_block() {
    let mut stack = IndentStack::new();
    assert_eq!(stack.advance(0), Ok(IndentAction::Same));
    assert_eq!(stack.depth(), 0);
}

#[test]
fn indenting_opens_a_block_and_dedenting_closes_it() {
    let mut stack = IndentStack::new();

    assert_eq!(stack.advance(4), Ok(IndentAction::Indent));
    assert_eq!(stack.depth(), 1);
    assert_eq!(stack.current(), 4);

    assert_eq!(stack.advance(8), Ok(IndentAction::Indent));
    assert_eq!(stack.depth(), 2);

    // Back to the middle level: one dedent.
    assert_eq!(stack.advance(4), Ok(IndentAction::Dedent(1)));
    assert_eq!(stack.depth(), 1);

    // Back to the top: one more.
    assert_eq!(stack.advance(0), Ok(IndentAction::Dedent(1)));
    assert_eq!(stack.depth(), 0);
}

#[test]
fn closing_several_blocks_at_once_reports_how_many() {
    let mut stack = IndentStack::new();
    stack.advance(4).expect("indent");
    stack.advance(8).expect("indent");
    stack.advance(12).expect("indent");

    assert_eq!(stack.advance(0), Ok(IndentAction::Dedent(3)));
    assert_eq!(stack.depth(), 0);
}

#[test]
fn a_block_remembers_the_step_its_first_child_used_e0004() {
    let mut stack = IndentStack::new();
    // The first child of the root establishes a step of four.
    assert_eq!(stack.advance(4), Ok(IndentAction::Indent));

    // Returning to the root and indenting by eight is a different step, and is
    // reported at the second line rather than surfacing later as a parse failure.
    assert_eq!(stack.advance(0), Ok(IndentAction::Dedent(1)));
    assert_eq!(
        stack.advance(8),
        Err(IndentError::InconsistentStep {
            expected: 4,
            found: 8
        })
    );
}

#[test]
fn dropping_between_two_open_levels_is_reported_e0005() {
    let mut stack = IndentStack::new();
    stack.advance(4).expect("indent");
    stack.advance(8).expect("indent");

    // Column six is shallower than eight but deeper than four: it is not a block that
    // is open, and guessing one would mis-parse everything after it.
    assert_eq!(
        stack.advance(6),
        Err(IndentError::MismatchedDedent { found: 6 })
    );
}

#[test]
fn an_indent_step_is_established_per_block_not_globally() {
    let mut stack = IndentStack::new();
    // Root indents by two.
    assert_eq!(stack.advance(2), Ok(IndentAction::Indent));
    // That block indents by six: a different width is fine, it is a different block.
    assert_eq!(stack.advance(8), Ok(IndentAction::Indent));
    assert_eq!(stack.current(), 8);
}
