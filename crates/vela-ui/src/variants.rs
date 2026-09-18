//! Which variants apply where a screen is running (`SCREENS.md §2.6`).
//!
//! A variant is a *name for the place* a screen is drawn in: which platform the bundle was built for,
//! and whether the frame being laid out has room. Ren'Py's `config.variants` is the same idea
//! (`renpy/main.py` builds it from what its launcher passes plus the size classes its platform modules
//! derive), and it is a list of strings there so that a plugin can add one.
//!
//! **The vocabulary here is closed**, for the reason every vocabulary in `vela-ui` is: a screen asking
//! `variant("pcc")` must not get a condition that is quietly false. Ren'Py answers `False` for any name
//! it does not have; Vela reports `E5018` at the call, and the four names below are the ones it can
//! actually answer. A name Ren'Py has and Vela does not (`tablet`, `touch`, `tv`, `chromeos`) is
//! therefore an error rather than a silent false — staged work stated out loud, which is the point.
//!
//! **Two sources, one set.** The platform names come from the bundle's descriptor (`target.json`'s
//! `variants`, written per target by `commands/target.rs`), and the size class comes from the frame the
//! screen is laid out in, which [`lay`](crate::ScreenSet::lay) already receives. Ren'Py decides `small`
//! from the *physical diagonal* of the device; Vela's is the room the frame has, because that is the
//! question a screen is actually asking — the sample guards its side image with `not renpy.variant
//! ("small")` and says why: "there's no room" — and because a desktop build has no diagonal to measure.

/// The name a screen asks which variants apply with.
pub const VARIANT: &str = "variant";

/// Whether a call is a screen's *question* rather than an action (`SCREENS.md §2.6`, `§7`).
///
/// One name today, and a table rather than a comparison at each site because four rules ask: the
/// evaluator (a question has an answer, not an action), the checker (a question's name is checked
/// against the vocabulary, and is not an unknown action), `W4013` (a known question *is* decidable) and
/// the answer itself.
#[must_use]
pub fn is_question(name: &str) -> bool {
    name == VARIANT
}

/// A name a screen may ask about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// A desktop target: `linux`, `win` or `mac` (`commands/target.rs`).
    Pc,
    /// The `web` target.
    Web,
    /// A phone or tablet target. A descriptor may declare it; no target emits it yet.
    Mobile,
    /// The frame has less room than the reference (see [`Variants::for_frame`]).
    Small,
}

impl Variant {
    /// Every name the engine knows, in the order the reference lists them.
    pub const ALL: [Variant; 4] = [Variant::Pc, Variant::Web, Variant::Mobile, Variant::Small];

    /// The name a screen writes, and what a descriptor carries.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Variant::Pc => "pc",
            Variant::Web => "web",
            Variant::Mobile => "mobile",
            Variant::Small => "small",
        }
    }

    /// The variant a name stands for, if it is one the engine knows.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Variant::ALL.into_iter().find(|v| v.name() == name)
    }
}

/// The frame Vela's own styles, layout tests and `--capture` default are written against.
///
/// A project declares its own (`vela.toml`'s `[project] size`, which is where Ren'Py's
/// `gui.init(1280, 720)` migrates to); this is what a project that declares none is designed for,
/// and therefore also what "three quarters" below is three quarters *of* by default.
pub const REFERENCE_FRAME: (f32, f32) = (1280.0, 720.0);

/// How much of the design frame a frame has to fall below to be `small`.
///
/// # How the size class is decided
///
/// A frame is `small` when either side is below three quarters of the frame the game was
/// *designed* for — 960×540 against Vela's default 1280×720. The question is "has this been
/// shrunk below what the screens were laid out for" rather than "is this a phone", because that is
/// the question a screen is actually asking: the sample guards its side image with
/// `not renpy.variant("small")` and says why — "there's no room". Ren'Py decides it from the
/// device's *physical diagonal*, which a desktop build cannot measure; Vela owns this rule and
/// states it here rather than deriving it from something unmeasurable.
const SMALL_FRACTION: f32 = 0.75;

/// A set of variants.
///
/// A set rather than a list, because a screen asks "is this one of them" and never "which came first" —
/// and because the two sources are merged before a screen sees them, so there is no order to preserve.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Variants {
    set: u8,
}

impl Variants {
    /// No variants apply.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The set a bundle's descriptor declares, and the names it asked for that this build cannot answer.
    ///
    /// The second half is returned rather than refused, and the *caller* says so out loud — the same
    /// shape the input profile takes (`commands/target.rs`), because the two are the same situation: a
    /// bundle built by something that knows more than this engine does. Nothing is drawn wrongly by
    /// ignoring it either, since a screen cannot ask about a name outside this vocabulary (`E5018`).
    pub fn declared<I, S>(names: I) -> (Self, Vec<String>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = Self::new();
        let mut unknown = Vec::new();
        for name in names {
            let name = name.as_ref();
            match Variant::named(name) {
                Some(variant) => set.add(variant),
                None => unknown.push(name.to_string()),
            }
        }
        (set, unknown)
    }

    /// Adds a variant to the set.
    pub fn add(&mut self, variant: Variant) {
        self.set |= 1 << variant as u8;
    }

    /// This set with a variant added.
    #[must_use]
    pub fn with(mut self, variant: Variant) -> Self {
        self.add(variant);
        self
    }

    /// Whether a variant applies.
    #[must_use]
    pub fn has(self, variant: Variant) -> bool {
        self.set & (1 << variant as u8) != 0
    }

    /// This set, plus the size class a frame has (`SCREENS.md §2.6`).
    ///
    /// `design` is the frame the game was built for — `vela.toml`'s `[project] size`, which a runner
    /// passes in beside this set — and the frame is the only source of `small`. It is added here
    /// rather than by the caller because the caller that has a frame is the one laying the screen
    /// out: a runner that had to remember to classify its own frame could forget, and a screen that
    /// asked would silently draw the desktop shape on a phone.
    #[must_use]
    pub fn for_frame(self, width: f32, height: f32, design: (f32, f32)) -> Self {
        if width < design.0 * SMALL_FRACTION || height < design.1 * SMALL_FRACTION {
            self.with(Variant::Small)
        } else {
            self
        }
    }

    /// The names in this set, in the engine's order.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        Variant::ALL
            .into_iter()
            .filter(|v| self.has(*v))
            .map(Variant::name)
            .collect()
    }
}
