//! Tests for the pack codec itself, over and above the behaviour tests in `tests/pack.rs`.
//!
//! Two things only a codec test can check: that *every* expression variant's tag and fields agree
//! between the writer and the reader — a screen rarely contains all of them — and that a corrupt
//! container is refused rather than half-read.

use vela_span::{FileId, Span};
use vela_syntax::{BinOp, Expr, StrPart, UnOp};

use super::model::ScreenPack;
use super::read::decode_expr;
use super::write::encode_expr;
use crate::error::PackError;

/// A span for a hand-built expression; the coordinates mean nothing to the codec.
fn span() -> Span {
    Span::new(FileId::from_raw(0), 3, 9)
}

/// A leaf to hang compound expressions off.
fn leaf() -> Expr {
    Expr::Int {
        span: span(),
        value: 1,
        hex: false,
    }
}

/// One of every expression variant that is not an operator application.
fn expressions() -> Vec<Expr> {
    vec![
        // Decimal, because that is the radix the codec stores: a pack keeps the value, and the
        // source formatter is what keeps the spelling.
        Expr::Int {
            span: span(),
            value: -3,
            hex: false,
        },
        Expr::Float {
            span: span(),
            value: 1.5,
        },
        Expr::Path {
            span: span(),
            value: "art/room.png".to_string(),
        },
        Expr::Bool {
            span: span(),
            value: true,
        },
        Expr::None { span: span() },
        Expr::Name {
            span: span(),
            name: "line".to_string(),
        },
        Expr::Str {
            span: span(),
            parts: vec![
                StrPart::Literal {
                    span: span(),
                    text: "score ".to_string(),
                },
                StrPart::Interpolation {
                    span: span(),
                    expr: Box::new(leaf()),
                },
            ],
        },
        Expr::List {
            span: span(),
            items: vec![leaf(), leaf()],
        },
        Expr::Map {
            span: span(),
            entries: vec![(leaf(), leaf())],
        },
        Expr::Field {
            span: span(),
            base: Box::new(leaf()),
            name: "score".to_string(),
        },
        Expr::Call {
            span: span(),
            callee: Box::new(leaf()),
            args: vec![leaf()],
        },
        Expr::Index {
            span: span(),
            base: Box::new(leaf()),
            index: Box::new(leaf()),
        },
        Expr::Paren {
            span: span(),
            inner: Box::new(leaf()),
        },
        Expr::If {
            span: span(),
            cond: Box::new(leaf()),
            then_: Box::new(leaf()),
            else_: Box::new(leaf()),
        },
        Expr::Lambda {
            span: span(),
            params: Vec::new(),
            body: Box::new(leaf()),
        },
        Expr::Error { span: span() },
    ]
}

/// Every operator application, so no tag can drift without a test noticing.
fn operators() -> Vec<Expr> {
    let bin_ops = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Div,
        BinOp::Rem,
        BinOp::Eq,
        BinOp::Ne,
        BinOp::Lt,
        BinOp::Le,
        BinOp::Gt,
        BinOp::Ge,
        BinOp::And,
        BinOp::Or,
        BinOp::Is,
        BinOp::IsNot,
        BinOp::In,
        BinOp::NotIn,
        BinOp::Coalesce,
    ];

    let mut exprs: Vec<Expr> = bin_ops
        .into_iter()
        .map(|op| Expr::Binary {
            span: span(),
            op,
            lhs: Box::new(leaf()),
            rhs: Box::new(leaf()),
        })
        .collect();
    for op in [UnOp::Neg, UnOp::Not, UnOp::Bang] {
        exprs.push(Expr::Unary {
            span: span(),
            op,
            operand: Box::new(leaf()),
        });
    }
    exprs
}

#[test]
fn every_expression_variant_round_trips() {
    for expr in expressions().into_iter().chain(operators()) {
        let bytes = encode_expr(&expr);
        let back = decode_expr(&bytes).expect("an expression decodes");
        assert_eq!(
            encode_expr(&back),
            bytes,
            "an expression did not survive the round trip, so a tag or a field disagrees"
        );
    }
}

