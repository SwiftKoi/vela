//! Target drivers: what `vela build --target` selects.
//!
//! `BUILD_AND_ASSETS.md §4`: one command, one source tree, many targets. A target changes
//! (a) which artifact variants are packed, (b) the shader backend, (c) the input profile
//! defaults, and (d) the signing/packaging step — and **never game logic**. The VM, the World,
//! and the bytecode are target-independent by construction (`REPO_LAYOUT.md §1`), which is why a
//! target bundle differs in its *descriptor* and its *launcher* and not in its story.
//!
//! # What is genuinely different, and what is not
//!
//! A flag that produced the same bytes for every target would be cosmetic, so this file is
//! explicit about the four dimensions §4 names — including the two that are not live yet:
//!
//! * **(a) variants** — **not implemented.** The descriptor records an empty variant set,
//!   because there is nothing to record: no importer emits a variant. §3.1's `ktx2` and `ogg`
//!   need transcoders this build does not have, and the manifest's `Variant` carries a digest
//!   with no path beside it, so a selector could not name a file to pack even if a variant
//!   existed. Every target packs the default artifact.
//! * **(b) backend** — recorded per target (`vulkan`, `metal`, `dx12`, `webgpu-webgl2`). The
//!   renderer does not yet choose a backend from it; the bundle run is headless or uses the
//!   built-in presenter.
//! * **(c) input profile** — recorded, and *consumed*: `vela run <bundle>` reads it and builds
//!   the bindings from it. The four targets shipped here are all pointer-and-keyboard platforms
//!   and share one profile; a touch profile arrives with android, which is not built yet.
//! * **(d) packaging** — a declared hook, per §8, never executed here: signing wants
//!   credentials, and those never enter the project tree.
//!
//! So the honest claim is: the descriptor and the launcher are per target and real, and *that*
//! is what makes `--target` more than cosmetic; (a) and (b) are not yet consumed, (c) is, and
//! (d) is a declared hook by design. Nothing here fakes a difference the engine does not have.

use std::fs;
use std::path::Path;

use crate::command::Error;

/// How a target's launcher is written on disk.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Launcher {
    /// A POSIX shell script.
    Shell,
    /// A Windows batch file.
    Batch,
    /// An HTML page for the web target.
    Page,
}

/// A platform a bundle can be built for.
pub struct Target {
    /// The name `--target` takes.
    name: &'static str,
    /// The rendering backend (`wgpu`'s).
    backend: &'static str,
    /// The default input profile.
    input_profile: &'static str,
    /// The packaging hook of §8.
    packaging: &'static str,
    /// The launcher file written beside the bundle.
    launcher: &'static str,
    /// How that launcher is written.
    shape: Launcher,
}

/// Every target, in the order `BUILD_AND_ASSETS.md §4` lists them.
///
/// A table rather than a `match`: adding `android` is a row here and nothing else
/// (`CONVENTIONS.md §1`).
pub const TARGETS: &[Target] = &[
    Target {
        name: "win",
        backend: "dx12",
        input_profile: "keyboard-mouse",
        packaging: "codesign",
        launcher: "launch.cmd",
        shape: Launcher::Batch,
    },
    Target {
        name: "mac",
        backend: "metal",
        input_profile: "keyboard-mouse",
        packaging: "notarize",
        launcher: "launch.sh",
        shape: Launcher::Shell,
    },
    Target {
        name: "linux",
        backend: "vulkan",
        input_profile: "keyboard-mouse",
        packaging: "appimage",
        launcher: "launch.sh",
        shape: Launcher::Shell,
    },
    Target {
        name: "web",
        backend: "webgpu-webgl2",
        input_profile: "keyboard-mouse",
        packaging: "browser",
        launcher: "index.html",
        shape: Launcher::Page,
    },
];

/// The target with this name, if it is one.
#[must_use]
pub fn find(name: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|target| target.name == name)
}

/// A built bundle's descriptor, read back.
#[derive(serde::Deserialize)]
pub struct Installed {
    /// The target it was built for.
    pub target: String,
    /// The input profile `vela run <bundle>` installs.
    pub input_profile: String,
}

