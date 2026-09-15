//! `vela check` — report diagnostics without running the game.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use vela_assets::{ImporterRegistry, import_tree};
use vela_compile::Session;
use vela_diag::{Severity, render};

use crate::command::{Command, Error};
use crate::manifest::Manifest;

/// How diagnostics are rendered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Format {
    /// For a person, with carets (`TOOLING.md §2.1`).
    Human,
    /// The documented machine-readable shape (`TOOLING.md §2.2`).
    Json,
    /// For a code-scanning service (`TOOLING.md §2.3`).
    Sarif,
}

/// What `vela check` produces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Emit {
    /// Diagnostics only. What a check is for.
    Diagnostics,
    /// The MIR each module lowered to, for reading.
    ///
    /// Not a separate command because it answers the same question from the other end:
    /// `check` says what the front end made of a story, `--emit mir` shows what the middle
    /// end made of *that*.
    Mir,
    /// The verified bytecode, as a listing.
    ///
    /// The end of the same chain: MIR is what the compiler decided, bytecode is what it
    /// emits, and a disassembly is the only way to read the difference.
    Disasm,
}

/// The `vela check` command.
///
/// The base directory is a field rather than a call to `current_dir` inside `run`, so
/// the command is testable without touching the developer's working directory.
pub struct Check {
    base: PathBuf,
}

impl Check {
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

impl Command for Check {
    fn name(&self) -> &'static str {
        "check"
    }

    fn about(&self) -> &'static str {
        "check a project without running it"
    }

    fn run(&self, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let deny_warnings = args.iter().any(|arg| arg == "--deny-warnings");

        let format = match flag_value(args, "--format") {
            None | Some("human") => Format::Human,
            Some("json") => Format::Json,
            Some("sarif") => Format::Sarif,
            Some(other) => {
                return Err(Error::usage(format!(
                    "unknown format `{other}`; expected human, json, or sarif"
                )));
            }
        };

        let emit = match flag_value(args, "--emit") {
            None | Some("diagnostics") => Emit::Diagnostics,
            Some("mir") => Emit::Mir,
            Some("disasm") => Emit::Disasm,
            Some(other) => {
                return Err(Error::usage(format!(
                    "unknown emit `{other}`; expected diagnostics, mir, or disasm"
                )));
            }
        };

        let target =
            positional(args).map_or_else(|| self.base.clone(), |path| self.base.join(path));

        let project = collect(&target)?;
        let (errors, warnings) = self.report(&project, format, emit, out)?;

        if errors > 0 || (deny_warnings && warnings > 0) {
            // The tally is a courtesy for a person at a terminal, and it would be trailing
            // garbage in a document a tool has to parse.
            if format == Format::Human {
                return Err(Error::diagnostics(format!(
                    "{errors} error(s), {warnings} warning(s)"
                )));
            }
            return Err(Error::silent());
        }

        // Only when the output is diagnostics for a person. A summary appended to MIR
        // makes the document undiffable, exactly as it made JSON unparseable.
        if format == Format::Human && emit == Emit::Diagnostics {
            let _ = writeln!(out, "checked {} file(s): no problems", project.files.len());
        }
        Ok(())
    }
}