/// A screen with a theme, a style, a condition, and a composition, so the round trip covers more
/// than a leaf — including the two lines that only exist because screens compose.
fn sample() -> ScreenPack {
    let source = "\
theme dusk:
    color bg = 0x10121a
    font ui = \"sans\"

style body from text:
    color = theme.fg
    hover_color = theme.bg

screen wrapper:
    column:
        transclude

screen dialogue(name: str?, line):
    layer ui
    style_prefix dialogue
    use wrapper:
        box at bottom, stretch_x:
            pad 24
            if name is not none:
                text name
            text line style = body

screen chooser(which):
    if which:
        text \"A\"
    elif which is not none:
        text \"B\"
    else:
        text \"C\"
    key cancel action close_screen()
    timer 1.5 action hide(chooser)
";
    let parsed = vela_syntax::parse(FileId::from_raw(0), source);
    ScreenPack::compile("main", &parsed.program.items)
}

/// A style's per-state setting survives by its *key*, prefix and all.
///
/// The codec carries a setting as a key and a value, so a state prefix needs no tag of its own — which
/// is the point of storing a state in the name rather than in a field of the format.
#[test]
fn a_state_setting_survives_the_round_trip() {
    let bytes = sample().to_bytes();
    let pack = ScreenPack::from_bytes(&bytes).expect("a pack decodes");

    let keys: Vec<&str> = pack
        .set
        .styles
        .iter()
        .flat_map(|style| style.settings.iter())
        .map(|setting| setting.key.as_str())
        .collect();
    assert!(keys.contains(&"color"), "{keys:?}");
    assert!(keys.contains(&"hover_color"), "{keys:?}");
}

/// A theme's font tokens survive the codec, so a style's `font = theme.<token>` still resolves from a
/// bundle.
///
/// The pack carries the table because a run from a bundle has no parser to read the theme again: a
/// token that did not survive would leave every screen that names it falling back to the default font —
/// a wrong screen rather than a refused one.
#[test]
fn a_theme_font_survives_the_round_trip() {
    let bytes = sample().to_bytes();
    let pack = ScreenPack::from_bytes(&bytes).expect("a pack decodes");
    assert_eq!(pack.set.fonts.get("ui"), Some("sans"));
}

/// An `if`'s `elif` arms and its `else` survive the codec.
///
/// They are fields of the `If` tag rather than one tag per arm, so the failure this pins is a reader
/// that takes the `elif` count for the next line's tag — a screen that decodes as a tree nobody wrote.
#[test]
fn an_if_chain_survives_the_round_trip() {
    let bytes = sample().to_bytes();
    let set = ScreenPack::from_bytes(&bytes)
        .expect("a pack decodes")
        .into_set();

    let chooser = set.screen("chooser").expect("the screen survives");
    let Some(vela_syntax::ScreenLine::If {
        elifs, else_body, ..
    }) = chooser.body.first()
    else {
        panic!("expected the chain at the top of the screen");
    };
    assert_eq!(elifs.len(), 1, "the `elif` arm did not survive");
    assert!(else_body.is_some(), "the `else` arm did not survive");
}

/// The two input lines survive the codec, their action expressions and all.
///
/// A version-7 reader would read either tag as a widget named `""`, which draws nothing — which is why
/// the version moved: a binding that came back as a widget would make every screen it skins wrong
/// rather than refused.
#[test]
fn a_binding_survives_the_round_trip() {
    let bytes = sample().to_bytes();
    let set = ScreenPack::from_bytes(&bytes)
        .expect("a pack decodes")
        .into_set();

    let chooser = set.screen("chooser").expect("the screen survives");
    assert!(
        chooser.body.iter().any(
            |line| matches!(line, vela_syntax::ScreenLine::Key { name, .. } if name == "cancel")
        ),
        "the `key` line did not survive"
    );
    assert!(
        chooser
            .body
            .iter()
            .any(|line| matches!(line, vela_syntax::ScreenLine::Timer { .. })),
        "the `timer` line did not survive"
    );
}

