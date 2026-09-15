//! The save schema, derived from source.
//!
//! `RUNTIME.md §2`: *"every `default` declaration and every declared `struct`/`enum`
//! contributes to the schema."* The schema is not the *values* — it is the *shape* the values
//! are saved against, so that a save can be recognized as belonging to a shape it does not
//! match and sent to the migration engine instead of loading as garbage.
//!
//! Deriving it from the parse rather than from `World` is deliberate: a build can be asked
//! what schema it *expects* before it has a world, which is exactly the question a load asks.

use vela_syntax::{EnumDecl, Item, StructDecl, Type, Variant};

use crate::digest;

/// One declaration that contributes to the schema.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Entry {
    /// A `default` declaration: a saved value, by name and type.
    Default {
        /// The name.
        name: String,
        /// Its declared type, if one was written.
        ty: Option<String>,
    },
    /// A `struct` declaration and its fields.
    Struct {
        /// The struct's name.
        name: String,
        /// Its fields, as `name: type`, in declaration order.
        fields: Vec<String>,
    },
    /// An `enum` declaration and its variants.
    Enum {
        /// The enum's name.
        name: String,
        /// Its variants, in declaration order.
        variants: Vec<String>,
    },
}

/// The shape of the saved state a build expects.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Schema {
    entries: Vec<Entry>,
}

impl Schema {
    /// Derives the schema from a program's items.
    ///
    /// Sorted by kind then name, because the schema is a *set*: moving a `default` in the
    /// source does not change the state it saves, so it must not change the digest either.
    #[must_use]
    pub fn derive(items: &[Item]) -> Self {
        let mut entries = Vec::new();
        for item in items {
            match item {
                Item::Default(decl) => entries.push(Entry::Default {
                    name: decl.name.clone(),
                    ty: decl.ty.as_ref().map(render_type),
                }),
                Item::Struct(decl) => entries.push(struct_entry(decl)),
                Item::Enum(decl) => entries.push(enum_entry(decl)),
                _ => {}
            }
        }
        entries.sort_by(|a, b| key(a).cmp(&key(b)));
        Self { entries }
    }

    /// The entries, in canonical order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether the schema declares nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether a `default` of this name is declared.
    ///
    /// A migration's `add_default` asks this before it writes (`RUNTIME.md §6.1`): the chain
    /// is shared by every project, so seeding a default the current build does not declare
    /// would put state into a world whose story never asked for it.
    #[must_use]
    pub fn declares_default(&self, name: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry, Entry::Default { name: declared, .. } if declared == name))
    }

    /// The 256-bit digest of the schema.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        digest::digest(self.canonical().as_bytes())
    }

    /// The schema as one canonical line per entry.
    ///
    /// The digest is taken over this text, so it is the schema's *spelling* that is stable —
    /// which is what makes two independently compiled builds agree.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut text = String::new();
        for entry in &self.entries {
            match entry {
                Entry::Default { name, ty } => {
                    text.push_str(&format!(
                        "default {name}: {}\n",
                        ty.as_deref().unwrap_or("?")
                    ));
                }
                Entry::Struct { name, fields } => {
                    text.push_str(&format!("struct {name} {{ {} }}\n", fields.join(", ")));
                }
                Entry::Enum { name, variants } => {
                    text.push_str(&format!("enum {name} {{ {} }}\n", variants.join(", ")));
                }
            }
        }
        text
    }
}

/// The sort key that makes the entry order canonical.
fn key(entry: &Entry) -> (u8, &str) {
    match entry {
        Entry::Default { name, .. } => (0, name),
        Entry::Struct { name, .. } => (1, name),
        Entry::Enum { name, .. } => (2, name),
    }
}

/// A struct declaration's entry.
fn struct_entry(decl: &StructDecl) -> Entry {
    Entry::Struct {
        name: decl.name.clone(),
        fields: decl
            .fields
            .iter()
            .map(|field| format!("{}: {}", field.name, render_type(&field.ty)))
            .collect(),
    }
}

/// An enum declaration's entry.
fn enum_entry(decl: &EnumDecl) -> Entry {
    Entry::Enum {
        name: decl.name.clone(),
        variants: decl.variants.iter().map(render_variant).collect(),
    }
}

/// A variant as `name` or `name(type, type)`.
fn render_variant(variant: &Variant) -> String {
    if variant.fields.is_empty() {
        return variant.name.clone();
    }
    let types: Vec<String> = variant
        .fields
        .iter()
        .map(|field| render_type(&field.ty))
        .collect();
    format!("{}({})", variant.name, types.join(", "))
}

/// A type expression as an author would write it.
fn render_type(ty: &Type) -> String {
    match ty {
        Type::Named { path, .. } => path.join("."),
        Type::Optional { inner, .. } => format!("{}?", render_type(inner)),
        Type::List { element, .. } => format!("list<{}>", render_type(element)),
        Type::Map { key, value, .. } => {
            format!("map<{}, {}>", render_type(key), render_type(value))
        }
        Type::Tuple { elements, .. } => {
            let inner: Vec<String> = elements.iter().map(render_type).collect();
            format!("({})", inner.join(", "))
        }
        Type::Error { .. } => "?".to_string(),
    }
}
