//! `vela build` — a project into a distribution bundle.
//!
//! `BUILD_AND_ASSETS.md §1`. Both halves of the pipeline, in one command: the scripts compile
//! to `.velac`, the assets import to a manifest, and the two land in one directory as the thing
//! a target driver will later package.
//!
//! A build **refuses to ship a story that does not check**. The compiler will assemble a module
//! whose checking reported errors — that is what lets a language server keep working over a
//! broken file — so the decision to refuse belongs here, at the one command whose job is to
//! produce something a player runs.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use vela_assets::{Built, ImporterRegistry, Manifest, import_tree};
use vela_compile::Session;
use vela_diag::{Severity, render};
use vela_span::FileId;

use crate::command::{Command, Error};
use crate::commands::check::{Project, collect, load, project_screen_diagnostics};

/// Where a build goes when `--out` is not given.
const DEFAULT_OUT: &str = "dist";

/// The directory a project's assets live in, beside `src/`.
const ASSETS: &str = "assets";

/// The directory compiled modules go in, inside the bundle.
const SCRIPTS: &str = "scripts";

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

/// What one build produced.
struct Counts {
    /// How many modules compiled.
    scripts: usize,
    /// How many sources were imported.
    assets: usize,
    /// How many artifacts those sources became.
    artifacts: usize,
}

/// The options `vela build` has.
const FLAGS: &[&str] = &[
    "--out",
    "--verify-reproducible",
    "--patch-from",
    "--patch-out",
];