/// `style_prefix` survives as itself rather than as a widget named `""`.
///
/// It carries no widget name, so the failure this pins is the fall-through arm: a reader that did not
/// know the tag would decode it as a node, and every screen it skins would come back unskinned.
#[test]
fn a_style_prefix_survives_the_round_trip() {
    let bytes = sample().to_bytes();
    let set = ScreenPack::from_bytes(&bytes)
        .expect("a pack decodes")
        .into_set();

    let dialogue = set.screen("dialogue").expect("the screen survives");
    assert!(
        dialogue.body.iter().any(
            |line| matches!(line, vela_syntax::ScreenLine::StylePrefix { name, .. } if name == "dialogue")
        ),
        "`style_prefix` did not survive"
    );
}

/// `use` and `transclude` survive the codec, and land as themselves rather than as widgets.
///
/// A tag read as the wrong variant is the failure this pins: both new lines carry no widget name, so
/// a reader that fell through to the widget arm would decode them as a node called `""`.
#[test]
fn composition_survives_the_round_trip() {
    let bytes = sample().to_bytes();
    let set = ScreenPack::from_bytes(&bytes)
        .expect("a pack decodes")
        .into_set();

    let wrapper = set.screen("wrapper").expect("the wrapper survives");
    assert!(
        wrapper
            .body
            .iter()
            .any(|line| matches!(line, vela_syntax::ScreenLine::Node(node)
                if node.children.iter().any(|line| matches!(line, vela_syntax::ScreenLine::Transclude { .. })))),
        "`transclude` did not survive"
    );

    let dialogue = set.screen("dialogue").expect("the caller survives");
    assert!(
        dialogue.body.iter().any(
            |line| matches!(line, vela_syntax::ScreenLine::Use { body, .. } if !body.is_empty())
        ),
        "`use` with its block did not survive"
    );
}

#[test]
fn a_pack_round_trips_byte_for_byte() {
    let bytes = sample().to_bytes();
    let back = ScreenPack::from_bytes(&bytes).expect("a pack decodes");
    assert_eq!(back.to_bytes(), bytes, "re-encoding a pack changed it");
    assert!(
        back.into_set().has("dialogue"),
        "the screen did not survive"
    );
}

#[test]
fn a_truncated_container_is_refused() {
    let bytes = sample().to_bytes();
    for cut in [0, 1, 4, 9, 10, bytes.len() - 1] {
        assert!(
            ScreenPack::from_bytes(&bytes[..cut]).is_err(),
            "a pack cut to {cut} bytes was accepted"
        );
    }
}

#[test]
fn a_pack_that_is_not_one_is_refused() {
    let bytes = sample().to_bytes();
    let mut wrong = bytes.clone();
    wrong[0] = b'X';

    let error = ScreenPack::from_bytes(&wrong).expect_err("bad magic is refused");
    assert!(error.to_string().contains("magic"), "{error}");
}

#[test]
fn a_broken_checksum_is_refused() {
    let bytes = sample().to_bytes();
    let mut damaged = bytes.clone();
    // A byte inside the sections rather than the header, so the magic and version still read.
    let index = damaged.len() / 2;
    damaged[index] ^= 0xFF;

    let error = ScreenPack::from_bytes(&damaged).expect_err("a damaged pack is refused");
    assert!(error.to_string().contains("checksum"), "{error}");
}

#[test]
fn a_newer_version_is_refused() {
    let bytes = sample().to_bytes();
    let mut newer = bytes.clone();
    newer[4..6].copy_from_slice(&(crate::pack::PACK_VERSION + 1).to_le_bytes());

    let error = ScreenPack::from_bytes(&newer).expect_err("a newer pack is refused");
    assert!(matches!(error, PackError::Version(_)), "{error:?}");
}
