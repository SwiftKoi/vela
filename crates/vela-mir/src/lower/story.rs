//! Lowering story constructs.
//!
//! Every construct here does the same two things: build a command, then suspend. That is
//! the whole of the VM's contact with the outside world (`ARCHITECTURE.md §7`), and it is
//! why a story is testable headlessly — a test observes the command stream instead of a
//! window.
//!
//! # The command schemas
//!
//! `BYTECODE.md §3.3` says a command is a registered name plus a field schema. These are
//! those schemas, in the one place that both builds them and documents them. An absence —
//! `none` for an optional, an empty list for the rest — is passed explicitly rather than
//! by arity, so a reader of the MIR sees the shape without counting.
//!
//! | Command | Arguments |
//! | --- | --- |
//! | `Say` | speaker, attributes, text, options, transition |
//! | `Menu` | prompt, choice texts, choice enabled, choice guards |
//! | `Scene` / `Show` / `Hide` | image, attributes, transforms, transition |
//! | `Transition` | name |
//! | `Play` / `Stop` / `Queue` | channel, source, looping, fade |
//! | `Pause` | seconds |
//! | `WaitClick` | — |

use vela_span::Span;
use vela_syntax::{
    AudioKind, AudioStmt, CallStmt, Expr, JumpStmt, MenuStmt, SayStmt, StageKind, StageStmt, Stmt,
    StrPart, WaitEvent, WaitStmt, WithStmt,
};
use vela_types::Ty;
use vela_world::CommandKind;

