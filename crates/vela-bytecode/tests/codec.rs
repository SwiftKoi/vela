//! Tests for the `.velac` container.
//!
//! Two things are being established. The first is that a module survives a round trip —
//! `decode(encode(m)) == m` — which is what makes the container a *transport* rather than a
//! second representation that could drift from the in-memory one. The second is that a
//! container which is not a module is rejected rather than misread, because the bytes come
//! off a disk or a network and a loader that trusts them is a loader that crashes.

use std::fs;
use std::path::{Path, PathBuf};

use vela_bytecode::{DecodeError, Module, decode, encode, known_commands, verify};

/// Where the historical-format goldens live.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/velac")
}

/// The MIR corpus, whose entries are the compiler's own output.
fn mir_corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/mir")
}

/// Whether to write goldens rather than compare against them.
fn blessing() -> bool {
    std::env::var_os("VELA_BLESS").is_some()
}

/// The whole pipeline: source text to a compiled module.
fn compiled(name: &str, text: &str) -> Module {
    let mut sources = vela_span::SourceMap::new();
    let id = sources.add(name, text);
    let parsed = vela_syntax::parse(id, text);
    let (env, _) = vela_types::Env::build(&parsed.program);
    let lowered = vela_mir::lower(&vela_hir::ModuleName::new(name), &parsed.program, &env);
    vela_bytecode::compile(&lowered.module, true)
}

/// Every entry in the MIR corpus, compiled.
fn corpus_modules() -> Vec<(String, Module)> {
    let mut entries: Vec<PathBuf> = fs::read_dir(mir_corpus())
        .expect("read the MIR corpus")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "vela"))
        .collect();
    entries.sort();

    entries
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let text = fs::read_to_string(&path).expect("read an entry");
            (name.clone(), compiled(&name, &text))
        })
        .collect()
}

/// A module survives being written and read back.
#[test]
fn the_corpus_round_trips() {
    let modules = corpus_modules();
    assert!(modules.len() >= 10, "only {} entries", modules.len());

    for (name, module) in modules {
        let bytes = encode(&module);
        let decoded = decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert_eq!(decoded, module, "{name} did not survive the round trip");
    }
}

/// The bytes are stable: encoding twice gives the same container.
///
/// Worth pinning separately from the round trip, because a round trip through a
/// nondeterministic encoder would still pass — and a build artifact that changes without
/// changing is a build artifact nobody can cache.
#[test]
fn encoding_is_deterministic() {
    for (name, module) in corpus_modules() {
        assert_eq!(encode(&module), encode(&module), "{name} encoded twice");
    }
}

/// A container cut short is rejected rather than half-read.
#[test]
fn a_truncated_container_is_rejected() {
    let module = compiled("hello", "label start:\n    \"Hi.\"\n    return\n");
    let bytes = encode(&module);

    for cut in [0, 4, 20, bytes.len() - 1] {
        let error = decode(&bytes[..cut]).expect_err("a truncated container was accepted");
        assert!(
            matches!(error, DecodeError::Corrupt(_)),
            "byte {cut} gave {error:?}"
        );
    }
}

/// A byte flipped anywhere changes the checksum.
#[test]
fn a_corrupted_byte_is_rejected() {
    let module = compiled("hello", "label start:\n    \"Hi.\"\n    return\n");
    let bytes = encode(&module);

    let mut damaged = bytes.clone();
    let middle = damaged.len() / 2;
    damaged[middle] ^= 0xff;

    match decode(&damaged) {
        Err(DecodeError::Corrupt(_)) => {}
        other => panic!("a corrupted container gave {other:?}"),
    }
}

/// A newer format is refused by name, so the message can say "rebuild".
#[test]
fn a_newer_format_is_rejected() {
    let module = compiled("hello", "label start:\n    \"Hi.\"\n    return\n");
    let mut bytes = encode(&module);

    // The format is the two bytes after the magic.
    bytes[4..6].copy_from_slice(&99u16.to_le_bytes());

    assert_eq!(decode(&bytes), Err(DecodeError::UnknownFormat(99)));
}

/// A module that uses a command this build does not know is reported, not run.
///
/// This is the mechanism behind "adding a command variant requires no change to the
/// interpreter": known variants are a table, and both sides read the same one. The variant
/// below is test-only — it is not in `CommandKind` — and the check finds it without
/// anything else in this crate being told about it.
#[test]
fn an_unknown_command_variant_is_reported() {
    let mut module = compiled("hello", "label start:\n    \"Hi.\"\n    return\n");
    assert!(
        known_commands(&module).is_empty(),
        "the corpus uses a command this build does not know"
    );

    let name = module.strings.add("test.only.variant");
    module.cmds.push(vela_bytecode::CommandSchema {
        name,
        fields: Vec::new(),
    });

    let diagnostics = known_commands(&module);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code.as_str(), "E7102");
}

/// Every container this project has ever written still loads.
///
/// `BYTECODE.md §6`: forward compatibility is a non-goal, backward compatibility is
/// *required and tested*. There is one historical format so far, and this is it — the point
/// of the test is that it keeps working when there are two, and a new format added without
/// a golden fails here rather than in someone's shipped build.
#[test]
fn the_historical_goldens_still_load() {
    let directory = golden_dir();
    let entries = fs::read_dir(&directory).unwrap_or_else(|e| {
        panic!("cannot read {}: {e}", directory.display());
    });

    let mut goldens: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "velac"))
        .collect();
    goldens.sort();
    assert!(!goldens.is_empty(), "no golden containers");

    for golden in &goldens {
        let bytes = fs::read(golden).expect("read a golden");
        let module = decode(&bytes).unwrap_or_else(|e| panic!("{}: {e}", golden.display()));

        assert_eq!(
            module.header.format,
            vela_bytecode::FORMAT,
            "{} is a different format",
            golden.display()
        );
        assert!(
            verify(&module).is_empty(),
            "{} does not verify",
            golden.display()
        );
    }
}

/// Writes the golden container for the current format, when blessing.
#[test]
fn the_golden_container_matches() {
    let module = compiled("hello", "label start:\n    \"Hi.\"\n    return\n");
    let bytes = encode(&module);

    let directory = golden_dir();
    let golden = directory.join(format!("hello-v{}.velac", vela_bytecode::FORMAT));

    if blessing() {
        fs::create_dir_all(&directory).expect("create the golden directory");
        fs::write(&golden, &bytes).expect("write the golden");
        return;
    }

    let expected = fs::read(&golden).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e} — run `VELA_BLESS=1 cargo test -p vela-bytecode --test codec`",
            golden.display()
        )
    });

    assert_eq!(
        bytes, expected,
        "the container changed — bless it only if the format change was intended"
    );
}
