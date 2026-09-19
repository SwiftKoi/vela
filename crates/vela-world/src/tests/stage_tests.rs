//! What `scene`, `show` and `hide` mean to the stage.
//!
//! The rule is Ren'Py's, and it is the reason a character can change expression without becoming two
//! characters: a name is a *tag* and attributes. A capture of the migrated sample is what showed it —
//! two Sylvies side by side, because `show` appended instead of replacing.

use crate::stage::SceneState;

/// A second `show` of the same tag replaces the sprite where it stands.
#[test]
fn a_show_replaces_by_tag_and_keeps_the_order() {
    let mut stage = SceneState::default();
    stage.scene("bg.uni");
    stage.show("sylvie.green.normal");
    stage.show("player");
    stage.show("sylvie.green.smile");

    let images: Vec<&str> = stage
        .images()
        .iter()
        .map(|staged| staged.image.as_str())
        .collect();
    assert_eq!(images, ["bg.uni", "sylvie.green.smile", "player"]);
}

/// A `show` of a tag that is not on stage is an addition, not a replacement.
#[test]
fn a_show_of_a_new_tag_adds_one() {
    let mut stage = SceneState::default();
    stage.scene("bg.uni");
    stage.show("sylvie.green.normal");
    stage.show("player");

    assert_eq!(stage.images().len(), 3);
}

/// `scene` replaces everything, which is what makes it a scene rather than a show.
#[test]
fn a_scene_clears_the_stage() {
    let mut stage = SceneState::default();
    stage.scene("bg.uni");
    stage.show("sylvie.green.normal");
    stage.scene("bg.meadow");

    let images: Vec<&str> = stage
        .images()
        .iter()
        .map(|staged| staged.image.as_str())
        .collect();
    assert_eq!(images, ["bg.meadow"]);
}

/// `hide` takes the tag, so the short and the long spelling are one instruction.
#[test]
fn a_hide_takes_the_tag() {
    let mut stage = SceneState::default();
    stage.scene("bg.uni");
    stage.show("sylvie.green.normal");
    stage.hide("sylvie");

    let images: Vec<&str> = stage
        .images()
        .iter()
        .map(|staged| staged.image.as_str())
        .collect();
    assert_eq!(images, ["bg.uni"]);
}

/// A bare tag keeps the face it has, which is what Ren'Py's attributes mean.
///
/// `show sylvie green normal` then `show sylvie` is the same Sylvie — the second names the *tag*, and
/// the attributes an earlier `show` set stay. In Vela the attributes are part of the image's *name*,
/// so keeping them is keeping the name; replacing it with the tag would stage a picture nothing
/// declares, which is the placeholder box.
#[test]
fn a_show_of_a_bare_tag_keeps_the_face() {
    let mut stage = SceneState::default();
    stage.scene("bg.uni");
    stage.show("sylvie.green.normal");
    stage.show("sylvie");

    let images: Vec<&str> = stage
        .images()
        .iter()
        .map(|staged| staged.image.as_str())
        .collect();
    assert_eq!(images, ["bg.uni", "sylvie.green.normal"]);
}

/// And a `show` that names a face replaces it, in place.
#[test]
fn a_show_with_a_face_changes_it() {
    let mut stage = SceneState::default();
    stage.show("sylvie.green.normal");
    stage.show("player");
    stage.show("sylvie.green.smile");

    let images: Vec<&str> = stage
        .images()
        .iter()
        .map(|staged| staged.image.as_str())
        .collect();
    assert_eq!(images, ["sylvie.green.smile", "player"]);
}