impl Check {
    /// Loads every file into a session, then reports the diagnostics it produces.
    ///
    /// Going through `vela-compile` rather than parsing directly means the CLI exercises
    /// the same memoized queries the language server will, so the incremental behaviour
    /// is load-bearing here rather than only in tests.
    fn report(
        &self,
        project: &Project,
        format: Format,
        emit: Emit,
        out: &mut dyn Write,
    ) -> Result<(usize, usize), Error> {
        let mut session = load(project)?;

        // One answer, asked by both this and the language server (`vela_lsp::diagnostics`): an editor
        // that disagreed with `vela check` about a file would be worse than no editor, so neither
        // side assembles its own list. The earlier version of this comment claimed the language
        // server could check screens; it could not, because the widget vocabulary is rank 8 and the
        // server was rank 8 too — which is why the server is now rank 9 (`ARCHITECTURE.md §1`).
        let diagnostics = vela_lsp::diagnostics::project(&mut session);

        match emit {
            Emit::Mir => write_mir(&mut session, out)?,
            Emit::Disasm => write_disasm(&mut session, out)?,
            Emit::Diagnostics => {}
        }

        let sources = session.sources();

        let (mut errors, mut warnings) = (0usize, 0usize);
        for diagnostic in &diagnostics {
            if diagnostic.severity() == Severity::Error {
                errors += 1;
            } else {
                warnings += 1;
            }
        }

        // Counting first, then rendering: the machine-readable formats carry the tally in
        // the exit code rather than the output, and none of them should be written twice.
        match format {
            Format::Human => {
                for diagnostic in &diagnostics {
                    let _ = out.write_all(render(diagnostic, sources).as_bytes());
                }
            }
            Format::Json => {
                let _ = writeln!(out, "{}", crate::format::json(&diagnostics, sources));
            }
            Format::Sarif => {
                let _ = writeln!(
                    out,
                    "{}",
                    crate::format::sarif(&diagnostics, sources, crate::VERSION)
                );
            }
        }

        Ok((errors, warnings))
    }
}

/// Writes the **linked program's** MIR.
///
/// The program, not each module: a cross-module `jump` is an unresolved reference until linking,
/// and a dump that showed it as written would be a listing of something that is not what runs
/// (`LANGUAGE.md §6`).
///
/// # Errors
///
/// Fails when the modules cannot be linked, which is the only thing that can go wrong here.
fn write_mir(session: &mut Session, out: &mut dyn Write) -> Result<(), Error> {
    let linked = session
        .linked()
        .map_err(|error| Error::diagnostics(error.to_string()))?;
    let _ = out.write_all(vela_mir::print_module(&linked).as_bytes());
    Ok(())
}

/// Writes the linked program's bytecode, as a listing.
///
/// Verification runs first, and a program that does not verify is not disassembled: a listing of
/// something that cannot run would be read as though it could.
///
/// # Errors
///
/// Fails when the modules cannot be linked, or the result does not verify — the second is an
/// engine bug, and saying so beats printing a listing nobody can trust.
fn write_disasm(session: &mut Session, out: &mut dyn Write) -> Result<(), Error> {
    let linked = session
        .linked()
        .map_err(|error| Error::diagnostics(error.to_string()))?;
    let bytecode = vela_bytecode::compile(&linked, true);

    let diagnostics = vela_bytecode::verify(&bytecode);
    if !diagnostics.is_empty() {
        for diagnostic in &diagnostics {
            let _ = out.write_all(render(diagnostic, session.sources()).as_bytes());
        }
        return Err(Error::internal(
            "the linked program does not verify".to_string(),
        ));
    }

    let _ = out.write_all(vela_bytecode::disassemble(&bytecode).as_bytes());
    Ok(())
}

/// Loads a project into a session: its sources, its entry point, and its asset manifest.
///
/// Separated from `report` because these are three *project settings* the session is given,
/// and keeping them together is what makes it obvious that all three have to be supplied before
/// any query runs — a session that is asked for diagnostics before its manifest is set answers
/// a different question.
pub(crate) fn load(project: &Project) -> Result<Session, Error> {
    let mut session = Session::new();

    // A project says where the story starts, which is what makes reachability answerable. A
    // single-file target has no manifest and so gets no entry point — and therefore no
    // unreachable-label warnings, rather than warnings about everything.
    if let Some(manifest) = &project.manifest
        && !session.set_entry(&manifest.project.entry)
    {
        return Err(Error::usage(format!(
            "`{}` is not a valid entry point; expected `module.label`",
            manifest.project.entry
        )));
    }

    for path in &project.files {
        let text = fs::read_to_string(path)
            .map_err(|e| Error::internal(format!("cannot read {}: {e}", path.display())))?;

        // A module's name comes from its path *under the source root*, not from wherever the
        // command was run (`LANGUAGE.md §6`). Passing the display path would name the module
        // after the directory it happens to sit in.
        session.set_file(module_path(&project.source_root, path), text);
    }

    // The manifest `@"path"` literals resolve against (`LANGUAGE.md §7.5`). Imported here
    // rather than read from `dist/`, because a check that trusted a manifest the last build
    // wrote would report about the assets as they were, not as they are — and the editor and
    // the build disagreeing is the failure this check exists to prevent.
    //
    // Only for a project: a single-file target has no assets, and an empty set would turn
    // "compiled out of its project" into "referenced a missing asset".
    if project.manifest.is_some() {
        session.set_assets(asset_sources(project)?);
    }

    Ok(session)
}

