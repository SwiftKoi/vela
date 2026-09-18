//! Writing a distribution bundle: compiled scripts, imported assets, and the manifest that
//! describes them (`BUILD_AND_ASSETS.md §1`, §2).
//!
//! Separated from the `build` command because the command is about *arguments* — which target,
//! where the output goes, whether to verify — and this is about *contents*. `--verify-reproducible`
//! builds the same contents twice, so both callers go through [`write`] rather than one of them
//! verifying a second code path that happens to agree today.
//!
//! A build **refuses to ship a story that does not check**. The compiler will assemble a module
//! whose checking reported errors — that is what lets a language server keep working over a
//! broken file — so the refusal belongs at the one command whose job is to produce something a
//! player runs.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use vela_assets::{Built, ImporterRegistry, Manifest, import_tree};
use vela_compile::Session;
use vela_diag::Severity;
use vela_span::FileId;

use crate::command::Error;
use crate::commands::check::{Project, load, module_path};
use crate::commands::target::Target;

/// The directory a project's assets live in, beside `src/`.
const ASSETS: &str = "assets";

/// The directory compiled modules go in, inside the bundle.
const SCRIPTS: &str = "scripts";

/// The directory a bundle's screen declarations go in.
const SCREENS: &str = "screens";

/// What one bundle contains.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    /// How many modules compiled.
    pub scripts: usize,
    /// How many sources were imported.
    pub assets: usize,
    /// How many artifacts those sources became.
    pub artifacts: usize,
}

/// Loads a project and refuses to build one that does not check.
///
/// The check runs once per *project*, not once per target: the story is target-independent, so
/// four targets are one check.
///
/// # Errors
///
/// Fails if a source cannot be read, or the story has errors.
pub fn prepare(project: &Project, out: &mut dyn Write) -> Result<Session, Error> {
    let mut session = load(project)?;
    refuse_errors(&mut session, out)?;
    Ok(session)
}

/// Writes one bundle into `destination`, for `target` if one is given.
///
/// # Errors
///
/// Fails if a module does not verify, an asset does not import, or a file cannot be written.
pub fn write(
    project: &Project,
    session: &mut Session,
    destination: &Path,
    target: Option<&Target>,
) -> Result<Counts, Error> {
    let linked = session
        .linked()
        .map_err(|error| Error::diagnostics(error.to_string()))?;
    let modules = session
        .file_ids()
        .into_iter()
        .filter(|file| session.module_of(*file).is_some())
        .count();

    // A single-file build has no `vela.toml` to name an entry point, so the linked program's first
    // label is recorded instead — otherwise the bundle would describe itself but not say where the
    // story starts. A directory target always has a manifest; `collect` insists on one.
    let entry = project
        .manifest
        .as_ref()
        .map(|manifest| manifest.project.entry.clone())
        .or_else(|| {
            linked
                .labels
                .first()
                .map(|body| body.name.as_str().to_string())
        })
        .ok_or_else(|| {
            Error::usage("the project has no labels, so there is nothing to start at".to_string())
        })?;

    write_program(&linked, &entry, destination)?;
    let mut built = build_assets(project)?;
    let name = project_name(project);

    let entry = Some(entry);
    if let Some(entry) = &entry {
        // The design frame rides along with the project's name and entry point, because a bundle
        // ships no `vela.toml` and a screen asking `variant("small")` has nothing else to measure
        // against (`SCREENS.md §2.6`). A single-file build has no manifest and therefore no size,
        // which the runtime reads as Vela's own reference — the same answer the source run gives.
        let size = project
            .manifest
            .as_ref()
            .map(|manifest| manifest.project.size.to_string());
        built
            .manifest
            .set_project(Some(name.clone()), entry.clone(), size);
    }
    built.manifest.images = image_map(project, &built.manifest);
    write_tree(destination, &built)?;
    write_screens(project, destination)?;

    if let Some(target) = target {
        let Some(entry) = &entry else {
            return Err(Error::usage(
                "`--target` needs a project with an entry point, and this one has none",
            ));
        };
        target.write(destination, &name, entry)?;
    }

    Ok(Counts {
        scripts: modules,
        assets: built.manifest.assets.len(),
        artifacts: built.artifacts.len(),
    })
}

