//! Building the values a command carries.
//!
//! A command's arguments are untyped slots and constants, so an absent one has to be
//! spelled `none` and an empty one as an empty aggregate. Doing that in one place is what
//! keeps the schemas in `story` readable as schemas.

use vela_span::Span;
use vela_types::Ty;

use crate::ir::{Const, Place, StmtKind, Value};
use crate::lower::Lowerer;

impl Lowerer<'_> {
    /// A string constant.
    pub(crate) fn text(&mut self, text: &str) -> Value {
        self.constant(Const::Str(text.to_string()))
    }

    /// A string constant, or `none`.
    pub(crate) fn optional_text(&mut self, text: Option<&str>) -> Value {
        match text {
            Some(text) => self.constant(Const::Str(text.to_string())),
            None => self.constant(Const::None),
        }
    }

    /// A list of strings.
    pub(crate) fn text_list(&mut self, items: &[String], span: Span) -> Value {
        let values: Vec<Value> = items.iter().map(|item| self.text(item)).collect();
        self.list_of(values, Ty::Str, span)
    }

    /// A list of values.
    pub(crate) fn list_of(&mut self, items: Vec<Value>, element: Ty, span: Span) -> Value {
        let dst = self.temp(Ty::List(Box::new(element)));
        self.emit(
            StmtKind::ListNew {
                dst: Place::Local(dst),
                items,
            },
            span,
        );
        Value::Slot(dst)
    }

    /// A map of values.
    pub(crate) fn map(&mut self, entries: Vec<(Value, Value)>, span: Span) -> Value {
        let dst = self.temp(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Unknown)));
        self.emit(
            StmtKind::MapNew {
                dst: Place::Local(dst),
                entries,
            },
            span,
        );
        Value::Slot(dst)
    }
}