/// Refuses an option this command does not have.
///
/// Ignoring one is the worst of the available behaviours: `vela build --target web` would look
/// like it had built for web, and the first evidence otherwise would be a player. `--target` gets
/// its own message because `BUILD_AND_ASSETS.md §4` names it, so it is the one somebody will
/// reasonably reach for.
fn check_flags(args: &[String]) -> Result<(), Error> {
    for arg in args.iter().filter(|arg| arg.starts_with('-')) {
        if FLAGS.contains(&arg.as_str()) {
            continue;
        }
        if arg == "--target" {
            return Err(Error::usage(
                "`--target` is specified (`BUILD_AND_ASSETS.md §4`) but not built yet: \
                 `vela build` produces one layout today",
            ));
        }
        return Err(Error::usage(format!(
            "`{arg}` is not an option of `vela build`; the ones there are: {}",
            FLAGS.join(", ")
        )));
    }
    Ok(())
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

        let target = positional(args, &["--out"])
            .map_or_else(|| self.base.clone(), |path| self.base.join(path));
        let destination = flag_value(args, "--out")
            .map_or_else(|| target.join(DEFAULT_OUT), |path| self.base.join(path));
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
        let project = collect(&target)?;
        let counts = bundle(&project, &destination, out)?;

        // Printed relative to where the command was run, so the common case reads as the path
        // the user would type rather than as an absolute one with a `./` in the middle of it.
        let shown = destination.strip_prefix(&self.base).unwrap_or(&destination);
        let _ = writeln!(
            out,
            "built {} script(s), {} asset(s), {} artifact(s) -> {}/",
            counts.scripts,
            counts.assets,
            counts.artifacts,
            shown.display()
        );

        if let (Some(previous), Some(where_to)) = (&patch_from, &patch_out) {
            write_patch(previous, &destination, where_to, out)?;
        }

        if verify {
            verify_reproducible(&project, &destination)?;
            let _ = writeln!(
                out,
                "reproducible: two independent builds are byte-identical"
            );
        }
        Ok(())
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

/// Builds a project into `destination`, returning what it wrote.
///
/// Everything a build does goes through here, including the check — so `--verify-reproducible`
/// verifying "a build" verifies the thing the user gets, rather than a second code path that
/// happens to agree today.
fn bundle(project: &Project, destination: &Path, out: &mut dyn Write) -> Result<Counts, Error> {
    let mut session = load(project)?;
    refuse_errors(&mut session, project, out)?;

    let scripts = write_scripts(&mut session, destination)?;
    let (assets, artifacts) = write_assets(project, destination)?;
    Ok(Counts {
        scripts,
        assets,
        artifacts,
    })
}

/// Stops the build when the story does not check.
///
/// Errors only: a warning is something the author has decided to live with, and a build that
/// refused on those is a build people learn to bypass. `vela check --deny-warnings` is where
/// that decision belongs.
fn refuse_errors(
    session: &mut Session,
    project: &Project,
    out: &mut dyn Write,
) -> Result<(), Error> {
    let mut diagnostics = session.diagnostics();
    diagnostics.extend(project_screen_diagnostics(project));

    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .count();
    if errors == 0 {
        return Ok(());
    }

    for diagnostic in &diagnostics {
        if diagnostic.severity() == Severity::Error {
            let _ = out.write_all(render(diagnostic, session.sources()).as_bytes());
        }
    }
    Err(Error::diagnostics(format!(
        "{errors} error(s); refusing to build"
    )))
}

/// Compiles every module and writes it as `.velac`, returning how many.
///
/// Debug information is left out: this is the bundle a player runs, and `RUNTIME.md §9` keeps
/// trace hooks out of a release build rather than paying for them in every shipped game.
fn write_scripts(session: &mut Session, destination: &Path) -> Result<usize, Error> {
    // Collected first: `mir` needs the session mutably, and the filter reads it.
    let files: Vec<FileId> = session
        .file_ids()
        .into_iter()
        .filter(|file| session.module_of(*file).is_some())
        .collect();

    for file in &files {
        // The name the driver gave the file — its path under `src/` — so the bundle mirrors the
        // project rather than inventing a naming scheme of its own.
        let name = session.sources().file(*file).name().to_string();
        let compiled = session.mir(*file);

        let module = vela_bytecode::compile(&compiled.module, false);
        let diagnostics = vela_bytecode::verify(&module);
        if !diagnostics.is_empty() {
            // Not the author's fault, and not something to ship: the verifier exists so that a
            // module reaching a player cannot be malformed.
            return Err(Error::internal(format!(
                "`{name}` did not verify ({} problem(s))",
                diagnostics.len()
            )));
        }

        let path = destination.join(SCRIPTS).join(name).with_extension("velac");
        write_file(&path, &vela_bytecode::encode(&module))?;
    }

    Ok(files.len())
}

/// Imports the assets and writes them beside their manifest.
///
/// A project with no `assets/` is a project with no assets, not an error: the one-line script
/// `VISION.md §1` protects must stay buildable.
fn write_assets(project: &Project, destination: &Path) -> Result<(usize, usize), Error> {
    let root = project
        .source_root
        .parent()
        .map(|root| root.join(ASSETS))
        .unwrap_or_default();

    let built = if root.is_dir() {
        import_tree(&root, &ImporterRegistry::builtin())
            .map_err(|error| Error::diagnostics(error.to_string()))?
    } else {
        Built {
            manifest: Manifest::new(),
            artifacts: Vec::new(),
        }
    };

    write(destination, &built)?;
    Ok((built.manifest.assets.len(), built.artifacts.len()))
}

/// Writes the manifest and every artifact under `destination`.
///
/// Existing files are overwritten but not pruned, which is deliberate for now: a stale artifact
/// is invisible to everything that reads the manifest, and `--verify-reproducible` builds into
/// two fresh directories rather than trusting a reused one. Pruning belongs with the patch work,
/// which is the first thing that would care.
fn write(destination: &Path, built: &Built) -> Result<(), Error> {
    for (path, bytes) in &built.artifacts {
        write_file(&destination.join(ASSETS).join(path), bytes)?;
    }

    let manifest = built
        .manifest
        .to_json()
        .map_err(|error| Error::internal(error.to_string()))?;
    write_file(&destination.join("manifest.json"), manifest.as_bytes())
}

/// Builds the project a second time, into a directory of its own, and compares.
///
/// A second *build*, not a second copy: a fresh session and a fresh import, because a check that
/// re-used a cached value would be comparing a build with itself. The trees are compared file by
/// file rather than by one digest of the whole, so a failure names what differed — "the bundles
/// differ" is not something anyone can act on.
fn verify_reproducible(project: &Project, destination: &Path) -> Result<(), Error> {
    let scratch = scratch_directory();
    let _ = fs::remove_dir_all(&scratch);

    let mut sink = std::io::sink();
    let outcome = bundle(project, &scratch, &mut sink);
    let differences = match outcome {
        Ok(_) => compare(destination, &scratch),
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

/// Writes a file, creating its parents.
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::internal(format!("cannot create {}: {e}", parent.display())))?;
    }
    fs::write(path, bytes)
        .map_err(|e| Error::internal(format!("cannot write {}: {e}", path.display())))
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
