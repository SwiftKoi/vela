//! The state one lowering carries, and the builder that assembles a body.

use std::collections::BTreeMap;

use vela_diag::Diagnostic;
use vela_hir::ModuleName;
use vela_span::Span;
use vela_syntax::{Expr, Item, Program};
use vela_types::{Env, Scope, Ty};

use crate::ir::{
    Block, BlockId, Body, Const, ConstId, DefaultId, LocalDecl, Module, Place, Slot, Stmt,
    StmtKind, Symbol, Terminator, Value,
};

/// A module lowered to MIR, and what lowering had to say about it.
///
/// Lowering is the last step that can see a *source* problem. Everything after it works on
/// MIR, where an author's intent is only recoverable through a span table — so a rule that
/// can only be checked here (`E2004`, a `default` initialised with something not constant)
/// has to be reported here or not at all.
#[derive(Debug)]
pub struct Lowered {
    /// The module.
    pub module: Module,
    /// Problems found while lowering.
    pub diagnostics: Vec<Diagnostic>,
}

/// Lowers a checked module into MIR.
///
/// Takes the module's *name* rather than deriving it: the name is a property of where the
/// file sits in the project, which the syntax tree does not know.
#[must_use]
pub fn lower(name: &ModuleName, tree: &Program, env: &Env) -> Lowered {
    let mut lowerer = Lowerer {
        env,
        module: Module {
            name: name.clone(),
            ..Module::default()
        },
        locals: BTreeMap::new(),
        types: Scope::default(),
        body: Builder::new(Symbol(String::new()), Ty::Unit),
        funcs: BTreeMap::new(),
        constants: BTreeMap::new(),
        effects: BTreeMap::new(),
        speakers: speakers(tree),
        diagnostics: Vec::new(),
    };

    // Two passes over the declarations: names first, so that a call can target a function
    // declared further down the file, then the bodies.
    for item in &tree.items {
        if let Item::Function(decl) = item
            && let Ok(index) = u32::try_from(lowerer.funcs.len())
        {
            lowerer.funcs.insert(decl.name.clone(), index);
        }
        if let Item::Effect(decl) = item
            && let Ok(index) = u32::try_from(lowerer.module.effects.len())
        {
            lowerer.effects.insert(decl.dotted(), index);
        }
    }

    // Declarations first, then bodies. A `const` has to be folded before any body can use
    // it — a use of one is replaced by its value, and a body lowered first would find a
    // name it had no value for.
    for item in &tree.items {
        match item {
            Item::Struct(decl) => lowerer.lower_struct(decl),
            Item::Enum(decl) => lowerer.lower_enum(decl),
            Item::Const(decl) => lowerer.lower_const(decl),
            Item::Effect(decl) => lowerer.lower_effect(decl),
            Item::Default(decl) => lowerer.lower_default(decl),
            // Characters, images, transforms, screens, styles, and themes do not lower to
            // code. They are declarations *about* presentation, and the statements that
            // use them are what produce commands.
            _ => {}
        }
    }

    for item in &tree.items {
        match item {
            Item::Function(decl) => lowerer.lower_function(decl),
            Item::Label(decl) => lowerer.lower_label(decl),
            _ => {}
        }
    }

    Lowered {
        module: lowerer.module,
        diagnostics: lowerer.diagnostics,
    }
}

/// The display name of every character a file declares, by the name a `say` statement uses.
///
/// A character with no `name` setting keeps the name it was declared with, which is what Ren'Py
/// does with a bare `define e = Character()`: the box shows the tag.
fn speakers(tree: &Program) -> BTreeMap<String, String> {
    let mut speakers = BTreeMap::new();
    for item in &tree.items {
        let Item::Character(decl) = item else {
            continue;
        };
        let named = decl
            .settings
            .iter()
            .find(|setting| setting.key == "name")
            .and_then(|setting| match &setting.value {
                Expr::Str { parts, .. } => match parts.as_slice() {
                    [vela_syntax::StrPart::Literal { text, .. }] => Some(text.clone()),
                    _ => None,
                },
                _ => None,
            });
        speakers.insert(
            decl.name.clone(),
            named.unwrap_or_else(|| decl.name.clone()),
        );
    }
    speakers
}

