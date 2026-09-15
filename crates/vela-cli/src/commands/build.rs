//! `vela build` — a project into a distribution bundle, for one target or several.
//!
//! `BUILD_AND_ASSETS.md §1` and §4. One command, one source tree: the scripts compile to
//! `.velac`, the assets import to a manifest, and the two land in a directory as the thing a
//! player runs. With `--target` the command writes one such bundle per target, each with its
//! descriptor and launcher (`commands::target`), and the *story stays the same in all of them* —
//! the VM, the World, and the bytecode are target-independent by construction.
//!
//! The contents of a bundle are written by [`crate::commands::bundle`]; this file is the command
//! around it: arguments, the patch that goes beside the build, and the reproducibility check.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use vela_compile::Session;

use crate::command::{Command, Error};
use crate::commands::bundle::{self, Counts};
use crate::commands::check::{Project, collect};
use crate::commands::target::{self, Target};

/// Where a build goes when `--out` is not given.
const DEFAULT_OUT: &str = "dist";

/// The `vela build` command.
pub struct Build {
    base: PathBuf,
}

impl Build {
    /// Creates the command rooted at `base`.
    #[must_use]
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Creates the command rooted at the process's working directory.
    #[must_use]
    pub fn at_current_dir() -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(base)
    }
}

/// The options `vela build` has, and the ones that take a value.
const FLAGS: &[&str] = &[
    "--out",
    "--target",
    "--verify-reproducible",
    "--patch-from",
    "--patch-out",
];

/// Refuses an option this command does not have, or a malformed `--target`.
///
/// Ignoring an option is the worst of the available behaviours: `vela build --targt web` would
/// look like it had built for web, and the first evidence otherwise would be a player.
fn check_flags(args: &[String]) -> Result<(), Error> {
    for arg in args.iter().filter(|arg| arg.starts_with('-')) {
        if FLAGS.contains(&arg.as_str()) {
            continue;
        }
        return Err(Error::usage(format!(
            "`{arg}` is not an option of `vela build`; the ones there are: {}",
            FLAGS.join(", ")
        )));
    }
    Ok(())
}

/// Parses the `--target` list: `win,mac,linux,web`.
fn parse_targets(args: &[String]) -> Result<Vec<&'static Target>, Error> {
    let Some(value) = flag_value(args, "--target") else {
        return Ok(Vec::new());
    };

    let mut chosen: Vec<&'static Target> = Vec::new();
    for name in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let Some(target) = target::find(name) else {
            return Err(Error::usage(format!(
                "`{name}` is not a target; the ones there are: {}",
                target::names().join(", ")
            )));
        };
        if !chosen.iter().any(|held| held.name() == target.name()) {
            chosen.push(target);
        }
    }

    if chosen.is_empty() {
        return Err(Error::usage(
            "`--target` needs at least one target, as `--target win,mac,linux,web`".to_string(),
        ));
    }
    Ok(chosen)
}

impl Command for Build {
    fn name(&self) -> &'static str {
        "build"
    }

    fn about(&self) -> &'static str {
        "compile a project into a distribution bundle"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        check_flags(args)?;

        let targets = parse_targets(args)?;
        let project_path = positional(args, &["--out", "--target"])
            .map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let destination = flag_value(args, "--out").map_or_else(
            || project_path.join(DEFAULT_OUT),
            |path| self.base.join(path),
        );
        let verify = args.iter().any(|arg| arg == "--verify-reproducible");
        let patch_from = flag_value(args, "--patch-from").map(|path| self.base.join(path));
        let patch_out = flag_value(args, "--patch-out").map(|path| self.base.join(path));
        if patch_from.is_some() && patch_out.is_none() {
            return Err(Error::usage(
                "`--patch-from` needs `--patch-out`: vela build --patch-from <previous> \
                 --patch-out <where>",
            ));
        }

        // A project, not a stray directory: `collect` is what insists on `vela.toml`, and its
        // message says so rather than this command inventing a second way to say it.
        let project = collect(&project_path)?;
        let mut session = bundle::prepare(&project, out)?;
        let rows = build_all(&project, &mut session, &destination, &targets)?;
        report(&rows, &destination, &self.base, out);

        if let (Some(previous), Some(where_to)) = (&patch_from, &patch_out) {
            write_patch(previous, &destination, where_to, out)?;
        }

        if verify {
            verify_reproducible(&project, &destination, &targets)?;
            let _ = writeln!(
                out,
                "reproducible: two independent builds are byte-identical"
            );
        }
        Ok(())
    }
}

/// Builds the bundle at `destination`, or one per target under it.
///
/// The story is target-independent, so the same session compiles it once and writes it into each
/// target directory; only the descriptor and the launcher differ (`commands::target`).
fn build_all(
    project: &Project,
    session: &mut Session,
    destination: &Path,
    targets: &[&Target],
) -> Result<Vec<(Option<&'static str>, Counts)>, Error> {
    if targets.is_empty() {
        return Ok(vec![(
            None,
            bundle::write(project, session, destination, None)?,
        )]);
    }

    let mut rows = Vec::with_capacity(targets.len());
    for target in targets {
        let dir = destination.join(target.name());
        let counts = bundle::write(project, session, &dir, Some(target))?;
        rows.push((Some(target.name()), counts));
    }
    Ok(rows)
}

