//! `cargo xtask budget` — measure the paths that are user-visible, and hold them to a line.
//!
//! `ARCHITECTURE.md §8` optimizes two things and calls everything else "fast enough until a
//! measurement says otherwise": **startup** and **frame time**. `RUNTIME.md §7.2` adds a
//! third with a number already attached — **the cost of a snapshot**, which is what a rollback
//! pays. This is the measurement, and the budgets in `xtask/budgets.toml` are the line.
//!
//! Three decisions worth stating, because each is a way this could be a costume rather than a
//! check:
//!
//! **It is not part of `all`.** A measurement is not a policy: `all` answers "is the tree
//! sound", this answers "is it still fast". Mixing them would make `all` take a minute and
//! fail for reasons that have nothing to do with correctness.
//!
//! **It refuses to run in a debug build.** A timed `HashMap` clone in a build with no
//! optimizer measures the absence of an optimizer, not the engine; a number like that is worse
//! than no number, because someone will quote it.
//!
//! **The budgets are set from a measured run, with headroom, and the measurement is printed
//! every time.** A budget that fails on a busy CI machine is worse than no budget: it teaches
//! people to rerun rather than to look. The startup and frame numbers are sized to catch an
//! *algorithmic* regression and ignore the noise of a shared runner; the snapshot number is
//! the spec's own, because there it is a contract rather than an observation.

use std::time::{Duration, Instant};

use crate::report::Report;

/// The measured budgets, as milliseconds.
#[derive(Debug, serde::Deserialize)]
struct Budgets {
    /// Compiling the benchmark fixture and reaching its first presented command.
    startup_ms: f64,
    /// Building one frame's draw list: layout and glyph placement.
    frame_ms: f64,
    /// How many frames to build when taking the median.
    #[serde(default = "default_frames")]
    frames: usize,
    /// Taking a snapshot of a world with `snapshot_defaults` values.
    #[serde(default = "default_snapshot_ms")]
    snapshot_ms: f64,
    /// How many `default` values the snapshot benchmark's world holds.
    #[serde(default = "default_snapshot_defaults")]
    snapshot_defaults: usize,
    /// How many snapshots to take when finding the median.
    #[serde(default = "default_snapshot_reps")]
    snapshot_reps: usize,
}

fn default_frames() -> usize {
    30
}

fn default_snapshot_ms() -> f64 {
    1.0
}

fn default_snapshot_defaults() -> usize {
    10_000
}

fn default_snapshot_reps() -> usize {
    100
}

