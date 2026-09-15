//! Lowering a written type into a type.

use vela_syntax::Type;

use crate::env::Env;
use crate::ty::Ty;

/// Lowers a type as written into a type.
///
/// A single name that is not builtin is looked up among the module's declared types. A
/// *dotted* name is a type from another module, whose shape this environment does not
/// hold, so it stays unknown rather than being guessed at.
#[must_use]
pub fn lower(written: &Type, env: &Env) -> Ty {
    match written {
        Type::Named { path, .. } => lookup(path, env),
        Type::Optional { inner, .. } => Ty::Optional(Box::new(lower(inner, env))),
        Type::List { element, .. } => Ty::List(Box::new(lower(element, env))),
        Type::Map { key, value, .. } => {
            Ty::Map(Box::new(lower(key, env)), Box::new(lower(value, env)))
        }
        // The language has no tuple type, so a written one names nothing the checker can
        // place. It is reported where it was written rather than here.
        Type::Tuple { .. } | Type::Error { .. } => Ty::Unknown,
    }
}

/// Lowers a possibly dotted type name.
fn lookup(path: &[String], env: &Env) -> Ty {
    if path.len() != 1 {
        return Ty::Unknown;
    }

    match path[0].as_str() {
        "int" => Ty::Int,
        "float" => Ty::Float,
        "bool" => Ty::Bool,
        "str" => Ty::Str,
        name => env.declared(name).unwrap_or(Ty::Unknown),
    }
}