/// Says what was built, relative to where the command was run.
///
/// Printed relative to the working directory so the common case reads as the path the user would
/// type rather than as an absolute one with a `./` in the middle of it.
fn report(
    rows: &[(Option<&'static str>, Counts)],
    destination: &Path,
    base: &Path,
    out: &mut dyn Write,
) {
    for (target, counts) in rows {
        let shown = target.map_or_else(|| destination.to_path_buf(), |name| destination.join(name));
        let shown = shown.strip_prefix(base).unwrap_or(&shown);
        let prefix = target.map_or_else(String::new, |name| format!("{name}: "));
        let _ = writeln!(
            out,
            "built {prefix}{} script(s), {} asset(s), {} artifact(s) -> {}/",
            counts.scripts,
            counts.assets,
            counts.artifacts,
            shown.display()
        );
    }
}

/// Writes the difference between the previous release and this one.
///
/// The ratio is printed because it is the number the acceptance bar is about (`VISION.md §5`:
/// a text-only change under 5% of the bundle) and because a patch that suddenly contains the
/// whole bundle should be visible at the moment it is built, not when a player downloads it.
fn write_patch(
    previous: &Path,
    current: &Path,
    destination: &Path,
    out: &mut dyn Write,
) -> Result<(), Error> {
    let patch = vela_assets::Patch::write_between(previous, current, destination)
        .map_err(|error| Error::diagnostics(error.to_string()))?;

    let size = patch
        .size(destination)
        .map_err(|error| Error::internal(error.to_string()))?;
    let bundle = size_of(current)?;
    let percent = if bundle == 0 {
        0.0
    } else {
        100.0 * size as f64 / bundle as f64
    };
    let shown = destination.display();
    let _ = writeln!(
        out,
        "patch: {} change(s), {} removal(s), {size} bytes ({percent:.1}% of the bundle) -> {shown}/",
        patch.entries.len(),
        patch.removed.len()
    );
    Ok(())
}

/// How many bytes a built bundle is.
fn size_of(root: &Path) -> Result<u64, Error> {
    let mut total = 0u64;
    for path in files_under(root) {
        if let Ok(meta) = fs::metadata(root.join(&path)) {
            total += meta.len();
        }
    }
    Ok(total)
}

/// Builds again, into a directory of its own, and compares.
///
/// A second *build*, not a second copy: a fresh session and a fresh import, because a check that
/// re-used a cached value would be comparing a build with itself. The trees are compared file by
/// file rather than by one digest of the whole, so a failure names what differed — "the bundles
/// differ" is not something anyone can act on.
fn verify_reproducible(
    project: &Project,
    destination: &Path,
    targets: &[&Target],
) -> Result<(), Error> {
    let scratch = scratch_directory();
    let _ = fs::remove_dir_all(&scratch);

    let outcome = (|| {
        let mut sink = std::io::sink();
        let mut session = bundle::prepare(project, &mut sink)?;
        build_all(project, &mut session, &scratch, targets)?;
        Ok::<(), Error>(())
    })();

    let differences = match outcome {
        Ok(()) => compare(destination, &scratch),
        Err(error) => {
            let _ = fs::remove_dir_all(&scratch);
            return Err(error);
        }
    };
    let _ = fs::remove_dir_all(&scratch);

    if differences.is_empty() {
        return Ok(());
    }
    Err(Error::diagnostics(format!(
        "two builds of one project differ, so the build is not reproducible:\n  {}",
        differences.join("\n  ")
    )))
}

/// A directory of its own for a verification build.
///
/// Unique per *call*, not per process: two verifications running at once — which is exactly what
/// a test runner does to itself — would otherwise share one directory and compare each other's
/// half-written output. A counter rather than a clock, because a name that changes every run
/// would make the build's own behaviour depend on when it ran.
fn scratch_directory() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nth = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("vela-verify-{}-{nth}", std::process::id()))
}

/// What two built trees disagree about. Every entry names the file, or says which tree has it.
fn compare(first: &Path, second: &Path) -> Vec<String> {
    let left = files_under(first);
    let right = files_under(second);

    let mut differences = Vec::new();
    for path in left.union(&right) {
        match (left.contains(path), right.contains(path)) {
            (true, false) => differences.push(format!("only the first build has `{path}`")),
            (false, true) => differences.push(format!("only the second build has `{path}`")),
            _ => {
                if fs::read(first.join(path)).ok() != fs::read(second.join(path)).ok() {
                    differences.push(format!("`{path}` differs"));
                }
            }
        }
    }
    differences
}

/// Every file under `root`, as paths relative to it and with `/` separators.
fn files_under(root: &Path) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(name) = path.strip_prefix(root) {
                found.insert(name.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found
}

/// The value given to a `--flag value` argument, if the flag is present.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// The first positional argument, skipping flags *and* the values they take.
///
/// Without the skip, `--out dist` would offer `dist` as the path to build.
fn positional<'a>(args: &'a [String], valued: &[&str]) -> Option<&'a str> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if valued.contains(&arg) {
            index += 2;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(arg);
    }
    None
}