use crate::ir::{BlockId, Const, Slot, StmtKind, Terminator, Value, VariantId, YieldSite};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// Lowers a statement that talks to the outside world.
    pub(crate) fn story(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Say(say) => self.say(say),
            Stmt::Menu(menu) => self.menu(menu),
            Stmt::Jump(jump) => self.jump(jump),
            Stmt::Call(call) => self.call_label(call),
            Stmt::Stage(stage) => self.stage(stage),
            Stmt::With(with) => self.with(with),
            Stmt::Wait(wait) => self.wait(wait),
            Stmt::Audio(audio) => self.audio(audio),
            _ => {}
        }
    }

    /// `"…"` and `eileen "…"`.
    pub(crate) fn say(&mut self, say: &SayStmt) {
        let span = say.span;
        // A `say` names the *character*; the box shows its display name. `speakers` is the
        // declaration's `name` setting, and a speaker that is not a character — a bare `"…"`, or a
        // name the file did not declare — is passed through as written.
        let speaker = self.optional_text(
            say.speaker
                .as_deref()
                .map(|who| {
                    self.speakers
                        .get(who)
                        .cloned()
                        .unwrap_or_else(|| who.to_string())
                })
                .as_deref(),
        );
        let attributes = self.text_list(&say.attributes, span);
        let text = self.expr(&say.line);

        let options: Vec<(Value, Value)> = say
            .options
            .iter()
            .map(|(name, value)| {
                let key = self.constant(Const::Str(name.clone()));
                let value = self.expr(value);
                (key, value)
            })
            .collect();
        let options = self.map(options, span);

        let transition = self.optional_text(say.transition.as_deref());
        self.commit(
            CommandKind::Say,
            vec![speaker, attributes, text, options, transition],
            span,
        );
    }

    /// `menu`.
    ///
    /// Guards are passed to the host as a parallel `enabled` list *and* re-checked after
    /// the answer arrives. The host honours them so the player cannot pick a choice that
    /// is not offered; the re-check means a host that ignores them cannot make the story
    /// take a branch it decided against.
    pub(crate) fn menu(&mut self, menu: &MenuStmt) {
        let span = menu.span;
        let prompt_text = menu.prompt.as_ref().and_then(literal_text);
        let prompt = self.optional_text(prompt_text.as_deref());

        let texts: Vec<String> = menu
            .choices
            .iter()
            .map(|choice| literal_text(&choice.text).unwrap_or_default())
            .collect();
        let choices = self.text_list(&texts, span);

        let enabled: Vec<Value> = menu
            .choices
            .iter()
            .map(|choice| match &choice.condition {
                Some(condition) => self.expr(condition),
                None => self.constant(Const::Bool(true)),
            })
            .collect();
        let enabled = self.list_of(enabled, Ty::Bool, span);

        let selected = self.temp(Ty::Int);
        self.commit_into(
            CommandKind::Menu,
            vec![prompt, choices, enabled],
            span,
            Some(selected),
        );

        // The block the menu is *in*, captured before the new blocks are opened: `open`
        // moves the insertion point, so asking afterwards would name the last choice's
        // block and seal the dispatch there instead.
        let entry = self.body.current();
        let join = self.body.open();
        let blocks: Vec<BlockId> = menu.choices.iter().map(|_| self.body.open()).collect();
        self.body.start(entry);
        let arms: Vec<(VariantId, BlockId)> = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (VariantId(u32::try_from(index).unwrap_or(u32::MAX)), *block))
            .collect();
        self.body.seal(Terminator::Dispatch {
            // A menu's tag is a choice position rather than a variant, so there is no enum
            // to name. The empty string is how the printer and the verifier tell the two
            // kinds of table apart.
            enum_name: String::new(),
            value: Value::Slot(selected),
            arms,
            else_: join,
        });

        for (choice, block) in menu.choices.iter().zip(&blocks) {
            self.body.start(*block);
            if let Some(condition) = &choice.condition {
                // A host that answered with a choice it was told not to offer lands here.
                let condition = self.expr(condition);
                let taken = self.body.open();
                self.body.seal(Terminator::Branch {
                    cond: condition,
                    then_: taken,
                    else_: join,
                });
                self.body.start(taken);
            }
            self.statements(&choice.body);
            self.body.seal(Terminator::Goto(join));
        }

        self.body.start(join);
    }

    /// `jump label`.
    pub(crate) fn jump(&mut self, jump: &JumpStmt) {
        let target = self.label_ref(&jump.target);
        self.body.seal(Terminator::JumpLabel(target));
        self.body.open();
    }

    /// `call label`.
    pub(crate) fn call_label(&mut self, call: &CallStmt) {
        let target = self.label_ref(&call.target);
        let entry = self.body.current();
        let ret = self.body.open();
        let after = self.body.open();
        self.body.start(entry);
        self.body.seal(Terminator::CallLabel { target, ret });
        self.body.start(ret);
        self.body.seal(Terminator::Goto(after));
        self.body.start(after);
    }

    /// `scene`, `show`, `hide`.
    pub(crate) fn stage(&mut self, stage: &StageStmt) {
        let kind = match stage.kind {
            StageKind::Scene => CommandKind::Scene,
            StageKind::Show => CommandKind::Show,
            StageKind::Hide => CommandKind::Hide,
        };
        let span = stage.span;
        let image = stage.image.join(".");
        if kind != CommandKind::Hide {
            self.module.assets.add_image(image.clone());
        }

        let image = self.text(&image);
        let attributes = self.text_list(&stage.attributes, span);
        let transforms = self.text_list(&stage.transforms, span);
        let transition = self.optional_text(stage.transition.as_deref());
        self.commit(kind, vec![image, attributes, transforms, transition], span);
    }

    /// `with transition`.
    pub(crate) fn with(&mut self, with: &WithStmt) {
        let name = self.text(&with.transition);
        self.commit(CommandKind::Transition, vec![name], with.span);
    }

    /// `pause`, and `wait`.
    pub(crate) fn wait(&mut self, wait: &WaitStmt) {
        match &wait.event {
            WaitEvent::Click => self.commit(CommandKind::WaitClick, Vec::new(), wait.span),
            WaitEvent::Duration(duration) => {
                let seconds = self.expr(duration);
                self.commit(CommandKind::Pause, vec![seconds], wait.span);
            }
        }
    }

    /// `play`, `stop`, `queue`.
    pub(crate) fn audio(&mut self, audio: &AudioStmt) {
        let kind = match audio.kind {
            AudioKind::Play => CommandKind::Play,
            AudioKind::Stop => CommandKind::Stop,
            AudioKind::Queue => CommandKind::Queue,
        };
        let span = audio.span;

        let channel = self.text(&audio.channel);
        let source = match &audio.source {
            Some(source) => {
                let path = literal_text(source).unwrap_or_default();
                self.module.assets.add_audio(path.clone());
                self.text(&path)
            }
            None => self.optional_text(None),
        };
        let looping = self.constant(Const::Bool(audio.looping));
        let fade = match &audio.fade {
            Some(fade) => self.expr(fade),
            None => self.constant(Const::None),
        };
        self.commit(kind, vec![channel, source, looping, fade], span);
    }

    /// Builds a command and suspends, discarding the host's answer.
    pub(crate) fn commit(&mut self, kind: CommandKind, args: Vec<Value>, span: Span) {
        self.commit_into(kind, args, span, None);
    }

    /// Builds a command and suspends, keeping the host's answer.
    pub(crate) fn commit_into(
        &mut self,
        kind: CommandKind,
        args: Vec<Value>,
        span: Span,
        result: Option<Slot>,
    ) {
        self.emit(StmtKind::Cmd { kind, args }, span);

        let head = self.body.current();
        let resume = self.body.open();
        self.body.start(head);
        self.body.seal(Terminator::Yield(YieldSite {
            command: kind,
            resume,
            result,
        }));
        self.body.start(resume);
    }

    /// A resolved label reference.
    ///
    /// `forest.clearing` and an aliased `f.clearing` name the same target, and collapsing
    /// them to a module and a label here is what keeps the runtime free of any notion of
    /// modules or imports.
    pub(crate) fn label_ref(&mut self, path: &[String]) -> crate::ir::LabelRef {
        let (label, qualifier): (&str, &[String]) = match path.split_last() {
            Some((label, qualifier)) => (label, qualifier),
            None => ("", &[]),
        };

        crate::ir::LabelRef {
            module: (!qualifier.is_empty()).then(|| qualifier.join(".")),
            label: label.to_string(),
        }
    }
}

/// The literal text of an expression, when it is a plain string.
///
/// A menu's text is rendered by the host before it can answer, so it has to be known when
/// the command is built. An interpolated choice is not rejected here — the checker has
/// already decided what is allowed in this position, and reporting it twice would be a
/// second diagnostic for one mistake.
fn literal_text(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str { parts, .. } => match parts.as_slice() {
            [StrPart::Literal { text, .. }] => Some(text.clone()),
            [] => Some(String::new()),
            _ => None,
        },
        // `@"audio/theme.ogg"` is the same text as `"audio/theme.ogg"` to a command.
        Expr::Path { value, .. } => Some(value.clone()),
        Expr::Paren { inner, .. } => literal_text(inner),
        _ => None,
    }
}
