//! The syntax tree.
//!
//! One file per family, so a construct's shape and its parsing rule stay near each other
//! without either file growing past the size budget.
//!
//! This is a *syntax* tree, not a lossless one: it records structure and spans, not the
//! exact bytes that produced it. That is enough for diagnostics and for lowering, which
//! are the only two consumers at this stage.

pub mod decl;
pub mod expr;
mod program;
mod screen;
pub mod stmt;
pub mod ty;

pub use decl::{
    CharacterDecl, ConstDecl, DefaultDecl, EffectDecl, EnumDecl, FnDecl, ImageDecl, Item,
    LabelDecl, Param, ScreenDecl, Setting, StructDecl, StructField, StyleDecl, ThemeDecl,
    TransformDecl, UseDecl, Variant,
};
pub use expr::{BinOp, Expr, StrPart, UnOp};
pub use program::Program;
pub use screen::{ScreenArg, ScreenLine, ScreenNode};
pub use stmt::{
    AssignOp, AssignStmt, AudioKind, AudioStmt, CallStmt, ElifClause, ExprStmt, ForStmt, IfStmt,
    JumpStmt, MatchArm, MatchStmt, MenuChoice, MenuStmt, Pattern, ReturnStmt, SayStmt, StageKind,
    StageStmt, Stmt, VarStmt, WaitEvent, WaitStmt, WhileStmt, WithStmt,
};
pub use ty::Type;