/// A file's path relative to the source root: a module name, and a short display path.
pub(crate) fn module_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The asset sources a project's `assets/` imports to.
///
/// An import failure is the project's problem, not the checker's, so it is reported here as a
/// diagnostic-bearing exit rather than swallowed: a manifest that cannot be built is a
/// manifest `@"path"` cannot be resolved against, and silently skipping the check would turn
/// "the build is broken" into "the check found nothing".
fn asset_sources(project: &Project) -> Result<Vec<String>, Error> {
    // `source_root` is `<project>/src`, so the assets directory is its sibling.
    let Some(root) = project.source_root.parent() else {
        return Ok(Vec::new());
    };
    let assets = root.join("assets");
    if !assets.is_dir() {
        return Ok(Vec::new());
    }

    let built = import_tree(&assets, &ImporterRegistry::builtin())
        .map_err(|error| Error::diagnostics(error.to_string()))?;
    Ok(built
        .manifest
        .assets
        .iter()
        .map(|asset| asset.source.clone())
        .collect())
}

/// The value given to a `--flag value` argument, if the flag is present.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// The first positional argument, skipping flags *and* the values they take.
///
/// Without the skip, `--format human` would offer `human` as the path to check.
fn positional(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--format" || arg == "--emit" {
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

/// A project's files, and the root module names are relative to.
pub(crate) struct Project {
    /// The directory module names are relative to: `src/` for a project, the file's own
    /// directory for a single file.
    pub(crate) source_root: PathBuf,
    /// Every `.vela` file, sorted.
    pub(crate) files: Vec<PathBuf>,
    /// The project manifest, when the target is a directory.
    pub(crate) manifest: Option<Manifest>,
}

/// Resolves a target to the files it names.
///
/// A file target is checked directly; a directory target is a project and must have a
/// `vela.toml`, whose sources live under `src/` (`LANGUAGE.md §6`).
pub(crate) fn collect(target: &Path) -> Result<Project, Error> {
    if target.is_file() {
        let source_root = target.parent().unwrap_or(Path::new(".")).to_path_buf();
        return Ok(Project {
            source_root,
            files: vec![target.to_path_buf()],
            manifest: None,
        });
    }
    if !target.is_dir() {
        return Err(Error::usage(format!(
            "`{}` is not a file or a directory",
            target.display()
        )));
    }
    let manifest_path = target.join("vela.toml");
    if !manifest_path.is_file() {
        return Err(Error::usage(format!(
            "`{}` has no vela.toml, so it is not a project",
            target.display()
        )));
    }
    let manifest = Manifest::read(&manifest_path).map_err(Error::usage)?;

    let source_root = target.join("src");
    let mut files = Vec::new();
    walk(&source_root, &mut files);
    // Sorted so that two runs report in the same order, whatever the filesystem says.
    files.sort();
    Ok(Project {
        source_root,
        files,
        manifest: Some(manifest),
    })
}

/// Collects `.vela` files under `dir`.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "vela") {
            out.push(path);
        }
    }
}