/// Stops the build when the story does not check.
///
/// Errors only: a warning is something the author has decided to live with, and a build that
/// refused on those is a build people learn to bypass. `vela check --deny-warnings` is where
/// that decision belongs.
fn refuse_errors(session: &mut Session, out: &mut dyn Write) -> Result<(), Error> {
    // The same answer `vela check` gives and the editor publishes (`vela_lsp::diagnostics`): a build
    // that refused on a different list than the one an author sees would be a build they learn to
    // bypass.
    let diagnostics = vela_lsp::diagnostics::project(session);

    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .count();
    if errors == 0 {
        return Ok(());
    }

    for diagnostic in &diagnostics {
        if diagnostic.severity() == Severity::Error {
            let _ = out.write_all(vela_diag::render(diagnostic, session.sources()).as_bytes());
        }
    }
    Err(Error::diagnostics(format!(
        "{errors} error(s); refusing to build"
    )))
}

/// Writes the linked program as the bundle's one `.velac`, named for its entry module.
///
/// One file, because a linked program *is* one module: every label of every source file is in one
/// table under a qualified name (`vela_mir::link`), which is also what makes the entry point's
/// spelling — `module.label` — the label's name in the image. Naming the file after the entry
/// module is what lets the loader find it from the manifest alone, with no second field saying
/// where the script is.
///
/// Debug information is left out: this is the bundle a player runs, and `RUNTIME.md §9` keeps
/// trace hooks out of a release build rather than paying for them in every shipped game.
fn write_program(linked: &vela_mir::Module, entry: &str, destination: &Path) -> Result<(), Error> {
    let module = vela_bytecode::compile(linked, false);
    let diagnostics = vela_bytecode::verify(&module);
    if !diagnostics.is_empty() {
        // Not the author's fault, and not something to ship: the verifier exists so that a module
        // reaching a player cannot be malformed.
        return Err(Error::internal(format!(
            "the linked program did not verify ({} problem(s))",
            diagnostics.len()
        )));
    }

    let path = destination.join(SCRIPTS).join(script_path(entry));
    write_file(&path, &vela_bytecode::encode(&module))
}

/// Where a bundle keeps the program an entry point starts.
///
/// `main.start` is `main.velac` and `chapters.forest.clearing` is `chapters/forest.velac` — the
/// module's path under `src/`, which is the same rule `LANGUAGE.md §6` gives a module its name by,
/// read backwards.
fn script_path(entry: &str) -> PathBuf {
    let module = entry.rsplit_once('.').map_or(entry, |(module, _)| module);
    PathBuf::from(module.replace('.', "/")).with_extension("velac")
}

/// A module's name as a screen pack spells it: its path under `src/`, extension dropped.
///
/// `chapters/forest.vela` is `chapters/forest`, which mirrors the naming `LANGUAGE.md §6` gives a
/// module and is why one pack per module is one pack per source file.
fn module_name(name: &str) -> String {
    name.strip_suffix(".vela").unwrap_or(name).to_string()
}

/// Imports the assets, ready to be written beside their manifest.
///
/// A project with no `assets/` is a project with no assets, not an error: the one-line script
/// `VISION.md §1` protects must stay buildable.
fn build_assets(project: &Project) -> Result<Built, Error> {
    let root = project
        .source_root
        .parent()
        .map(|root| root.join(ASSETS))
        .unwrap_or_default();

    if !root.is_dir() {
        return Ok(Built {
            manifest: Manifest::new(),
            artifacts: Vec::new(),
        });
    }
    import_tree(&root, &ImporterRegistry::builtin())
        .map_err(|error| Error::diagnostics(error.to_string()))
}

