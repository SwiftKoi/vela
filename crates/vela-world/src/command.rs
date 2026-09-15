//! Presentation commands.
//!
//! A `Command` is the *only* thing the runtime tells the outside world, and the boundary
//! is load-bearing (`ARCHITECTURE.md §7`, decision D7). The VM builds a command and
//! suspends; the host presents it and acknowledges. Nothing calls into a renderer, which
//! is what makes headless testing, deterministic replay, and save/restore the same
//! mechanism rather than three subsystems.
//!
//! The vocabulary lives here, at rank 1, because the compiler constructs these values when
//! it lowers `say` and `show`, and because a command is close kin to the scene state it
//! leaves behind — both are presentation described as plain data.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::value::Value;

/// Which staging statement a command carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Stage {
    /// `scene` — replace the scene.
    Scene,
    /// `show` — add to it.
    Show,
    /// `hide` — remove from it.
    Hide,
}

impl Stage {
    /// The keyword, for rendering.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Show => "show",
            Self::Hide => "hide",
        }
    }
}

/// Which audio statement a command carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Audio {
    /// `play`
    Play,
    /// `stop`
    Stop,
    /// `queue`
    Queue,
}

impl Audio {
    /// The keyword, for rendering.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Stop => "stop",
            Self::Queue => "queue",
        }
    }
}

/// Which command a construction site builds.
///
/// A flat tag rather than the structured `Command` it produces. The compiler emits the
/// *shape* of a command plus a list of argument slots, and at that point it knows which
/// shape it is emitting but not what the arguments will evaluate to. This is also the key
/// the command's field schema is registered under (`BYTECODE.md §3.3`), so adding a
/// command is a schema registration rather than a new type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandKind {
    /// Dialogue or narration.
    Say,
    /// A choice.
    Menu,
    /// `scene`
    Scene,
    /// `show`
    Show,
    /// `hide`
    Hide,
    /// A standalone transition.
    Transition,
    /// `play`
    Play,
    /// `stop`
    Stop,
    /// `queue`
    Queue,
    /// Wait for a duration or a click.
    Pause,
    /// Wait for a click.
    WaitClick,
}

impl CommandKind {
    /// The fields the variant takes, in the order `Cmd` pushes them.
    ///
    /// The *names* live here, with the vocabulary; the *types* are derived at codegen from
    /// the arguments themselves. Adding a command variant is therefore an entry in this
    /// table and a case in `Command` — not a change to the code generator, the verifier, or
    /// the interpreter, all of which read the schema the module carries.
    #[must_use]
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Say => &["speaker", "attributes", "text", "options", "transition"],
            Self::Menu => &["prompt", "choices", "enabled"],
            Self::Scene | Self::Show | Self::Hide => {
                &["image", "attributes", "transforms", "transition"]
            }
            Self::Transition => &["name"],
            Self::Play | Self::Queue => &["channel", "source", "looping", "fade"],
            Self::Stop => &["channel", "source", "looping", "fade"],
            Self::Pause => &["seconds"],
            Self::WaitClick => &[],
        }
    }

    /// Every variant, so the schema table can be checked exhaustively.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Say,
            Self::Menu,
            Self::Scene,
            Self::Show,
            Self::Hide,
            Self::Transition,
            Self::Play,
            Self::Stop,
            Self::Queue,
            Self::Pause,
            Self::WaitClick,
        ]
    }

    /// The kind's name, which is its schema key.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Say => "say",
            Self::Menu => "menu",
            Self::Scene => "scene",
            Self::Show => "show",
            Self::Hide => "hide",
            Self::Transition => "with",
            Self::Play => "play",
            Self::Stop => "stop",
            Self::Queue => "queue",
            Self::Pause => "pause",
            Self::WaitClick => "wait_click",
        }
    }
}

/// One option a menu offers.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Choice {
    /// The choice's position in the menu, which is what a host reports back.
    pub index: usize,
    /// The text shown.
    pub text: String,
}

/// A presentation directive.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Command {
    /// Dialogue or narration.
    Say {
        /// The character speaking, if one is.
        speaker: Option<String>,
        /// Image attributes selecting a variant.
        attributes: Vec<String>,
        /// The line, already interpolated.
        text: String,
        /// Say-scoped options, e.g. `volume`.
        options: Vec<(String, Value)>,
        /// A transition applied on advance.
        transition: Option<String>,
    },
    /// A choice, which the host answers with an index.
    Menu {
        /// The prompt above the choices, if there is one.
        prompt: Option<String>,
        /// The choices offered.
        choices: Vec<Choice>,
    },
    /// Staging an image.
    Stage {
        /// Which of the three it is.
        kind: Stage,
        /// The image, as a dotted path.
        image: String,
        /// Image attributes.
        attributes: Vec<String>,
        /// Transforms applied.
        transforms: Vec<String>,
        /// A transition applied.
        transition: Option<String>,
    },
    /// A standalone transition.
    Transition {
        /// The transition's name.
        name: String,
    },
    /// Audio control.
    Audio {
        /// Which of the three it is.
        kind: Audio,
        /// The channel.
        channel: String,
        /// The source, for `play` and `queue`.
        source: Option<String>,
        /// Whether the sound loops.
        looping: bool,
        /// A fade duration.
        fade: Option<f64>,
    },
    /// Wait for a duration, or until the player advances.
    Pause {
        /// The duration, if one was given.
        seconds: Option<f64>,
    },
    /// Wait for a click.
    WaitClick,
}

