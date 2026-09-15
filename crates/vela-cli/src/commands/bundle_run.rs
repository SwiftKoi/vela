//! Running a *built* bundle: bytecode to the machine, no compiler in the path.
//!
//! `BUILD_AND_ASSETS.md §8` and `RUNTIME.md §8`. This is what makes `vela build` produce
//! something a player runs rather than an artifact nothing consumes: a bundle directory is
//! loaded with [`vela_vm::Session::load`], which reads the entry point from the manifest and the
//! module from `scripts/`, and never parses a `.vela` file.
//!
//! # What a bundle cannot do yet
//!
//! Screens are compiled from source and a bundle ships none, so a windowed bundle run uses the
//! presenter's built-in dialogue and menu rather than a project's `screen` blocks. Backgrounds
//! *do* show, because the build records each `image` declaration's artifact in the manifest and
//! this loads them.
//!
//! A save from a bundle is written against an **empty schema**: `vela-replay` derives the schema
//! from source and a bundle has none. Its own saves agree with each other and are refused against
//! a source build; deriving the schema from the module's `default`s is the missing step. Packing
//! compiled screens is the other. Both are stated here rather than discovered by a player.

use std::io::Write;
use std::path::Path;

use vela_vm::{Host, Session, Step};

use crate::command::Error;
use crate::commands::run::{play, saves_dir, wants_a_window};
use crate::commands::target;
use crate::commands::ui::Screens;
use crate::commands::{frame, run};

/// Whether a path is a built bundle rather than a project.
///
/// A bundle is a directory with a `manifest.json`; a project is a directory with a `vela.toml`.
/// The `vela.toml` test is what keeps a project whose build happened to write into it from being
/// mistaken for its own output.
#[must_use]
pub(crate) fn is_bundle(path: &Path) -> bool {
    path.is_dir() && path.join("manifest.json").is_file() && !path.join("vela.toml").is_file()
}

/// Runs a built bundle.
///
/// # Errors
///
/// Fails if the bundle cannot be loaded, the story faults, or a requested capture cannot be
/// written.
pub(crate) fn run(dir: &Path, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
    let manifest = manifest(dir)?;
    let session = Session::load(dir).map_err(|error| Error::diagnostics(error.to_string()))?;

    // The entry point comes from the bundle, not from a flag: a bundle is a game that knows
    // where it starts. Falling back to the manifest covers a module the loader started at its
    // first label.
    let entry = session
        .entry()
        .map(str::to_string)
        .or_else(|| manifest.entry.clone())
        .unwrap_or_default();
    let title = manifest.name.clone().unwrap_or_else(|| entry.clone());

    let images = images(dir, &manifest, out);
    let bindings = bindings(dir);

    if !wants_a_window(args) {
        let commands = drive(session)?;
        if let Some(path) = run::flag_value(args, "--capture") {
            return frame::capture(&commands, path, args, out, &Screens::empty(), images);
        }
        for command in &commands {
            let _ = writeln!(out, "{command}");
        }
        return Ok(());
    }

    // The schema a save is written against. `vela-replay` derives it from source today, and a
    // bundle ships no source, so a bundle writes against an empty schema: its own saves agree
    // with each other and are *refused* against a source build rather than silently written in
    // a shape the source would not recognize. Deriving the schema from the module's `default`s
    // is the missing step, and stating it here is the difference between a gap and a bug.
    let schema = vela_replay::Schema::default();
    play(
        session.module(),
        &entry,
        &title,
        args,
        out,
        Screens::empty(),
        saves_dir(dir),
        schema,
        images,
        bindings,
    )
}

/// Reads a bundle's manifest.
fn manifest(dir: &Path) -> Result<vela_assets::Manifest, Error> {
    let path = dir.join("manifest.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|error| Error::diagnostics(format!("cannot read {}: {error}", path.display())))?;
    vela_assets::Manifest::from_json(&text).map_err(|error| Error::diagnostics(error.to_string()))
}

/// The input bindings a bundle asks for.
///
/// The target descriptor names a profile (`BUILD_AND_ASSETS.md §4`); a bundle built without
/// `--target` has none and gets the built-in defaults. An unknown profile is reported rather
/// than silently replaced, because a build that asked for one this engine does not have is a
/// mismatch worth seeing.
fn bindings(dir: &Path) -> vela_host::Bindings {
    let Some(installed) = target::read(dir) else {
        return vela_host::Bindings::default();
    };
    if let Some(profile) = vela_host::Bindings::profile(&installed.input_profile) {
        return profile;
    }
    eprintln!(
        "vela run: the {} bundle asks for an input profile `{}` this build does not have; \
         using the defaults",
        installed.target, installed.input_profile
    );
    vela_host::Bindings::default()
}

/// Decodes the pictures a bundle's images resolve to, by the name a scene stages them under.
fn images(
    dir: &Path,
    manifest: &vela_assets::Manifest,
    out: &mut dyn Write,
) -> Vec<(String, u32, u32, Vec<u8>)> {
    let mut images = Vec::new();
    for (name, artifact) in &manifest.images {
        let file = dir.join("assets").join(artifact);
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = writeln!(out, "image {name}: {error}");
                continue;
            }
        };
        match vela_assets::decode_png(&bytes) {
            Ok(image) => images.push((name.clone(), image.width, image.height, image.rgba)),
            Err(error) => {
                let _ = writeln!(out, "image {name}: {error}");
            }
        }
    }
    images
}

/// Plays a loaded session to its halt, answering the way a headless run does.
///
/// The answers are the same ones `vela_vm::run` gives with a `TakeFirst` host, which is what
/// makes a bundle's command stream identical to the same story run from source — the assertion
/// `tests/` makes.
fn drive(mut session: Session) -> Result<Vec<vela_world::Command>, Error> {
    let mut host = vela_vm::TakeFirst;
    let mut commands = Vec::new();
    let mut step = session.advance();

    loop {
        match step {
            Step::Yield(command) => {
                let answer = host.answer(&command);
                commands.push((*command).clone());
                step = session.answer(answer);
            }
            Step::Continue => step = session.advance(),
            Step::Halt => break,
            Step::Fault(fault) => return Err(Error::internal(format!("{fault}"))),
        }
    }
    Ok(commands)
}
