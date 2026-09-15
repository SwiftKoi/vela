//! Helpers shared by the CLI's test modules.

use crate::run_code;

/// Runs the CLI, returning the exit code and captured output.
pub(super) fn cli(args: &[&str]) -> (u8, String) {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = run_code(&args, &mut out, &mut err);

    // Errors go to stderr so they cannot land inside a document a tool parses; the tests
    // read the two together, because a user does.
    out.extend_from_slice(&err);
    (code, String::from_utf8_lossy(&out).into_owned())
}

/// Writes a one-file project and returns its directory.
pub(super) fn temp_project(name: &str, source: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("vela-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("src")).expect("create project");
    std::fs::write(
        base.join("vela.toml"),
        "schema = 1\n\n[project]\nname = \"t\"\nversion = \"0.1.0\"\nentry = \"main.start\"\n",
    )
    .expect("write manifest");
    std::fs::write(base.join("src").join("main.vela"), source).expect("write source");
    base
}

/// The bundled face, for a test that has to lay a screen out.
pub(super) fn text_engine() -> vela_text::TextEngine {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font =
        vela_text::Font::from_bytes(std::fs::read(path).expect("read font"), 0).expect("load font");
    let mut text = vela_text::TextEngine::new();
    text.add_font("sans", font);
    text
}