/// The state of one lowering.
pub(crate) struct Lowerer<'a> {
    /// The module's declarations.
    pub(crate) env: &'a Env,
    /// The module being built.
    pub(crate) module: Module,
    /// Names that resolve to slots in the body being built.
    pub(crate) locals: BTreeMap<String, Slot>,
    /// The same names, for asking the checker what an expression's type is.
    ///
    /// Kept alongside `locals` rather than derived from it because a slot's type is what
    /// the checker needs to answer, and deriving the scope would mean a lookup per
    /// subexpression.
    pub(crate) types: Scope,
    /// The body being built.
    pub(crate) body: Builder,
    /// Declared functions by name, registered before any body is lowered so that a call
    /// can name a function defined further down the file.
    pub(crate) funcs: BTreeMap<String, u32>,
    /// Constants folded so far, so that one can be defined in terms of an earlier one.
    pub(crate) constants: BTreeMap<String, ConstId>,
    /// Declared effects by name, so a dotted call can be told from a field access.
    pub(crate) effects: BTreeMap<String, u32>,
    /// Each character's *display* name, by the name a `say` uses: `s` → `Sylvie`.
    ///
    /// A `say` names the character and a player reads its name, and the two are different strings
    /// on purpose — `s "Hi"` is what an author writes, `Sylvie` is what the box shows. Nothing below
    /// this can tell them apart, which is why the substitution happens while lowering rather than
    /// at the point the box is drawn: the command carries what is on screen, and a save that is
    /// loaded a year later draws the same words.
    pub(crate) speakers: BTreeMap<String, String>,
    /// What lowering found.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Lowerer<'_> {
    /// Introduces a local, in both the slot table and the checker's scope.
    pub(crate) fn declare(&mut self, name: &str, ty: Ty) -> Slot {
        let slot = self.body.slot(name, ty.clone());
        self.locals.insert(name.to_string(), slot);
        self.types.insert(name.to_string(), ty);
        slot
    }

    /// The slot a name refers to, if it is a local.
    pub(crate) fn lookup(&self, name: &str) -> Option<Slot> {
        self.locals.get(name).copied()
    }

    /// A fresh temporary slot.
    pub(crate) fn temp(&mut self, ty: Ty) -> Slot {
        self.body.temp(ty)
    }

    /// Where a `default` lives, if the name is one.
    pub(crate) fn default_of(&self, name: &str) -> Option<DefaultId> {
        self.module
            .defaults
            .iter()
            .position(|candidate| candidate.name == name)
            .and_then(|index| u32::try_from(index).ok())
            .map(DefaultId)
    }

    /// Adds a constant to the pool.
    pub(crate) fn constant(&mut self, value: Const) -> Value {
        Value::Const(self.module.pool.add(value))
    }

    /// Adds a statement to the block being built.
    pub(crate) fn emit(&mut self, kind: StmtKind, span: Span) {
        self.body.push(kind, span);
    }

    /// What the checker says an expression's type is.
    pub(crate) fn type_of(&self, expr: &Expr) -> Ty {
        vela_types::type_of(self.env, &self.types, expr)
    }

    /// The place an assignment target names, if it names one.
    ///
    /// `None` means the target is not a place — a call, a literal, an arithmetic
    /// expression. The checker has already rejected it; lowering must not invent a
    /// location and write there.
    pub(crate) fn place(&mut self, expr: &Expr) -> Option<Place> {
        match expr {
            Expr::Name { name, .. } => {
                if let Some(slot) = self.lookup(name) {
                    return Some(Place::Local(slot));
                }
                self.default_of(name).map(Place::Default)
            }
            Expr::Paren { inner, .. } => self.place(inner),
            Expr::Field { base, name, .. } => {
                let base = self.place(base)?;
                Some(Place::Field {
                    base: Box::new(base),
                    field: name.clone(),
                })
            }
            Expr::Index { base, index, .. } => {
                let base = self.place(base)?;
                let index = self.expr(index);
                Some(Place::Index {
                    base: Box::new(base),
                    index,
                })
            }
            _ => None,
        }
    }

    /// Runs `f` with a fresh body, and installs the result.
    pub(crate) fn in_body<T>(
        &mut self,
        name: Symbol,
        ret: Ty,
        f: impl FnOnce(&mut Self) -> T,
    ) -> (Body, T) {
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_types = std::mem::take(&mut self.types);
        let saved_body = std::mem::replace(&mut self.body, Builder::new(name, ret));

        let result = f(self);

        let body = std::mem::replace(&mut self.body, saved_body).finish();
        self.locals = saved_locals;
        self.types = saved_types;
        (body, result)
    }
}

