//! `check-registries` — a dispatch `match` that grows with the number of kinds must be a
//! registry, not a `match` edited in place (`docs/engineering/CONVENTIONS.md §1.2`).
//!
//! This check is a *heuristic*: it flags large `match` blocks outside files whose path
//! mentions `registry`, with an explicit allowlist for legitimate exceptions. It is
//! deliberately conservative — a false positive blocks work, so the threshold is high
//! and the escape hatch is documented.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

use crate::ctx::Ctx;
use crate::report::Report;
use crate::scan;

const NAME: &str = "check-registries";

/// Number of arms above which a non-registry `match` is suspect.
///
/// This is a deliberately coarse backstop for `CONVENTIONS.md §1.2`, which asks for a
/// registry as soon as dispatch starts growing. The check cannot tell a growing
/// *domain* (statement kinds, widget kinds) from a closed *mapping*, so it fires high
/// and leaves judgement to review. A small match over a closed enum is often better
/// than a registry, because the compiler enforces exhaustiveness and a table would
/// silently default.
const ARM_THRESHOLD: usize = 12;

const ALLOWLIST: &str = "xtask/registry-allowlist.toml";

#[derive(Debug, Default, Deserialize)]
struct Allowlist {
    #[serde(default)]
    files: Vec<String>,
}

/// Runs the check.
pub fn run(ctx: &Ctx) -> Report {
    let allow: BTreeSet<String> = load_allowlist(&ctx.root);
    let mut report = Report::pass(NAME, "");
    let mut scanned = 0usize;

    for path in scan::walk(&ctx.root, &["rs"]) {
        let rel = ctx.rel(&path);
        if is_exempt(&rel) || allow.contains(&rel) {
            continue;
        }
        let Some(text) = scan::read(&path) else {
            continue;
        };
        scanned += 1;

        let blanked = scan::blank_comments(&text);
        for hit in match_blocks(&blanked) {
            if hit.arms >= ARM_THRESHOLD {
                report.violation(format!(
                    "{rel}:{}: `match` with {} arms\n  rule: CONVENTIONS.md §1.2 — dispatch that grows with the number of kinds is a registry\n  fix: move the arms into a registry module (CONVENTIONS.md §4), or list the file in {ALLOWLIST} with a justification",
                    hit.line, hit.arms
                ));
            }
        }
    }

    report.summary = format!("{scanned} files scanned for large dispatch matches");
    report
}

/// A `match` block that looks like domain dispatch rather than a lexical table.
struct MatchHit {
    /// The line the `match` starts on.
    line: usize,
    /// How many arms it has.
    arms: usize,
}

/// One open brace while scanning.
struct Frame {
    /// Whether this brace is a `match` body.
    is_match: bool,
    /// The line the `match` starts on.
    line: usize,
    /// Arms seen so far.
    arms: usize,
    /// How many arms had a non-literal pattern. Zero means a lexical table.
    non_literal: usize,
    /// Offset where the current arm's pattern starts.
    arm_start: usize,
}

impl Frame {
    /// This frame as a suspect hit, if it is a `match` with any non-literal arm.
    fn hit(&self) -> Option<MatchHit> {
        // A match whose every arm is a literal is a lexical table over a closed
        // alphabet (`'a'`, `b'x'`, `"->"`), not dispatch over a domain anyone extends.
        // The compiler already enforces that those are complete, which a registry would
        // give up.
        (self.is_match && self.non_literal > 0).then_some(MatchHit {
            line: self.line,
            arms: self.arms,
        })
    }
}

/// Finds every `match` block and counts its top-level arms.
///
/// Only `=>` counts as an arm; `->` is a return type and must not be miscounted, which
/// is why the scan looks for `=` followed by `>` rather than the reverse.
///
/// Matches whose arms are all literal patterns are skipped: those are lexical tables
/// over a closed alphabet, where an exhaustive `match` is the stronger structure.
fn match_blocks(src: &str) -> Vec<MatchHit> {
    let chars: Vec<char> = src.chars().collect();
    let mut stack: Vec<Frame> = Vec::new();
    let mut hits = Vec::new();
    let mut pending_match = false;
    let mut line = 1usize;
    let mut i = 0usize;

    while i < chars.len() {
        match chars[i] {
            '\n' => {
                line += 1;
                i += 1;
            }
            '{' => {
                stack.push(Frame {
                    is_match: pending_match,
                    line,
                    arms: 0,
                    non_literal: 0,
                    arm_start: i + 1,
                });
                pending_match = false;
                i += 1;
            }
            '}' => {
                if let Some(hit) = stack.pop().and_then(|frame| frame.hit()) {
                    hits.push(hit);
                }
                i += 1;
            }
            // A `match` always has a body before the next statement ends; resetting on
            // `;` keeps a stray keyword from latching onto a later block.
            ';' => {
                pending_match = false;
                i += 1;
            }
            '=' if chars.get(i + 1) == Some(&'>') => {
                record_arm(&mut stack, &chars, i);
                i += 2;
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                if chars[start..i].iter().collect::<String>() == "match" {
                    pending_match = true;
                }
            }
            _ => i += 1,
        }
    }

    hits
}