/// Runs the measurements and judges them.
pub fn run() -> bool {
    if cfg!(debug_assertions) {
        eprintln!(
            "xtask: `budget` measures release code, and this build has its assertions on — a \
             debug measurement is the absence of an optimizer, not the engine\n  run: cargo run \
             --release -p xtask -- budget"
        );
        return false;
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent")
        .to_path_buf();

    let Some(budgets) = load_budgets(&root) else {
        return false;
    };

    let mut report = Report::pass("budget", "");
    let mut measured = 0usize;
    judge_startup(&root, &budgets, &mut report, &mut measured);
    judge_frame(&root, &budgets, &mut report, &mut measured);
    judge_snapshot(&budgets, &mut report, &mut measured);

    report.summary = format!("{measured} measurement(s) against xtask/budgets.toml");
    crate::report::print(&report);

    // A measurement that was never taken is not a pass.
    !report.failed() && measured == 3
}

/// Reads the budgets, reporting rather than panicking when they are unusable.
fn load_budgets(root: &std::path::Path) -> Option<Budgets> {
    let path = root.join("xtask/budgets.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("xtask: cannot read {}", path.display());
        return None;
    };
    match toml::from_str(&text) {
        Ok(budgets) => Some(budgets),
        Err(error) => {
            eprintln!("xtask: cannot parse {}: {error}", path.display());
            None
        }
    }
}

/// Measures startup and holds it to the line.
fn judge_startup(
    root: &std::path::Path,
    budgets: &Budgets,
    report: &mut Report,
    measured: &mut usize,
) {
    match measure_startup(root) {
        Ok(elapsed) => {
            *measured += 1;
            println!(
                "  startup {:>8.2} ms  (budget {:.2})",
                ms(elapsed),
                budgets.startup_ms
            );
            if ms(elapsed) > budgets.startup_ms {
                report.violation(format!(
                    "startup took {:.2} ms, over the {:.2} ms budget\n  \
                     rule: ARCHITECTURE.md §8 — startup is one of the two things optimized\n  \
                     fix: find what got slower, or raise the budget in xtask/budgets.toml with a reason",
                    ms(elapsed),
                    budgets.startup_ms
                ));
            }
        }
        Err(error) => report.violation(format!("startup could not be measured: {error}")),
    }
}

/// Measures a frame and holds it to the line.
fn judge_frame(
    root: &std::path::Path,
    budgets: &Budgets,
    report: &mut Report,
    measured: &mut usize,
) {
    match measure_frame(root, budgets.frames) {
        Ok(elapsed) => {
            *measured += 1;
            println!(
                "  frame   {:>8.2} ms  (median of {}, budget {:.2})",
                ms(elapsed),
                budgets.frames,
                budgets.frame_ms
            );
            if ms(elapsed) > budgets.frame_ms {
                report.violation(format!(
                    "a frame took {:.2} ms, over the {:.2} ms budget\n  \
                     rule: ARCHITECTURE.md §8 — layout and atlas work is cached, not per-frame\n  \
                     fix: find what stopped being cached, or raise the budget with a reason",
                    ms(elapsed),
                    budgets.frame_ms
                ));
            }
        }
        Err(error) => report.violation(format!("frame time could not be measured: {error}")),
    }
}

/// Measures a snapshot and holds it to the line.
///
/// `RUNTIME.md §7.2`: *"snapshot must not exceed 1 ms for a project with 10k `default`
/// values."* Unlike the other two this has no error path — a snapshot is a copy in memory, so
/// there is no fixture to read and nothing to fail — and the budget is the spec's own number
/// rather than one derived from a measurement.
fn judge_snapshot(budgets: &Budgets, report: &mut Report, measured: &mut usize) {
    let elapsed = measure_snapshot(budgets.snapshot_defaults, budgets.snapshot_reps);
    *measured += 1;
    println!(
        "  snapshot{:>7.2} ms  (median of {}, {} defaults, budget {:.2})",
        ms(elapsed),
        budgets.snapshot_reps,
        budgets.snapshot_defaults,
        budgets.snapshot_ms
    );
    if ms(elapsed) > budgets.snapshot_ms {
        report.violation(format!(
            "a snapshot took {:.2} ms, over the {:.2} ms budget\n  \
             rule: RUNTIME.md §7.2 — a snapshot is a copy of plain data, not a serialization\n  \
             fix: find what started allocating per value, or raise the budget with a reason",
            ms(elapsed),
            budgets.snapshot_ms
        ));
    }
}

/// Milliseconds, as a float.
fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Times a snapshot of a world with `defaults` values, and returns the median.
///
/// The world is the fixture the spec names — ten thousand `default` values — and the machine
/// beside it is a realistic short call stack, because a snapshot copies both. The clone is
/// exactly what `Session::snapshot` does to the *world*; the machine's translation to named
/// frames is a small fixed cost on top, and is what the small frame stack stands in for.
///
/// `black_box` is load-bearing: without it the optimizer is free to notice the copy is never
/// read and delete it, and the benchmark would report the cost of a loop.
fn measure_snapshot(defaults: usize, reps: usize) -> Duration {
    let mut world = vela_world::World::new();
    for index in 0..defaults {
        world.set(
            format!("state_{index}"),
            vela_world::Value::Int(index as i64),
        );
    }

    let snapshot = vela_vm::Snapshot {
        world,
        vm: vela_vm::VmState {
            frames: (0..8)
                .map(|depth| vela_vm::FrameState {
                    body: format!("label:scene_{depth}"),
                    ip: depth as u32,
                    base: depth * 4,
                    // Only the frame on top is suspended at a statement; the ones under it
                    // are callers waiting on it, which is what a real stack looks like.
                    resume: (depth == 7).then_some(vela_vm::Resume {
                        start: 90,
                        end: 120,
                    }),
                })
                .collect(),
            stack: vec![vela_world::Value::None; 32],
            pending: None,
            finished: false,
        },
        current: Some(vela_world::Command::Say {
            speaker: Some("Eileen".to_string()),
            attributes: Vec::new(),
            text: "The rain had stopped.".to_string(),
            options: Vec::new(),
            transition: None,
        }),
    };

    // One warm-up copy, so the first allocation's page faults are not the measurement.
    std::hint::black_box(&snapshot.clone());

    let mut timings = Vec::with_capacity(reps.max(1));
    for _ in 0..reps.max(1) {
        let start = Instant::now();
        let copy = snapshot.clone();
        timings.push(start.elapsed());
        std::hint::black_box(&copy);
    }
    timings.sort();
    timings[timings.len() / 2]
}

/// Compiles the benchmark fixture and runs it to its first presented command.
///
/// This is the *whole* launch path, not a slice of it: reading files, the front end, MIR,
/// bytecode, verification, and the VM reaching its first suspension. Timing one stage in
/// isolation would measure something no user experiences.
fn measure_startup(root: &std::path::Path) -> Result<Duration, String> {
    let fixture = root.join("examples/hello");
    let start = Instant::now();

    let files = collect_source(&fixture.join("src"))?;
    let mut session = vela_compile::Session::new();
    let mut file = None;
    for (name, text) in &files {
        let id = session.set_file(name.clone(), text.clone());
        file.get_or_insert(id);
    }

    let file = file.ok_or_else(|| "the fixture has no modules".to_string())?;
    let module = session.mir(file).module.clone();
    let compiled = vela_bytecode::compile(&module, true);
    let diagnostics = vela_bytecode::verify(&compiled);
    if !diagnostics.is_empty() {
        return Err(format!(
            "the fixture does not verify ({} problem(s))",
            diagnostics.len()
        ));
    }
    let mut vm_session =
        vela_vm::Session::start(&compiled, "start").map_err(|fault| format!("{fault}"))?;
    let _ = vm_session.advance();

    Ok(start.elapsed())
}

/// Builds one frame's draw list, repeatedly, and returns the median.
///
/// The median rather than the mean: one scheduling hiccup on a shared runner moves an average
/// and does not move a median, and this is looking for a consistent cost rather than a spike.
fn measure_frame(root: &std::path::Path, frames: usize) -> Result<Duration, String> {
    let face = root.join("assets/fonts/LiberationSans-Regular.ttf");
    let bytes = std::fs::read(&face).map_err(|error| format!("{}: {error}", face.display()))?;
    let font = vela_text::Font::from_bytes(bytes, 0).ok_or("the fixture font did not load")?;

    let mut text = vela_text::TextEngine::new();
    text.add_font("sans", font);
    let mut presenter = vela_render::Presenter::new(text, "sans", (1280, 720));

    // A realistic frame: a speaker, two wrapped lines, and something on stage.
    presenter.apply(&vela_world::Command::Stage {
        kind: vela_world::Stage::Scene,
        image: "bg.street".to_string(),
        attributes: Vec::new(),
        transforms: Vec::new(),
        transition: None,
    });
    presenter.apply(&vela_world::Command::Say {
        speaker: Some("Eileen".to_string()),
        attributes: Vec::new(),
        text: "The rain had stopped an hour ago, but the street still shone, and nobody had \
               come to close the shutters."
            .to_string(),
        options: Vec::new(),
        transition: None,
    });

    // One warm-up frame: the first build rasterises every glyph, and a budget that included
    // that would be measuring font loading rather than per-frame work.
    let mut warm = vela_render::DrawList::new();
    presenter.build(&mut warm);

    let mut timings = Vec::with_capacity(frames);
    for _ in 0..frames.max(1) {
        let start = Instant::now();
        let mut draw = vela_render::DrawList::new();
        presenter.build(&mut draw);
        timings.push(start.elapsed());
    }
    timings.sort();
    Ok(timings[timings.len() / 2])
}

/// Every `.vela` file under `dir`, as `(name, source)`.
///
/// The file's stem is the module name, which is what the compiler's own loader does — a
/// benchmark that named modules differently from the real one would be measuring a different
/// program.
fn collect_source(dir: &std::path::Path) -> Result<Vec<(String, String)>, String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "vela"))
        .collect();
    entries.sort();

    let mut files = Vec::new();
    for path in &entries {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("{}: no usable name", path.display()))?;
        files.push((name.to_string(), text));
    }
    if files.is_empty() {
        return Err(format!("no `.vela` files under {}", dir.display()));
    }
    Ok(files)
}
