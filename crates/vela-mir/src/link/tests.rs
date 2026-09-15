//! Linking: the renames it makes, the conflicts it refuses, and a program that runs across a
//! module boundary.
//!
//! The last one is the point of the whole file. A linker that renames correctly but produces a
//! module the machine cannot execute has fixed nothing, so the cross-module test *runs* the linked
//! program in the reference interpreter and asserts on the command stream.

use std::collections::BTreeMap;

use vela_hir::ModuleName;
use vela_span::SourceMap;
use vela_types::Env;

use crate::ir::Module;
use crate::{Answer, Execution, LinkError, Outcome, Unit, link, print_module};

/// Lowers one module from source, as `vela-compile` does.
fn module(name: &str, text: &str) -> Module {
    let mut sources = SourceMap::new();
    let id = sources.add(name, text);
    let parsed = vela_syntax::parse(id, text);
    let (env, _) = Env::build(&parsed.program);
    let lowered = crate::lower(&ModuleName::new(name), &parsed.program, &env);
    assert!(
        lowered.diagnostics.is_empty(),
        "the fixture does not lower: {:?}",
        lowered.diagnostics
    );
    lowered.module
}

/// An import map with one alias, which is what a `use x.y as z` produces.
fn alias(alias: &str, module: &str) -> BTreeMap<String, ModuleName> {
    BTreeMap::from([(alias.to_string(), ModuleName::new(module))])
}

/// The label names a linked module has.
fn labels(module: &Module) -> Vec<String> {
    module
        .labels
        .iter()
        .map(|body| body.name.to_string())
        .collect()
}

/// The names a unit's own labels become for the same module, for a test that only links one.
fn single(module: &Module) -> Module {
    let empty = BTreeMap::new();
    link(&[Unit {
        module,
        imports: &empty,
    }])
    .expect("one module links")
}

#[test]
fn a_modules_labels_take_its_name() {
    let linked = single(&module(
        "main",
        "label start:\n    jump later\n\nlabel later:\n    return\n",
    ));
    assert_eq!(labels(&linked), vec!["main.start", "main.later"]);
}

