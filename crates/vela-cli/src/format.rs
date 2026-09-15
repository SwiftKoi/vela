//! Machine-readable diagnostics.
//!
//! Two shapes, for two consumers. JSON is the documented, versioned surface a build script
//! reads (`TOOLING.md §2.2`). SARIF is what a code-scanning service reads, so a story's
//! diagnostics appear inline on a pull request.
//!
//! Both are built from the same diagnostic, and both are *derived* rather than
//! re-implemented: a second traverser over the spans would drift from the renderer the
//! first time either changed.

use serde_json::{Value, json};
use vela_diag::{Diagnostic, Severity};
use vela_span::{SourceMap, Span};

/// The JSON shape, as an array of diagnostics.
#[must_use]
pub fn json(diagnostics: &[Diagnostic], sources: &SourceMap) -> Value {
    Value::Array(
        diagnostics
            .iter()
            .map(|diagnostic| one(diagnostic, sources))
            .collect(),
    )
}

/// The SARIF 2.1.0 shape, as a single run.
///
/// Reports are grouped into one run because the tool is one tool: a run per file would
/// make the same rule appear many times and split a single execution's findings.
#[must_use]
pub fn sarif(diagnostics: &[Diagnostic], sources: &SourceMap, version: &str) -> Value {
    let mut rules: Vec<Value> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for diagnostic in diagnostics {
        let id = diagnostic.code.as_str();
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        rules.push(json!({
            "id": id,
            "name": id,
            "shortDescription": { "text": diagnostic.code.title() },
            "defaultConfiguration": { "level": level(diagnostic.severity()) },
        }));
    }

    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": { "name": "vela", "version": version, "rules": rules } },
            "results": diagnostics.iter().map(|d| result(d, sources)).collect::<Vec<_>>(),
        }],
    })
}

/// One diagnostic in the documented JSON shape.
fn one(diagnostic: &Diagnostic, sources: &SourceMap) -> Value {
    let mut spans = vec![region(
        diagnostic.primary.span,
        &diagnostic.primary.message,
        sources,
    )];

    for label in &diagnostic.secondary {
        spans.push(region(label.span, &label.message, sources));
    }
    let suggestions: Vec<Value> = diagnostic
        .suggestion
        .iter()
        .map(|suggestion| {
            let mut entry = region(suggestion.span, "suggested fix", sources);
            entry["replacement"] = json!(suggestion.replacement);
            entry
        })
        .collect();

    json!({
        "code": diagnostic.code.as_str(),
        "severity": diagnostic.severity().as_str(),
        "message": diagnostic.message,
        "spans": spans,
        "notes": diagnostic.notes,
        "suggestions": suggestions,
        // Additive to the documented shape: an editor wants the advice, and adding a
        // field is compatible in a way that removing one would not be.
        "help": diagnostic.help,
    })
}

/// One diagnostic as a SARIF result.
fn result(diagnostic: &Diagnostic, sources: &SourceMap) -> Value {
    json!({
        "ruleId": diagnostic.code.as_str(),
        "level": level(diagnostic.severity()),
        "message": { "text": diagnostic.message },
        "locations": [{ "physicalLocation": physical(diagnostic.primary.span, sources) }],
        "relatedLocations": diagnostic.secondary.iter().map(|label| {
            json!({
                "message": { "text": label.message },
                "physicalLocation": physical(label.span, sources),
            })
        }).collect::<Vec<_>>(),
    })
}

/// Where a span is, in the JSON shape.
fn region(span: Span, message: &str, sources: &SourceMap) -> Value {
    let mut entry = json!({ "message": message });
    if let Some(file) = sources.get(span.file()) {
        entry["file"] = json!(file.name());
        let start = file.line_col(span.start());
        let end = file.line_col(span.end());
        // 1-based, like every tool that consumes this.
        entry["startLine"] = json!(start.line + 1);
        entry["startColumn"] = json!(start.col + 1);
        entry["endLine"] = json!(end.line + 1);
        entry["endColumn"] = json!(end.col + 1);
    }
    entry
}

/// Where a span is, in SARIF's shape.
fn physical(span: Span, sources: &SourceMap) -> Value {
    let Some(file) = sources.get(span.file()) else {
        return json!({});
    };

    let start = file.line_col(span.start());
    let end = file.line_col(span.end());
    json!({
        "artifactLocation": { "uri": file.name() },
        "region": {
            "startLine": start.line + 1,
            "startColumn": start.col + 1,
            "endLine": end.line + 1,
            "endColumn": end.col + 1,
        },
    })
}

/// SARIF's word for a severity.
fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Lint => "note",
    }
}