/// The `image` declarations, resolved to the artifact each one became.
///
/// The runtime cannot make this mapping itself: `scene bg.room` names an image and
/// `image bg.room = @"art/room.png"` says what that name is a picture of, and a bundle ships no
/// source. Recording it in the manifest is what lets a bundle stage its backgrounds without a
/// compiler (`BUILD_AND_ASSETS.md §8`).
fn image_map(project: &Project, manifest: &Manifest) -> BTreeMap<String, String> {
    use vela_syntax::{Expr, Item};

    let mut images = BTreeMap::new();
    for path in &project.files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = vela_syntax::parse(FileId::from_raw(0), &text);
        for item in &parsed.program.items {
            let Item::Image(declaration) = item else {
                continue;
            };
            let Expr::Path { value, .. } = &declaration.value else {
                continue;
            };
            if let Some(artifact) = manifest.artifact_for_source(value) {
                images.insert(declaration.name.join("."), artifact.to_string());
            }
        }
    }
    images
}

/// Compiles each source module's screens into `screens/<module>.velspk` (`SCREENS.md §13`).
///
/// A bundle that drops these is a bundle that loses the game's interface: a `dialogue` screen
/// never draws, and Escape has no `pause` to open, so the built version behaves differently from
/// `vela run` on the very things a player touches. This is where that stops being true — and it
/// stops being true *at build time*: the declarations are parsed and turned into a
/// [`vela_ui::ScreenPack`] here, so running the bundle loads a pack instead of parsing a screen.
/// Nothing about the interface is compiled at run time, which is what `BUILD_AND_ASSETS.md §1`
/// asks of a built artifact.
///
/// One pack per module, because a [`vela_ui::ScreenSet`] is one file's worth — styles resolve
/// where they are declared.
fn write_screens(project: &Project, destination: &Path) -> Result<usize, Error> {
    let mut written = 0;
    for path in &project.files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let parsed = vela_syntax::parse(FileId::from_raw(0), &text);
        let module = module_name(&module_path(&project.source_root, path));

        let pack = vela_ui::ScreenPack::compile(module.clone(), &parsed.program.items);
        if pack.is_empty() {
            // A module with no screens, styles, or theme compiles to a pack of nothing; writing
            // one would be a file that says the module has an interface when it has none.
            continue;
        }
        let out = destination.join(SCREENS).join(format!("{module}.velspk"));
        pack.write(&out)
            .map_err(|error| Error::internal(error.to_string()))?;
        written += 1;
    }
    Ok(written)
}

/// The name a launcher and a window title use: the project's, else the directory's.
fn project_name(project: &Project) -> String {
    if let Some(name) = project
        .manifest
        .as_ref()
        .and_then(|manifest| manifest.project.name.clone())
    {
        return name;
    }
    project
        .source_root
        .parent()
        .and_then(Path::file_name)
        .map_or_else(
            || "vela".to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
}

/// Writes the manifest and every artifact under `destination`.
///
/// Existing files are overwritten but not pruned, which is deliberate for now: a stale artifact
/// is invisible to everything that reads the manifest, and `--verify-reproducible` builds into
/// two fresh directories rather than trusting a reused one. Pruning belongs with the patch work,
/// which is the first thing that would care.
fn write_tree(destination: &Path, built: &Built) -> Result<(), Error> {
    for (path, bytes) in &built.artifacts {
        write_file(&destination.join(ASSETS).join(path), bytes)?;
    }

    let manifest = built
        .manifest
        .to_json()
        .map_err(|error| Error::internal(error.to_string()))?;
    write_file(&destination.join("manifest.json"), manifest.as_bytes())
}

/// Writes a file, creating its parents.
pub fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::internal(format!("cannot create {}: {e}", parent.display())))?;
    }
    fs::write(path, bytes)
        .map_err(|e| Error::internal(format!("cannot write {}: {e}", path.display())))
}