/// A label written in its own module is qualified too: after linking there is one label table, and
/// a bare name in it would be a name from nowhere.
#[test]
fn a_same_module_jump_becomes_a_qualified_label() {
    let linked = single(&module(
        "main",
        "label start:\n    jump later\n\nlabel later:\n    return\n",
    ));
    let targets = linked
        .labels
        .iter()
        .flat_map(|body| body.blocks.iter())
        .filter_map(|block| match &block.term {
            crate::Terminator::JumpLabel(target) => Some(target.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(targets.len(), 1, "expected one jump");
    assert_eq!(targets[0].module, None, "a jump should not name a module");
    assert_eq!(targets[0].label, "main.later");
}

/// The end the whole feature exists for: a program split across modules runs.
#[test]
fn a_program_can_jump_between_modules() {
    let main = module(
        "main",
        "use chapters.forest as forest\n\nlabel start:\n    \"A.\"\n    jump forest.clearing\n",
    );
    // The reference resolution happens before lowering, so the qualifier here is the *alias*.
    let imports = alias("forest", "chapters.forest");
    let forest = module(
        "chapters.forest",
        "label clearing:\n    \"B.\"\n    return\n",
    );
    let none = BTreeMap::new();

    let linked = link(&[
        Unit {
            module: &main,
            imports: &imports,
        },
        Unit {
            module: &forest,
            imports: &none,
        },
    ])
    .expect("the program links");

    assert_eq!(
        labels(&linked),
        vec!["chapters.forest.clearing", "main.start"],
        "both modules' labels are in one table, qualified"
    );

    let run = Execution::run(&linked, "main.start", &[Answer::Ack, Answer::Ack]);
    assert_eq!(run.outcome, Outcome::Halted, "{:?}", run.outcome);
    let commands: Vec<String> = run.commands.iter().map(ToString::to_string).collect();
    assert!(
        commands.iter().any(|line| line.contains("\"B.\"")),
        "the story never reached the other module: {commands:?}"
    );
}

/// The image must not depend on the order a driver read the files in, or two builds of one project
/// would differ (`BUILD_AND_ASSETS.md §7`).
#[test]
fn the_order_of_units_does_not_change_the_image() {
    let main = module("main", "label start:\n    return\n");
    let forest = module("chapters.forest", "label clearing:\n    return\n");
    let empty = BTreeMap::new();

    let first = link(&[
        Unit {
            module: &main,
            imports: &empty,
        },
        Unit {
            module: &forest,
            imports: &empty,
        },
    ])
    .expect("links");
    let second = link(&[
        Unit {
            module: &forest,
            imports: &empty,
        },
        Unit {
            module: &main,
            imports: &empty,
        },
    ])
    .expect("links");

    assert_eq!(print_module(&first), print_module(&second));
}

#[test]
fn two_modules_declaring_the_same_default_are_refused() {
    let first = module("a", "default trust: int = 1\n\nlabel start:\n    return\n");
    let second = module("b", "default trust: int = 2\n\nlabel start:\n    return\n");
    let empty = BTreeMap::new();

    let error = link(&[
        Unit {
            module: &first,
            imports: &empty,
        },
        Unit {
            module: &second,
            imports: &empty,
        },
    ])
    .expect_err("a duplicate default is refused");

    assert!(
        matches!(&error, LinkError::DuplicateDefault { name, .. } if name == "trust"),
        "{error:?}"
    );
    // The message names both files, because that is the only place an author can look.
    assert!(error.to_string().contains("`a`"), "{error}");
    assert!(error.to_string().contains("`b`"), "{error}");
}

#[test]
fn a_reference_to_a_module_the_program_does_not_have_is_refused() {
    let main = module("main", "label start:\n    jump forest.clearing\n");
    let empty = BTreeMap::new();

    let error = link(&[Unit {
        module: &main,
        imports: &empty,
    }])
    .expect_err("an unknown module is refused");

    assert!(
        matches!(&error, LinkError::UnknownModule { qualifier, .. } if qualifier == "forest"),
        "{error:?}"
    );
}

/// Two modules declaring the same effect the same way share one entry; the host is asked once.
#[test]
fn an_effect_declared_twice_keeps_one_entry() {
    let first = module(
        "a",
        "effect rand.int(low: int, high: int) -> int\n\nlabel start:\n    return\n",
    );
    let second = module(
        "b",
        "effect rand.int(low: int, high: int) -> int\n\nlabel start:\n    return\n",
    );
    let empty = BTreeMap::new();

    let linked = link(&[
        Unit {
            module: &first,
            imports: &empty,
        },
        Unit {
            module: &second,
            imports: &empty,
        },
    ])
    .expect("identical declarations are not a conflict");

    assert_eq!(linked.effects.len(), 1, "{:?}", linked.effects);
}

#[test]
fn two_modules_disagreeing_about_an_effect_are_refused() {
    let first = module(
        "a",
        "effect rand.int(low: int, high: int) -> int\n\nlabel start:\n    return\n",
    );
    let second = module(
        "b",
        "effect rand.int(low: int) -> int\n\nlabel start:\n    return\n",
    );
    let empty = BTreeMap::new();

    let error = link(&[
        Unit {
            module: &first,
            imports: &empty,
        },
        Unit {
            module: &second,
            imports: &empty,
        },
    ])
    .expect_err("a disagreeing signature is refused");

    assert!(
        matches!(&error, LinkError::EffectMismatch { name, .. } if name == "rand.int"),
        "{error:?}"
    );
}

/// A struct declared in two modules is two structs. Without the prefix, one would overwrite the
/// other in the linked module's table and every field index after it would be wrong.
#[test]
fn a_type_declared_in_two_modules_stays_two_types() {
    let first = module(
        "a",
        "struct Route:\n    name: str\n\nlabel start:\n    return\n",
    );
    let second = module(
        "b",
        "struct Route:\n    name: str\n\nlabel start:\n    return\n",
    );
    let empty = BTreeMap::new();

    let linked = link(&[
        Unit {
            module: &first,
            imports: &empty,
        },
        Unit {
            module: &second,
            imports: &empty,
        },
    ])
    .expect("two same-named structs are not a conflict");

    let names: Vec<&str> = linked.structs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["a.Route", "b.Route"]);
}
