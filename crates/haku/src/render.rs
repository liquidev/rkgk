use core::iter;

use alloc::vec::Vec;

use crate::{
    value::{Ref, Rgba, Scribble, Shape, Stroke, Value, Vec4},
    vm::{Exception, Vm},
};

pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgba>,
}

impl Bitmap {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: Vec::from_iter(
                iter::repeat(Rgba::default()).take(width as usize * height as usize),
            ),
        }
    }

    pub fn pixel_index(&self, x: u32, y: u32) -> usize {
        x as usize + y as usize * self.width as usize
    }

    pub fn get(&self, x: u32, y: u32) -> Rgba {
        self.pixels[self.pixel_index(x, y)]
    }

    pub fn set(&mut self, x: u32, y: u32, rgba: Rgba) {
        let index = self.pixel_index(x, y);
        self.pixels[index] = rgba;
    }
}

pub struct RendererLimits {
    pub bitmap_stack_capacity: usize,
    pub transform_stack_capacity: usize,
}

pub struct Renderer {
    bitmap_stack: Vec<Bitmap>,
    transform_stack: Vec<Vec4>,
}

impl Renderer {
    pub fn new(bitmap: Bitmap, limits: &RendererLimits) -> Self {
        assert!(limits.bitmap_stack_capacity > 0);
        assert!(limits.transform_stack_capacity > 0);

        let mut blend_stack = Vec::with_capacity(limits.bitmap_stack_capacity);
        blend_stack.push(bitmap);

        let mut transform_stack = Vec::with_capacity(limits.transform_stack_capacity);
        transform_stack.push(Vec4::default());

        Self {
            bitmap_stack: blend_stack,
            transform_stack,
        }
    }

    fn create_exception(_vm: &Vm, _at: Value, message: &'static str) -> Exception {
        Exception { message }
    }

    fn transform(&self) -> &Vec4 {
        self.transform_stack.last().unwrap()
    }

    fn transform_mut(&mut self) -> &mut Vec4 {
        self.transform_stack.last_mut().unwrap()
    }

    fn bitmap(&self) -> &Bitmap {
        self.bitmap_stack.last().unwrap()
    }

    fn bitmap_mut(&mut self) -> &mut Bitmap {
        self.bitmap_stack.last_mut().unwrap()
    }

    pub fn translate(&mut self, translation: Vec4) {
        let transform = self.transform_mut();
        transform.x += translation.x;
        transform.y += translation.y;
        transform.z += translation.z;
        transform.w += translation.w;
    }

    pub fn to_bitmap_coords(&self, point: Vec4) -> Option<(u32, u32)> {
        let transform = self.transform();
        let x = point.x + transform.x;
        let y = point.y + transform.y;
        if x >= 0.0 && y >= 0.0 {
            let (x, y) = (x as u32, y as u32);
            if x < self.bitmap().width && y < self.bitmap().height {
                Some((x, y))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn render(&mut self, vm: &Vm, value: Value) -> Result<(), Exception> {
        static NOT_A_SCRIBBLE: &str = "cannot draw something that is not a scribble";
        let (_id, scribble) = vm
            .get_ref_value(value)
            .ok_or_else(|| Self::create_exception(vm, value, NOT_A_SCRIBBLE))?;
        let Ref::Scribble(scribble) = scribble else {
            return Err(Self::create_exception(vm, value, NOT_A_SCRIBBLE));
        };

        match scribble {
            Scribble::Stroke(stroke) => self.render_stroke(vm, value, stroke)?,
        }

        Ok(())
    }

    fn render_stroke(&mut self, _vm: &Vm, _value: Value, stroke: &Stroke) -> Result<(), Exception> {
        match stroke.shape {
            Shape::Point(vec) => {
                if let Some((x, y)) = self.to_bitmap_coords(vec) {
                    // TODO: thickness
                    self.bitmap_mut().set(x, y, stroke.color);
                }
            }
        }

        Ok(())
    }

    pub fn finish(mut self) -> Bitmap {
        self.bitmap_stack.drain(..).next().unwrap()
    }
}
