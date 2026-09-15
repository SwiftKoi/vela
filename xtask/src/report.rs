//! Check results: a summary, zero or more violations, and how to print them.

/// The outcome of a single check.
#[derive(Debug)]
pub struct Report {
    /// The check's registered name.
    pub name: &'static str,
    /// One line describing what was examined, printed on success and failure alike.
    pub summary: String,
    /// Human-readable violations. Empty means the check passed.
    pub violations: Vec<String>,
}

impl Report {
    /// A passing report.
    #[must_use]
    pub fn pass(name: &'static str, summary: impl Into<String>) -> Self {
        Self {
            name,
            summary: summary.into(),
            violations: Vec::new(),
        }
    }

    /// Records a violation.
    pub fn violation(&mut self, message: impl Into<String>) {
        self.violations.push(message.into());
    }

    /// Whether the check found anything wrong.
    #[must_use]
    pub fn failed(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// Prints a report in a stable, greppable shape.
pub fn print(report: &Report) {
    if report.failed() {
        println!(
            "  FAIL  {}: {} ({} violation{})",
            report.name,
            report.summary,
            report.violations.len(),
            if report.violations.len() == 1 {
                ""
            } else {
                "s"
            }
        );
        for violation in &report.violations {
            for (i, line) in violation.lines().enumerate() {
                if i == 0 {
                    println!("          - {line}");
                } else {
                    println!("            {line}");
                }
            }
        }
    } else {
        println!("  ok    {}: {}", report.name, report.summary);
    }
}