/// Records one arm against the innermost `match`, if the stack is inside one.
fn record_arm(stack: &mut [Frame], chars: &[char], arrow: usize) {
    let Some(frame) = stack.last_mut() else {
        return;
    };
    if !frame.is_match {
        return;
    }

    frame.arms += 1;
    let arm: String = chars[frame.arm_start..arrow].iter().collect();
    if !is_literal_arm(&arm) {
        frame.non_literal += 1;
    }
    frame.arm_start = arrow + 2;
}

/// Whether the pattern at the end of `arm` is a literal or a wildcard.
///
/// The text runs from the previous arm's arrow, so it contains that arm's body too; only
/// the tail is *this* arm's pattern. That is why the wildcard case inspects the final
/// token rather than the whole string.
fn is_literal_arm(arm: &str) -> bool {
    let arm = arm.trim_end();
    if arm.ends_with('\'') || arm.ends_with('"') {
        return true;
    }
    arm.rsplit(|c: char| c.is_whitespace() || c == ',' || c == '{' || c == '}')
        .next()
        .is_some_and(|token| token == "_")
}

/// Crates whose dispatch is over the language's own closed grammar.
///
/// A plugin cannot add a statement, an expression, or an item kind, so a `match` over
/// node kinds here is the *right* structure: the compiler enforces that it is complete,
/// which a registry would give up. The convention targets dispatch that extension code
/// adds to, and that lives in the runtime crates — effects, widgets, importers, passes.
/// Exempting the front end as a class is why this check stops firing on the parser and
/// the lowering walks while still watching the crates where a registry is the answer.
const CLOSED_GRAMMAR: &[&str] = &[
    "vela-syntax",
    "vela-hir",
    "vela-types",
    "vela-mir",
    "vela-bytecode",
];

/// The one crate whose dispatch is a *declared* exception rather than an oversight.
///
/// `CONVENTIONS.md §4.6` says the instruction set is closed for verification and
/// performance reasons, so adding an op touches exactly one handler table — and a registry
/// would be the wrong structure for it. `vela-vm` therefore carries one large match that is
/// correct by design, and the closed `Fault` enum beside it is the same shape.
///
/// Kept apart from [`CLOSED_GRAMMAR`] because the reasoning is different: the front end is
/// exempt because a plugin cannot add syntax, and this is exempt because the instruction
/// set is deliberately not extensible.
const CLOSED_INSTRUCTION_SET: &[&str] = &["vela-vm"];

/// Whether a file is out of scope for this heuristic.
///
/// Registry modules are the *destination* for dispatch, so their own matches are
/// expected. `xtask/` is a build tool rather than engine code, and its dispatch is the
/// check table in `checks/mod.rs`.
fn is_exempt(rel: &str) -> bool {
    rel.starts_with("xtask/")
        || rel
            .split('/')
            .any(|component| component.starts_with("registr"))
        || crate_of(rel).is_some_and(|krate| CLOSED_GRAMMAR.contains(&krate))
        || crate_of(rel).is_some_and(|krate| CLOSED_INSTRUCTION_SET.contains(&krate))
}

/// The crate a path belongs to, for `crates/<name>/…` or `<name>/…`.
fn crate_of(rel: &str) -> Option<&str> {
    let mut parts = rel.split('/');
    match parts.next()? {
        "crates" => parts.next(),
        first => Some(first),
    }
}

fn load_allowlist(root: &Path) -> BTreeSet<String> {
    let path = root.join(ALLOWLIST);
    let Some(text) = scan::read(&path) else {
        return BTreeSet::new();
    };
    toml::from_str::<Allowlist>(&text)
        .map(|a| a.files.into_iter().collect())
        .unwrap_or_default()
}
