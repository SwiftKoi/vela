//! Renders a frame to a PNG.
//!
//! ```text
//! cargo run -p vela-render --example capture -- /tmp/frame.png
//! ```
//!
//! Kept as an example rather than a test because it *writes a file for a person to look at*.
//! The assertions live in `tests/`; this is the thing that lets someone see what the
//! renderer draws, which no assertion replaces.

use std::path::PathBuf;

use vela_render::{
    Capture, ClearStage, Color, DrawList, GeometryStage, GlyphQuad, RectQuad, RenderGraph,
};
use vela_text::{Font, TextEngine};

/// The frame size.
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

fn main() {
    let options = Options::parse();

    let font_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/LiberationSans-Regular.ttf");
    let font =
        Font::from_bytes(std::fs::read(&font_path).expect("read font"), 0).expect("load font");

    let mut text = TextEngine::new();
    text.add_font("sans", font);

    let Some(mut capture) = Capture::new(options.width, options.height) else {
        eprintln!("no GPU adapter available — nothing to render");
        std::process::exit(1);
    };

    let mut draw = DrawList::new();
    build_scene(&mut text, &mut draw, options.width, options.height);
    capture.renderer_mut().upload_atlas(text.atlas());

    let graph = default_graph();
    if options.ascii {
        print_ascii(
            &capture.render(&graph, &draw),
            options.width,
            options.height,
        );
        return;
    }
    capture
        .save(&graph, &draw, &options.out)
        .expect("write the capture");
    println!("wrote {}", options.out.display());
}

/// Command-line options, matching `tools/capture.sh`.
struct Options {
    out: PathBuf,
    width: u32,
    height: u32,
    ascii: bool,
}

impl Options {
    fn parse() -> Self {
        let mut options = Self {
            out: PathBuf::from("frame.png"),
            width: WIDTH,
            height: HEIGHT,
            ascii: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out" => {
                    options.out = PathBuf::from(args.next().expect("--out needs a path"));
                }
                "--size" => {
                    let size = args.next().expect("--size needs WxH");
                    let (width, height) =
                        size.split_once('x').expect("--size is WxH, e.g. 1280x720");
                    options.width = width.parse().expect("a numeric width");
                    options.height = height.parse().expect("a numeric height");
                }
                "--ascii" => options.ascii = true,
                // The project argument is accepted and ignored until the command stream is
                // wired in; taking it now keeps the wrapper's interface stable.
                other => eprintln!("capture: ignoring {other}"),
            }
        }
        options
    }
}

/// Prints the frame as a map of `#` where it is lit.
///
/// Exact pixels, with no image viewer and no JPEG in between. A geometry bug that looks like
/// "a shape is wrong" in a PNG is a numbered row here, which is the difference between
/// guessing and knowing.
fn print_ascii(pixels: &[u8], width: u32, height: u32) {
    for y in 0..height {
        let mut row = String::new();
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            let (r, g, b) = (pixels[index], pixels[index + 1], pixels[index + 2]);
            row.push(if r > 100 && g < 100 {
                '#'
            } else if r > 20 || g > 20 || b > 20 {
                '+'
            } else {
                '.'
            });
        }
        println!("{y:>3} {row}");
    }
}

/// The stages a frame runs.
fn default_graph() -> RenderGraph {
    let mut graph = RenderGraph::new();
    graph.push(Box::new(ClearStage {
        color: Color::rgb(18, 20, 28),
    }));
    graph.push(Box::new(GeometryStage));
    graph
}

/// A dialogue box with some text in it.
fn build_scene(text: &mut TextEngine, draw: &mut DrawList, width: u32, height: u32) {
    // A letterboxed box, in the shape a visual novel puts at the bottom.
    let box_top = height as f32 * 0.72;
    draw.push_rect(RectQuad::from_corners(
        60.0,
        box_top,
        width as f32 - 60.0,
        height as f32 - 60.0,
        Color {
            r: 0.04,
            g: 0.05,
            b: 0.09,
            a: 0.85,
        },
    ));

    let name = "Eileen";
    let body = "The rain had stopped an hour ago, but the street still shone, \
                and nobody had come to close the shutters.";

    place_text(
        text,
        draw,
        name,
        &Placement {
            size: 28.0,
            left: 92.0,
            top: box_top + 34.0,
            color: (204, 190, 150),
        },
        width,
    );
    place_text(
        text,
        draw,
        body,
        &Placement {
            size: 24.0,
            left: 92.0,
            top: box_top + 78.0,
            color: (235, 235, 240),
        },
        width,
    );
}

/// Lays out and emits one run of text.
/// Where a run of text goes and how it looks.
struct Placement {
    size: f32,
    left: f32,
    top: f32,
    color: (u8, u8, u8),
}

fn place_text(
    text: &mut TextEngine,
    draw: &mut DrawList,
    body: &str,
    placement: &Placement,
    frame_width: u32,
) {
    let Placement {
        size,
        left,
        top,
        color,
    } = *placement;
    let width = frame_width as f32 - left - 80.0;
    let Some(layout) = text.layout("sans", size, body, Some(width)) else {
        return;
    };
    let tint = Color::rgb(color.0, color.1, color.2);
    let (atlas_w, atlas_h) = text.atlas().size();

    for line in &layout.lines {
        let baseline = top + line.baseline;
        for glyph in &line.glyphs {
            // Rasterise, then place the mask against the baseline: `top` is measured upwards
            // from it, which is the font convention and the opposite of the screen's.
            let Some(rect) = text.glyph("sans", size, glyph.id) else {
                continue;
            };
            draw.push_glyph(GlyphQuad {
                x: left + glyph.x + rect.left as f32,
                y: baseline - rect.top as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                uv: [
                    rect.x as f32 / atlas_w as f32,
                    rect.y as f32 / atlas_h as f32,
                    (rect.x + rect.width) as f32 / atlas_w as f32,
                    (rect.y + rect.height) as f32 / atlas_h as f32,
                ],
                color: tint,
            });
        }
    }
}
