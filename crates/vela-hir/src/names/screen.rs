//! What a screen reads (`SCREENS.md §2.2`).
//!
//! Split from `names` because a screen is a *body* with its own scope rather than a statement: its
//! parameters, its `default`s and what a loop bound are the names it has, and the module's
//! definitions are the world it reads. The walk is here and the resolver is there, which is also how
//! the two halves of the rule stay readable — *what is in scope* and *what a name may be*.

use vela_syntax::{Expr, ScreenArg, ScreenDecl, ScreenLine, ScreenNode};

use super::{Resolver, dotted};

/// The nine positions `at`, `align` and `anchor` name (`SCREENS.md §4.2`).
///
/// These props take an *anchor* rather than a value, so a word from this list is the vocabulary's
/// rather than a name the module has to declare.
const ANCHORS: &[&str] = &[
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

/// Whether a prop is one that names a position rather than a value.
fn is_anchor(name: &str, value: &Expr) -> bool {
    matches!(name, "at" | "align" | "anchor")
        && matches!(value, Expr::Name { name, .. } if ANCHORS.contains(&name.as_str()))
}

impl Resolver<'_> {
    /// Walks a screen's lines, with the screen's own scope.
    ///
    /// A screen reads four kinds of name (`SCREENS.md §2.2`): what it was *given* (its parameters),
    /// what it *keeps* (`default`, §2.5), what a loop bound — and the world, which is
    /// `LANGUAGE.md §1`'s rule applied to the one construct that had never been held to it. A name
    /// that is none of those reads nothing at runtime *and says so nowhere*: the screen draws the
    /// arm that nothing took, and the first symptom is a screen with a piece missing.
    pub(super) fn screen(&mut self, decl: &ScreenDecl) {
        // The scope is the screen's, not the module's: a screen is a function (§2), and a parameter
        // shadows whatever shares its name outside.
        let outer = std::mem::take(&mut self.scope);
        self.in_screen = true;
        for param in &decl.params {
            self.scope.insert(param.name.clone());
        }
        self.screen_lines(&decl.body);
        self.in_screen = false;
        self.scope = outer;
    }

    /// Walks one screen's lines in order.
    fn screen_lines(&mut self, lines: &[ScreenLine]) {
        for line in lines {
            match line {
                // A variable is visible from where it is written, like a `var`.
                ScreenLine::Default { name, value, .. } => {
                    self.screen_expr(value);
                    self.scope.insert(name.clone());
                }
                ScreenLine::If {
                    condition,
                    body,
                    elifs,
                    else_body,
                    ..
                } => {
                    self.screen_expr(condition);
                    self.screen_lines(body);
                    for clause in elifs {
                        self.screen_expr(&clause.condition);
                        self.screen_lines(&clause.body);
                    }
                    if let Some(else_body) = else_body {
                        self.screen_lines(else_body);
                    }
                }
                ScreenLine::For {
                    binding,
                    iterable,
                    body,
                    ..
                } => {
                    self.screen_expr(iterable);
                    self.scope.insert(binding.clone());
                    self.screen_lines(body);
                }
                ScreenLine::Use { args, body, .. } => {
                    self.screen_args(args, None);
                    self.screen_lines(body);
                }
                ScreenLine::Key { action, .. } => self.screen_expr(action),
                ScreenLine::Timer {
                    seconds, action, ..
                } => {
                    self.screen_expr(seconds);
                    self.screen_expr(action);
                }
                ScreenLine::Node(node) => self.screen_node(node),
                ScreenLine::Layer { .. }
                | ScreenLine::StylePrefix { .. }
                | ScreenLine::Transclude { .. } => {}
            }
        }
    }

    /// Walks a widget's arguments and its children.
    fn screen_node(&mut self, node: &ScreenNode) {
        // `image`'s content is a *picture's* name when its head is not in scope (`SCREENS.md §3.1`):
        // `image bg.room` names an asset, `image item.icon` reads the element a loop bound. The
        // picture table is the platform's, so a name it has and this file does not is not a typo.
        let picture = node.name == "image";
        self.screen_args(&node.args, picture.then_some(0));
        self.screen_lines(&node.children);
    }

    /// Walks a list of arguments, exempting the one at `content` when it is a picture.
    fn screen_args(&mut self, args: &[ScreenArg], content: Option<usize>) {
        for (index, arg) in args.iter().enumerate() {
            match arg {
                ScreenArg::Value(value) if Some(index) == content && !self.head_is_known(value) => {
                }
                ScreenArg::Value(value) => self.screen_expr(value),
                // `style = body` names a *style*, which the screen checker holds to existence
                // (`E5007`), and `at`/`align`/`anchor` take one of nine positions rather than a
                // value (`SCREENS.md §4.2`) — `box at bottom` reads an anchor, not a name.
                ScreenArg::Named {
                    name,
                    value: Some(value),
                    ..
                } if name != "style" && !is_anchor(name, value) => self.screen_expr(value),
                _ => {}
            }
        }
    }

    /// An expression in a screen, where a *call's* name is not a value.
    ///
    /// An action the registry holds (`action quit()`, `SCREENS.md §7`), a question the host answers
    /// (`variant("pc")`, §2.6) and a builtin are all calls, and each is checked where its own
    /// vocabulary lives. What a call *passes* is a value, and is walked like any other.
    fn screen_expr(&mut self, expr: &Expr) {
        // `theme.<token>` is a theme token rather than a name (`SCREENS.md §5`), and a token the
        // theme does not hold is a style's problem rather than a screen's.
        if dotted(expr).is_some_and(|path| path[0] == "theme" && path.len() > 1) {
            return;
        }
        self.expr(expr);
    }

    /// Whether a dotted expression starts at a name this screen has.
    fn head_is_known(&self, expr: &Expr) -> bool {
        dotted(expr).is_some_and(|path| self.known(&path[0]))
    }
}
