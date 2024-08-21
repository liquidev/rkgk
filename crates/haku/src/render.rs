use alloc::vec::Vec;
use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Rect, Shader,
    Stroke as SStroke, Transform,
};

use crate::{
    value::{Fill, Ref, Rgba, Scribble, Shape, Stroke, Value},
    vm::{Exception, Vm},
};

pub use tiny_skia;

pub struct RendererLimits {
    pub pixmap_stack_capacity: usize,
    pub transform_stack_capacity: usize,
}

pub enum RenderTarget<'a> {
    Borrowed(&'a mut Pixmap),
    Owned(Pixmap),
}

pub struct Renderer<'a> {
    pixmap_stack: Vec<RenderTarget<'a>>,
    transform_stack: Vec<Transform>,
}

impl<'a> Renderer<'a> {
    pub fn new(pixmap: &'a mut Pixmap, limits: &RendererLimits) -> Self {
        assert!(limits.pixmap_stack_capacity > 0);
        assert!(limits.transform_stack_capacity > 0);

        let mut blend_stack = Vec::with_capacity(limits.pixmap_stack_capacity);
        blend_stack.push(RenderTarget::Borrowed(pixmap));

        let mut transform_stack = Vec::with_capacity(limits.transform_stack_capacity);
        transform_stack.push(Transform::identity());

        Self {
            pixmap_stack: blend_stack,
            transform_stack,
        }
    }

    fn create_exception(_vm: &Vm, _at: Value, message: &'static str) -> Exception {
        Exception { message }
    }

    fn transform(&self) -> Transform {
        self.transform_stack.last().copied().unwrap()
    }

    fn transform_mut(&mut self) -> &mut Transform {
        self.transform_stack.last_mut().unwrap()
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        let translated = self.transform().post_translate(x, y);
        *self.transform_mut() = translated;
    }

    fn pixmap_mut(&mut self) -> &mut Pixmap {
        match self.pixmap_stack.last_mut().unwrap() {
            RenderTarget::Borrowed(pixmap) => pixmap,
            RenderTarget::Owned(pixmap) => pixmap,
        }
    }

    pub fn render(&mut self, vm: &Vm, value: Value) -> Result<(), Exception> {
        static NOT_A_SCRIBBLE: &str = "cannot draw something that is not a scribble";
        let (_id, scribble) = vm
            .get_ref_value(value)
            .ok_or_else(|| Self::create_exception(vm, value, NOT_A_SCRIBBLE))?;

        match &scribble {
            Ref::List(list) => {
                for element in &list.elements {
                    self.render(vm, *element)?;
                }
            }
            Ref::Scribble(scribble) => match scribble {
                Scribble::Stroke(stroke) => self.render_stroke(vm, value, stroke)?,
                Scribble::Fill(fill) => self.render_fill(vm, value, fill)?,
            },
            _ => return Err(Self::create_exception(vm, value, NOT_A_SCRIBBLE))?,
        }

        Ok(())
    }

    fn shape_to_path(shape: &Shape) -> Path {
        let mut pb = PathBuilder::new();
        match shape {
            Shape::Point(vec) => {
                pb.move_to(vec.x, vec.y);
                pb.line_to(vec.x, vec.y);
            }
            Shape::Line(start, end) => {
                pb.move_to(start.x, start.y);
                pb.line_to(end.x, end.y);
            }
            Shape::Rect(position, size) => {
                if let Some(rect) =
                    tiny_skia::Rect::from_xywh(position.x, position.y, size.x, size.y)
                {
                    pb.push_rect(rect);
                }
            }
            Shape::Circle(position, radius) => {
                pb.push_circle(position.x, position.y, *radius);
            }
        }
        pb.finish().unwrap()
    }

    fn render_stroke(&mut self, _vm: &Vm, _value: Value, stroke: &Stroke) -> Result<(), Exception> {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia_color(stroke.color)),
            ..default_paint()
        };
        let transform = self.transform();
        let path = Self::shape_to_path(&stroke.shape);

        self.pixmap_mut().stroke_path(
            &path,
            &paint,
            &SStroke {
                width: stroke.thickness,
                line_cap: LineCap::Square,
                ..Default::default()
            },
            transform,
            None,
        );

        Ok(())
    }

    fn render_fill(&mut self, _vm: &Vm, _value: Value, fill: &Fill) -> Result<(), Exception> {
        let paint = Paint {
            shader: Shader::SolidColor(tiny_skia_color(fill.color)),
            ..default_paint()
        };
        let transform = self.transform();
        let path = Self::shape_to_path(&fill.shape);

        self.pixmap_mut()
            .fill_path(&path, &paint, FillRule::EvenOdd, transform, None);

        Ok(())
    }
}

fn default_paint() -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(Color::BLACK),
        blend_mode: BlendMode::SourceOver,
        anti_alias: false,
        force_hq_pipeline: false,
    }
}

fn tiny_skia_color(color: Rgba) -> Color {
    Color::from_rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    )
    .unwrap()
}