impl Command {
    /// Which shape this command is.
    #[must_use]
    pub fn kind(&self) -> CommandKind {
        match self {
            Self::Say { .. } => CommandKind::Say,
            Self::Menu { .. } => CommandKind::Menu,
            Self::Stage {
                kind: Stage::Scene, ..
            } => CommandKind::Scene,
            Self::Stage {
                kind: Stage::Show, ..
            } => CommandKind::Show,
            Self::Stage {
                kind: Stage::Hide, ..
            } => CommandKind::Hide,
            Self::Transition { .. } => CommandKind::Transition,
            Self::Audio {
                kind: Audio::Play, ..
            } => CommandKind::Play,
            Self::Audio {
                kind: Audio::Stop, ..
            } => CommandKind::Stop,
            Self::Audio {
                kind: Audio::Queue, ..
            } => CommandKind::Queue,
            Self::Pause { .. } => CommandKind::Pause,
            Self::WaitClick => CommandKind::WaitClick,
        }
    }
}

impl fmt::Display for Command {
    /// Renders a command for a log, a test's expectation, or a terminal.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Say {
                speaker,
                attributes,
                text,
                options,
                transition,
            } => render_say(
                f,
                speaker.as_deref(),
                attributes,
                text,
                options,
                transition.as_deref(),
            ),
            Self::Menu { prompt, choices } => render_menu(f, prompt.as_deref(), choices),
            Self::Stage {
                kind,
                image,
                attributes,
                transforms,
                transition,
            } => render_stage(
                f,
                *kind,
                image,
                attributes,
                transforms,
                transition.as_deref(),
            ),
            Self::Audio {
                kind,
                channel,
                source,
                looping,
                fade,
            } => render_audio(f, *kind, channel, source.as_deref(), *looping, *fade),
            Self::Transition { name } => write!(f, "with {name}"),
            Self::Pause { seconds: None } => f.write_str("pause"),
            Self::Pause {
                seconds: Some(seconds),
            } => write!(f, "pause {seconds}"),
            Self::WaitClick => f.write_str("wait click"),
        }
    }
}

/// Renders a say statement.
fn render_say(
    f: &mut fmt::Formatter<'_>,
    speaker: Option<&str>,
    attributes: &[String],
    text: &str,
    options: &[(String, Value)],
    transition: Option<&str>,
) -> fmt::Result {
    f.write_str("say ")?;
    if let Some(speaker) = speaker {
        write!(f, "{speaker}")?;
        for attribute in attributes {
            write!(f, " {attribute}")?;
        }
        f.write_str(" ")?;
    }
    write!(f, "{text:?}")?;
    if !options.is_empty() {
        let rendered: Vec<String> = options
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        write!(f, " ({})", rendered.join(", "))?;
    }
    if let Some(transition) = transition {
        write!(f, " with {transition}")?;
    }
    Ok(())
}

/// Renders a menu.
fn render_menu(
    f: &mut fmt::Formatter<'_>,
    prompt: Option<&str>,
    choices: &[Choice],
) -> fmt::Result {
    match prompt {
        Some(prompt) => write!(f, "menu {prompt:?}")?,
        None => f.write_str("menu")?,
    }
    for choice in choices {
        write!(f, "\n    [{}] {}", choice.index, choice.text)?;
    }
    Ok(())
}

/// Renders a staging command.
fn render_stage(
    f: &mut fmt::Formatter<'_>,
    kind: Stage,
    image: &str,
    attributes: &[String],
    transforms: &[String],
    transition: Option<&str>,
) -> fmt::Result {
    write!(f, "{} {image}", kind.as_str())?;
    for attribute in attributes {
        write!(f, " {attribute}")?;
    }
    if !transforms.is_empty() {
        write!(f, " at {}", transforms.join(", "))?;
    }
    if let Some(transition) = transition {
        write!(f, " with {transition}")?;
    }
    Ok(())
}

/// Renders an audio command.
fn render_audio(
    f: &mut fmt::Formatter<'_>,
    kind: Audio,
    channel: &str,
    source: Option<&str>,
    looping: bool,
    fade: Option<f64>,
) -> fmt::Result {
    write!(f, "{} {channel}", kind.as_str())?;
    if let Some(source) = source {
        write!(f, " {source}")?;
    }
    if looping {
        f.write_str(" loop")?;
    }
    if let Some(fade) = fade {
        write!(f, " fade {fade}")?;
    }
    Ok(())
}