/// The descriptor a built bundle carries, if it has one.
///
/// A bundle built without `--target` has no descriptor — it is the plain, target-independent
/// core — and gets the host's defaults rather than a target's.
#[must_use]
pub fn read(dir: &Path) -> Option<Installed> {
    let text = fs::read_to_string(dir.join("target.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every target name, for a usage message.
#[must_use]
pub fn names() -> Vec<&'static str> {
    TARGETS.iter().map(|target| target.name).collect()
}

impl Target {
    /// The name `--target` takes.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The target descriptor, written beside the bundle.
    ///
    /// JSON, and pretty-printed, because it is read by a person debugging a build as often as by
    /// the runtime. Field order is fixed by the struct, so two builds write the same bytes.
    fn descriptor(&self, name: &str, entry: &str) -> Result<String, Error> {
        #[derive(serde::Serialize)]
        struct Descriptor<'a> {
            target_version: u32,
            target: &'a str,
            name: &'a str,
            entry: &'a str,
            backend: &'a str,
            input_profile: &'a str,
            packaging: &'a str,
            variants: [&'a str; 0],
        }

        let descriptor = Descriptor {
            target_version: 1,
            target: self.name,
            name,
            entry,
            backend: self.backend,
            input_profile: self.input_profile,
            packaging: self.packaging,
            variants: [],
        };
        let text = serde_json::to_string_pretty(&descriptor).map_err(|error| {
            Error::internal(format!("cannot write a target descriptor: {error}"))
        })?;
        Ok(format!("{text}\n"))
    }

    /// Writes this target's descriptor and launcher into `dir`.
    ///
    /// The bundle's own files — manifest, scripts, assets — are written by `vela build` first;
    /// this adds the two things that make the directory a *distribution* rather than a build
    /// artifact (`BUILD_AND_ASSETS.md §8`).
    ///
    /// # Errors
    ///
    /// Fails if either file cannot be written.
    pub fn write(&self, dir: &Path, name: &str, entry: &str) -> Result<(), Error> {
        write_file(
            &dir.join("target.json"),
            self.descriptor(name, entry)?.as_bytes(),
        )?;
        let launcher = self.launcher_text(name, entry);
        let path = dir.join(self.launcher);
        write_file(&path, launcher.as_bytes())?;
        make_executable(&path, self.shape);
        Ok(())
    }

    /// The launcher a player starts.
    fn launcher_text(&self, name: &str, entry: &str) -> String {
        match self.shape {
            Launcher::Shell => shell_launcher(self.name, name),
            Launcher::Batch => batch_launcher(self.name, name),
            Launcher::Page => page_launcher(name, entry),
        }
    }
}

/// A POSIX launcher.
///
/// The engine is looked for beside the script first, then on `PATH`: a release copies the runtime
/// in, and a development tree falls back to the `vela` a developer already has.
fn shell_launcher(target: &str, name: &str) -> String {
    format!(
        "#!/bin/sh\n\
         # {title} — Vela launcher for the {target} target.\n\
         #\n\
         # Assets + bytecode + the engine, not a platform installer (`BUILD_AND_ASSETS.md §8`).\n\
         # The engine lives beside this script in a release build; `vela` on PATH is the\n\
         # development fallback.\n\
         set -e\n\
         here=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n\
         if [ -x \"$here/vela\" ]; then\n\
         \x20   exec \"$here/vela\" run \"$here\" \"$@\"\n\
         fi\n\
         exec vela run \"$here\" \"$@\"\n",
        title = name,
    )
}

/// A Windows launcher.
fn batch_launcher(target: &str, name: &str) -> String {
    format!(
        "@echo off\r\n\
         rem {title} - Vela launcher for the {target} target.\r\n\
         rem\r\n\
         rem Assets + bytecode + the engine, not a platform installer (`BUILD_AND_ASSETS.md §8`).\r\n\
         rem The engine lives beside this script in a release build; `vela` on PATH is the\r\n\
         rem development fallback.\r\n\
         setlocal\r\n\
         set \"here=%~dp0\"\r\n\
         if exist \"%here%vela.exe\" (\r\n\
         \x20   \"%here%vela.exe\" run \"%here%\" %*\r\n\
         ) else (\r\n\
         \x20   vela run \"%here%\" %*\r\n\
         )\r\n",
        title = name,
    )
}

/// The web launcher: a page that plays the bundle.
///
/// It fetches `manifest.json` for the entry point and the module, then drives
/// `vela_web::Player`. The engine glue (`vela_web.js`) is produced by the wasm build — a browser
/// target's engine is a separate artifact from its assets, exactly as the native engine is a
/// separate binary — and is not written by `vela build`, which has no toolchain in it.
fn page_launcher(name: &str, _entry: &str) -> String {
    // Braces that belong to JavaScript are doubled; `${module}` likewise.
    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         \x20 <meta charset=\"utf-8\">\n\
         \x20 <title>{title}</title>\n\
         </head>\n\
         <body>\n\
         <pre id=\"stage\"></pre>\n\
         <script type=\"module\">\n\
         // Vela launcher for the web target (`BUILD_AND_ASSETS.md §5`, §8).\n\
         //\n\
         // The engine is `./vela_web.js`, built with `wasm-bindgen --target web`. It is not\n\
         \x20// written by `vela build`: a browser target's engine is a separate artifact, the same\n\
         \x20// way the native engine is a separate binary. Copy it beside this page before serving.\n\
         import init, {{ Player }} from \"./vela_web.js\";\n\
         \n\
         const stage = document.getElementById(\"stage\");\n\
         const manifest = await (await fetch(\"./manifest.json\")).json();\n\
         const dot = manifest.entry.lastIndexOf(\".\");\n\
         const module = manifest.entry.slice(0, dot);\n\
         const label = manifest.entry.slice(dot + 1);\n\
         const response = await fetch(\"./scripts/\" + module + \".velac\");\n\
         const bytes = new Uint8Array(await response.arrayBuffer());\n\
         \n\
         await init();\n\
         const player = new Player(bytes, label);\n\
         let line = player.step();\n\
         while (line !== \"\") {{\n\
         \x20 stage.textContent += line + \"\\n\";\n\
         \x20 line = line.startsWith(\"menu\") ? player.choose(0) : player.line();\n\
         }}\n\
         </script>\n\
         </body>\n\
         </html>\n",
        title = escape_html(name),
    )
}

/// Escapes the few characters that must not appear raw in an HTML title.
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(character),
        }
    }
    out
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

/// Makes a launcher executable, where that concept exists.
///
/// A no-op on Windows and for the web page: the mode bits mean nothing there, and setting them
/// would be a lie about what the file is.
fn make_executable(path: &Path, shape: Launcher) {
    if shape != Launcher::Shell {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            let _ = fs::set_permissions(path, permissions);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}
