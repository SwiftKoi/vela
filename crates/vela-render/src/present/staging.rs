//! Drawing what is on stage: the backdrop, the sprites over it, and the placeholder for a name with
//! no picture behind it.
//!
//! Split from `present.rs` by `REPO_LAYOUT.md §3.1`'s third recipe — the presenter holds the state,
//! and this is one of the two things that state is drawn as (the other being the dialogue). It is
//! also where the *look* of a stage lives: a backdrop fills the frame, a sprite stands in it, and
//! both are decided here rather than by the world, which only knows what is on stage.

use super::{Color, DrawList, ImageQuad, Placement, Presenter, RectQuad, text};
use crate::tint_of;

impl Presenter {
    /// Draws what is on stage: the backdrop filling the frame, then the sprites over it.
    ///
    /// The two are drawn differently because they are different things, and a *place* is not a
    /// picture: a backdrop is a picture of where the story is, so it is scaled to fill the frame
    /// and cropped rather than squeezed; a sprite is a person, drawn at the size the picture is,
    /// standing at the bottom of the frame. Drawing them as equal slots — which is what this did
    /// while the stage was a placeholder — made every scene a strip of squashed thumbnails side by
    /// side, and a capture of the sample is what showed it.
    pub(super) fn build_stage(&mut self, draw: &mut DrawList, width: f32, height: f32) {
        let images: Vec<String> = self
            .scene
            .images()
            .iter()
            .map(|staged| staged.image.clone())
            .collect();

        for image in &images {
            // A solid fills whatever it is in, backdrop or sprite: it has no size of its own, and
            // Ren'Py's `Solid` takes the space it is given for the same reason.
            if let Some(colour) = self.colour_of(image) {
                draw.push_rect(RectQuad::from_corners(0.0, 0.0, width, height, colour));
                continue;
            }
            if self.backdrop.as_deref() == Some(image.as_str()) {
                self.draw_backdrop(draw, image, width, height);
            } else {
                self.draw_sprite(draw, image, width, height);
            }
        }
    }

    /// The backdrop: scaled to cover the frame, cropped where it overflows.
    fn draw_backdrop(&mut self, draw: &mut DrawList, image: &str, width: f32, height: f32) {
        let Some(texture) = self.texture_of(image) else {
            self.draw_placeholder(draw, image, 0.0, 0.0, width, height);
            return;
        };
        let (image_width, image_height) =
            self.size_of(image).unwrap_or((width as u32, height as u32));
        let (image_width, image_height) = (image_width as f32, image_height as f32);
        // `cover`: the smaller scale of the two, so the frame is filled and the overflow is cropped.
        let scale = (width / image_width).max(height / image_height);
        let (scaled_width, scaled_height) = (image_width * scale, image_height * scale);
        let left = (width - scaled_width) / 2.0;
        let top = (height - scaled_height) / 2.0;

        // The crop, as texture coordinates: what hangs off the frame is outside the unit square.
        let u0 = (-left / scaled_width).max(0.0);
        let v0 = (-top / scaled_height).max(0.0);
        let u1 = ((width - left) / scaled_width).min(1.0);
        let v1 = ((height - top) / scaled_height).min(1.0);

        draw.push_image(ImageQuad {
            x: left,
            y: top,
            width: scaled_width,
            height: scaled_height,
            uv: [u0, v0, u1, v1],
            image: texture,
            color: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
        });
    }

    /// A sprite: its own size, standing at the bottom of the frame, centred.
    ///
    /// Centred because that is where Ren'Py puts one that no `at` clause moves — and `at` is the
    /// thing this engine reports rather than guesses at, so a sprite that a project *did* place is
    /// placed by hand. Two sprites both stand in the middle and overlap, which is exactly what
    /// Ren'Py does with two `show`s and no `at`.
    fn draw_sprite(&mut self, draw: &mut DrawList, image: &str, width: f32, height: f32) {
        let (image_width, image_height) = match self.size_of(image) {
            Some(size) => (size.0 as f32, size.1 as f32),
            None => (width / 4.0, height / 2.0),
        };
        let left = (width - image_width) / 2.0;
        let top = height - image_height;
        let Some(texture) = self.texture_of(image) else {
            self.draw_placeholder(draw, image, left, top, image_width, image_height);
            return;
        };
        draw.push_image(ImageQuad {
            x: left,
            y: top,
            width: image_width,
            height: image_height,
            uv: [0.0, 0.0, 1.0, 1.0],
            image: texture,
            color: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
        });
    }

    /// A name with no picture behind it — a typo, or an asset the build has not produced — as a
    /// labelled block.
    ///
    /// A blank screen reads as a broken renderer; a labelled one reads as a missing background,
    /// which is what it is. `vela check` is what makes it an error rather than a placeholder.
    fn draw_placeholder(
        &mut self,
        draw: &mut DrawList,
        image: &str,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
    ) {
        let tint = tint_of(image);
        draw.push_rect(RectQuad::from_corners(
            left,
            top,
            left + width,
            top + height,
            tint,
        ));
        let font = self.font.clone();
        text::place(
            &mut self.text,
            &font,
            draw,
            image,
            Placement {
                size: self.style.size * 1.6,
                left: left + 24.0,
                top: top + 40.0,
                max_width: width,
            },
            Color {
                a: 0.9,
                ..Color::rgb(255, 255, 255)
            },
        );
    }
}