/// One body, under construction.
pub(crate) struct Builder {
    name: Symbol,
    ret: Ty,
    params: Vec<Slot>,
    locals: Vec<LocalDecl>,
    blocks: Vec<OpenBlock>,
    current: usize,
}

/// A block whose terminator is not decided yet.
struct OpenBlock {
    stmts: Vec<Stmt>,
    term: Option<Terminator>,
}

impl Builder {
    /// A builder holding one empty block.
    pub(crate) fn new(name: Symbol, ret: Ty) -> Self {
        Self {
            name,
            ret,
            params: Vec::new(),
            locals: Vec::new(),
            blocks: vec![OpenBlock {
                stmts: Vec::new(),
                term: None,
            }],
            current: 0,
        }
    }

    /// Allocates a slot with a name.
    pub(crate) fn slot(&mut self, name: &str, ty: Ty) -> Slot {
        let slot = Slot(u32::try_from(self.locals.len()).unwrap_or(u32::MAX));
        self.locals.push(LocalDecl {
            name: name.to_string(),
            ty,
        });
        slot
    }

    /// Allocates an unnamed slot.
    pub(crate) fn temp(&mut self, ty: Ty) -> Slot {
        self.slot("", ty)
    }

    /// Whether the block being built has already ended.
    pub(crate) fn is_terminated(&self) -> bool {
        self.blocks[self.current].term.is_some()
    }

    /// Appends a statement to the block being built.
    pub(crate) fn push(&mut self, kind: StmtKind, span: Span) {
        debug_assert!(
            !self.is_terminated(),
            "a statement was added after the block ended"
        );
        if self.is_terminated() {
            return;
        }
        self.blocks[self.current].stmts.push(Stmt { kind, span });
    }

    /// Ends the block being built.
    pub(crate) fn seal(&mut self, term: Terminator) {
        if !self.is_terminated() {
            self.blocks[self.current].term = Some(term);
        }
    }

    /// Creates a block and starts building it.
    pub(crate) fn open(&mut self) -> BlockId {
        let id = BlockId(u32::try_from(self.blocks.len()).unwrap_or(u32::MAX));
        self.blocks.push(OpenBlock {
            stmts: Vec::new(),
            term: None,
        });
        self.current = self.blocks.len() - 1;
        id
    }

    /// Starts building a block that already exists.
    pub(crate) fn start(&mut self, id: BlockId) {
        self.current = id.0 as usize;
    }

    /// The block being built.
    pub(crate) fn current(&self) -> BlockId {
        BlockId(u32::try_from(self.current).unwrap_or(u32::MAX))
    }

    /// The body's name, for naming a lifted lambda after its parent.
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Declares a parameter.
    pub(crate) fn push_param(&mut self, slot: Slot) {
        self.params.push(slot);
    }

    /// Sets what the body returns, once it is known.
    pub(crate) fn set_ret(&mut self, ty: Ty) {
        self.ret = ty;
    }

    /// The finished body.
    ///
    /// A block still open at the end falls off the end of the body, which returns. Every
    /// other open block was created for a path that ended early, so it is unreachable and
    /// `dead_block` will remove it.
    pub(crate) fn finish(mut self) -> Body {
        let last = self.current;
        for (index, block) in self.blocks.iter_mut().enumerate() {
            if block.term.is_none() {
                block.term = Some(if index == last {
                    Terminator::Return(None)
                } else {
                    Terminator::Unreachable
                });
            }
        }

        Body {
            name: self.name,
            params: self.params,
            ret: self.ret,
            locals: self.locals,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .map(|(index, block)| Block {
                    id: BlockId(u32::try_from(index).unwrap_or(u32::MAX)),
                    stmts: block.stmts,
                    term: block.term.unwrap_or(Terminator::Unreachable),
                })
                .collect(),
            entry: BlockId::ENTRY,
        }
    }
}
