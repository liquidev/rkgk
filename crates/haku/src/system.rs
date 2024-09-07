use core::{
    error::Error,
    fmt::{self, Display},
};

use alloc::vec::Vec;

use crate::{
    bytecode::Chunk,
    value::Value,
    vm::{Exception, FnArgs, Vm},
};

pub type SystemFn = fn(&mut Vm, FnArgs) -> Result<Value, Exception>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemFnArity {
    Unary,
    Binary,
    Nary,
}

#[derive(Debug, Clone)]
pub struct System {
    /// Resolves a system function name to an index into `fn`s.
    pub resolve_fn: fn(SystemFnArity, &str) -> Option<u8>,
    pub fns: [Option<SystemFn>; 256],
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, Copy)]
pub struct SystemImage {
    chunks: usize,
}

macro_rules! def_fns {
    ($($index:tt $arity:tt $name:tt => $fnref:expr),* $(,)?) => {
        pub(crate) fn init_fns(system: &mut System) {
            $(
                debug_assert!(system.fns[$index].is_none());
                system.fns[$index] = Some($fnref);
            )*
        }

        pub(crate) fn resolve(arity: SystemFnArity, name: &str) -> Option<u8> {
            match (arity, name){
                $((SystemFnArity::$arity, $name) => Some($index),)*
                _ => None,
            }
        }
    };
}

impl System {
    pub fn new(max_chunks: usize) -> Self {
        assert!(max_chunks < u32::MAX as usize);

        let mut system = Self {
            resolve_fn: Self::resolve,
            fns: [None; 256],
            chunks: Vec::with_capacity(max_chunks),
        };
        Self::init_fns(&mut system);
        system
    }

    pub fn add_chunk(&mut self, chunk: Chunk) -> Result<ChunkId, ChunkError> {
        if self.chunks.len() >= self.chunks.capacity() {
            return Err(ChunkError);
        }

        let id = ChunkId(self.chunks.len() as u32);
        self.chunks.push(chunk);
        Ok(id)
    }

    pub fn chunk(&self, id: ChunkId) -> &Chunk {
        &self.chunks[id.0 as usize]
    }

    pub fn image(&self) -> SystemImage {
        SystemImage {
            chunks: self.chunks.len(),
        }
    }

    pub fn restore_image(&mut self, image: &SystemImage) {
        self.chunks.resize_with(image.chunks, || {
            panic!("image must be a subset of the current system")
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkError;

impl Display for ChunkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("too many chunks")
    }
}

impl Error for ChunkError {}

pub mod fns {
    use alloc::{format, vec::Vec};

    use crate::{
        value::{Fill, List, Ref, Rgba, Scribble, Shape, Stroke, Value, Vec2, Vec4},
        vm::{Exception, FnArgs, Vm},
    };

    use super::{System, SystemFnArity};

    impl System {
        def_fns! {
            0x00 Binary "+" => add,
            0x01 Binary "-" => sub,
            0x02 Binary "*" => mul,
            0x03 Binary "/" => div,
            0x04 Unary "-" => neg,

            0x10 Nary "floor" => floorf,
            0x11 Nary "ceil" => ceilf,
            0x12 Nary "round" => roundf,
            0x13 Nary "abs" => fabsf,
            0x14 Nary "mod" => fmodf,
            0x15 Nary "pow" => powf,
            0x16 Nary "sqrt" => sqrtf,
            0x17 Nary "cbrt" => cbrtf,
            0x18 Nary "exp" => expf,
            0x19 Nary "exp2" => exp2f,
            0x1A Nary "ln" => logf,
            0x1B Nary "log2" => log2f,
            0x1C Nary "log10" => log10f,
            0x1D Nary "hypot" => hypotf,
            0x1E Nary "sin" => sinf,
            0x1F Nary "cos" => cosf,
            0x20 Nary "tan" => tanf,
            0x21 Nary "asin" => asinf,
            0x22 Nary "acos" => acosf,
            0x23 Nary "atan" => atanf,
            0x24 Nary "atan2" => atan2f,
            0x25 Nary "expMinus1" => expm1f,
            0x26 Nary "ln1Plus" => log1pf,
            0x27 Nary "sinh" => sinhf,
            0x28 Nary "cosh" => coshf,
            0x29 Nary "tanh" => tanhf,
            0x2A Nary "asinh" => asinhf,
            0x2B Nary "acosh" => acoshf,
            0x2C Nary "atanh" => atanhf,

            0x40 Unary "!" => not,
            0x41 Binary "==" => eq,
            0x42 Binary "!=" => neq,
            0x43 Binary "<" => lt,
            0x44 Binary "<=" => leq,
            0x45 Binary ">" => gt,
            0x46 Binary ">=" => geq,

            0x80 Nary "vec" => vec,
            0x81 Nary "vecX" => vec_x,
            0x82 Nary "vecY" => vec_y,
            0x83 Nary "vecZ" => vec_z,
            0x84 Nary "vecW" => vec_w,

            0x85 Nary "rgba" => rgba,
            0x86 Nary "rgbaR" => rgba_r,
            0x87 Nary "rgbaG" => rgba_g,
            0x88 Nary "rgbaB" => rgba_b,
            0x89 Nary "rgbaA" => rgba_a,

            // NOTE: Not used right now, has been replaced with Opcode::List.
            // Keeping it around to reserve a slot for data structure operations.
            0x90 Nary "list (unused)" => list,

            0xc0 Nary "toShape" => to_shape_f,
            0xc1 Nary "line" => line,
            0xc2 Nary "rect" => rect,
            0xc3 Nary "circle" => circle,
            0xe0 Nary "stroke" => stroke,
            0xe1 Nary "fill" => fill,
        }
    }

    pub fn add(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get_number(vm, 0, "arguments to `+` must be numbers")?;
        let b = args.get_number(vm, 1, "arguments to `+` must be numbers")?;
        Ok(Value::Number(a + b))
    }

    pub fn sub(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get_number(vm, 0, "arguments to `-` must be numbers")?;
        let b = args.get_number(vm, 1, "arguments to `-` must be numbers")?;
        Ok(Value::Number(a - b))
    }

    pub fn mul(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get_number(vm, 0, "arguments to `*` must be numbers")?;
        let b = args.get_number(vm, 1, "arguments to `*` must be numbers")?;
        Ok(Value::Number(a * b))
    }

    pub fn div(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get_number(vm, 0, "arguments to `/` must be numbers")?;
        let b = args.get_number(vm, 1, "arguments to `/` must be numbers")?;
        Ok(Value::Number(a / b))
    }

    pub fn neg(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let x = args.get_number(vm, 0, "`-` can only work with numbers")?;
        Ok(Value::Number(-x))
    }

    #[inline(never)]
    fn math1(vm: &mut Vm, args: FnArgs, name: &str, f: fn(f32) -> f32) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(
                vm.create_exception(format!("`{name}` expects a single argument ({name} x)"))
            );
        }

        let x = args
            .get(vm, 0)
            .to_number()
            .ok_or_else(|| vm.create_exception(format!("`{name}` argument must be a number")))?;
        Ok(Value::Number(f(x)))
    }

    #[inline(never)]
    fn math2(
        vm: &mut Vm,
        args: FnArgs,
        name: &str,
        f: fn(f32, f32) -> f32,
    ) -> Result<Value, Exception> {
        if args.num() != 2 {
            return Err(vm.create_exception(format!("`{name}` expects two arguments ({name} x y)")));
        }

        let x = args
            .get(vm, 0)
            .to_number()
            .ok_or_else(|| vm.create_exception(format!("`{name}` arguments must be numbers")))?;
        let y = args
            .get(vm, 1)
            .to_number()
            .ok_or_else(|| vm.create_exception(format!("`{name}` arguments must be numbers")))?;
        Ok(Value::Number(f(x, y)))
    }

    macro_rules! math_fns {
        ($($arity:tt $sysname:tt $name:tt),* $(,)?) => {
            $(
                pub fn $name(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
                    $arity(vm, args, $sysname, libm::$name)
                }
            )*
        }
    }

    math_fns! {
        math1 "floor" floorf,
        math1 "ceil" ceilf,
        math1 "round" roundf,
        math1 "abs" fabsf,
        math2 "mod" fmodf,
        math2 "pow" powf,
        math1 "sqrt" sqrtf,
        math1 "cbrt" cbrtf,
        math1 "exp" expf,
        math1 "exp2" exp2f,
        math1 "ln" logf,
        math1 "log2" log2f,
        math1 "log10" log10f,
        math2 "hypot" hypotf,
        math1 "sin" sinf,
        math1 "cos" cosf,
        math1 "tan" tanf,
        math1 "asin" asinf,
        math1 "acos" acosf,
        math1 "atan" atanf,
        math2 "atan2" atan2f,
        math1 "expMinus1" expm1f,
        math1 "ln1Plus" log1pf,
        math1 "sinh" sinhf,
        math1 "cosh" coshf,
        math1 "tanh" tanhf,
        math1 "asinh" asinhf,
        math1 "acosh" acoshf,
        math1 "atanh" atanhf,
    }

    pub fn not(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let value = args.get(vm, 0);
        Ok(Value::from(value.is_falsy()))
    }

    pub fn eq(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get(vm, 0);
        let b = args.get(vm, 1);
        Ok(Value::from(a == b))
    }

    pub fn neq(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get(vm, 0);
        let b = args.get(vm, 1);
        Ok(Value::from(a != b))
    }

    pub fn lt(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get(vm, 0);
        let b = args.get(vm, 1);
        Ok(Value::from(a < b))
    }

    pub fn leq(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get(vm, 0);
        let b = args.get(vm, 1);
        Ok(Value::from(a <= b))
    }

    pub fn gt(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get(vm, 0);
        let b = args.get(vm, 1);
        Ok(Value::from(a > b))
    }

    pub fn geq(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let a = args.get(vm, 0);
        let b = args.get(vm, 1);
        Ok(Value::from(a >= b))
    }

    pub fn vec(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        static ERROR: &str = "arguments to `vec` must be numbers (vec x y z w)";
        match args.num() {
            1 => {
                let x = args.get_number(vm, 0, ERROR)?;
                Ok(Value::Vec4(Vec4 {
                    x,
                    y: 0.0,
                    z: 0.0,
                    w: 0.0,
                }))
            }
            2 => {
                let x = args.get_number(vm, 0, ERROR)?;
                let y = args.get_number(vm, 1, ERROR)?;
                Ok(Value::Vec4(Vec4 {
                    x,
                    y,
                    z: 0.0,
                    w: 0.0,
                }))
            }
            3 => {
                let x = args.get_number(vm, 0, ERROR)?;
                let y = args.get_number(vm, 1, ERROR)?;
                let z = args.get_number(vm, 2, ERROR)?;
                Ok(Value::Vec4(Vec4 { x, y, z, w: 0.0 }))
            }
            4 => {
                let x = args.get_number(vm, 0, ERROR)?;
                let y = args.get_number(vm, 1, ERROR)?;
                let z = args.get_number(vm, 2, ERROR)?;
                let w = args.get_number(vm, 3, ERROR)?;
                Ok(Value::Vec4(Vec4 { x, y, z, w }))
            }
            _ => Err(vm.create_exception("`vec` expects 1-4 arguments (vec x y z w)")),
        }
    }

    pub fn vec_x(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`vecX` expects a single argument (vecX vec)"));
        }

        let vec = args.get_vec4(vm, 0, "argument to (vecX vec) must be a `vec`")?;
        Ok(Value::Number(vec.x))
    }

    pub fn vec_y(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`vecY` expects a single argument (vecY vec)"));
        }

        let vec = args.get_vec4(vm, 0, "argument to (vecY vec) must be a `vec`")?;
        Ok(Value::Number(vec.y))
    }

    pub fn vec_z(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`vecZ` expects a single argument (vecZ vec)"));
        }

        let vec = args.get_vec4(vm, 0, "argument to (vecZ vec) must be a `vec`")?;
        Ok(Value::Number(vec.z))
    }

    pub fn vec_w(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`vecW` expects a single argument (vecW vec)"));
        }

        let vec = args.get_vec4(vm, 0, "argument to (vecW vec) must be a `vec`")?;
        Ok(Value::Number(vec.w))
    }

    pub fn rgba(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 4 {
            return Err(vm.create_exception("`rgba` expects four arguments (rgba r g b a)"));
        }

        static ERROR: &str = "arguments to (rgba r g b a) must be numbers";
        let r = args.get_number(vm, 0, ERROR)?;
        let g = args.get_number(vm, 1, ERROR)?;
        let b = args.get_number(vm, 2, ERROR)?;
        let a = args.get_number(vm, 3, ERROR)?;

        Ok(Value::Rgba(Rgba { r, g, b, a }))
    }

    pub fn rgba_r(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`rgbaR` expects a single argument (rgbaR rgba)"));
        }

        let rgba = args.get_rgba(vm, 0, "argument to (rgbaR rgba) must be an `rgba`")?;
        Ok(Value::Number(rgba.r))
    }

    pub fn rgba_g(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`rgbaG` expects a single argument (rgbaG rgba)"));
        }

        let rgba = args.get_rgba(vm, 0, "argument to (rgbaG rgba) must be an `rgba`")?;
        Ok(Value::Number(rgba.r))
    }

    pub fn rgba_b(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`rgbaB` expects a single argument (rgbaB rgba)"));
        }

        let rgba = args.get_rgba(vm, 0, "argument to (rgbaB rgba) must be an `rgba`")?;
        Ok(Value::Number(rgba.r))
    }

    pub fn rgba_a(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`rgbaA` expects a single argument (rgbaA rgba)"));
        }

        let rgba = args.get_rgba(vm, 0, "argument to (rgbaA rgba) must be an `rgba`")?;
        Ok(Value::Number(rgba.r))
    }

    pub fn list(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        let elements: Vec<_> = (0..args.num()).map(|i| args.get(vm, i)).collect();
        vm.track_array(&elements)?;
        let id = vm.create_ref(Ref::List(List { elements }))?;
        Ok(Value::Ref(id))
    }

    fn to_shape(value: Value, vm: &Vm) -> Option<Shape> {
        match value {
            Value::Nil | Value::False | Value::True | Value::Number(_) | Value::Rgba(_) => None,
            Value::Ref(id) => {
                if let Ref::Shape(shape) = vm.get_ref(id) {
                    Some(shape.clone())
                } else {
                    None
                }
            }
            Value::Vec4(vec) => Some(Shape::Point(vec.into())),
        }
    }

    pub fn to_shape_f(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 1 {
            return Err(vm.create_exception("`toShape` expects 1 argument (toShape value)"));
        }

        if let Some(shape) = to_shape(args.get(vm, 0), vm) {
            let id = vm.create_ref(Ref::Shape(shape))?;
            Ok(Value::Ref(id))
        } else {
            Ok(Value::Nil)
        }
    }

    pub fn line(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 2 {
            return Err(vm.create_exception("`line` expects 2 arguments (line start end)"));
        }

        static ERROR: &str = "arguments to `line` must be `vec`";
        let start = args.get_vec4(vm, 0, ERROR)?;
        let end = args.get_vec4(vm, 1, ERROR)?;

        let id = vm.create_ref(Ref::Shape(Shape::Line(start.into(), end.into())))?;
        Ok(Value::Ref(id))
    }

    pub fn rect(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        static ARGS2: &str = "arguments to 2-argument `rect` must be `vec`";
        static ARGS4: &str = "arguments to 4-argument `rect` must be numbers";

        let (position, size) = match args.num() {
            2 => (args.get_vec4(vm, 0, ARGS2)?.into(), args.get_vec4(vm, 1, ARGS2)?.into()),
            4 => (
                Vec2 {
                    x: args.get_number(vm, 0, ARGS4)?,
                    y: args.get_number(vm, 1, ARGS4)?,
                },
                Vec2 {
                    x: args.get_number(vm, 2, ARGS4)?,
                    y: args.get_number(vm, 3, ARGS4)?,
                },
            ),
            _ => return Err(vm.create_exception("`rect` expects 2 arguments (rect position size) or 4 arguments (rect x y width height)"))
        };

        let id = vm.create_ref(Ref::Shape(Shape::Rect(position, size)))?;
        Ok(Value::Ref(id))
    }

    pub fn circle(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        static ARGS2: &str = "arguments to 2-argument `circle` must be `vec` and a number";
        static ARGS3: &str = "arguments to 3-argument `circle` must be numbers";

        let (position, radius) = match args.num() {
            2 => (args.get_vec4(vm, 0, ARGS2)?.into(), args.get_number(vm, 1, ARGS2)?),
            3 => (
                Vec2 {
                    x: args.get_number(vm, 0, ARGS3)?,
                    y: args.get_number(vm, 1, ARGS3)?,
                },
                args.get_number(vm, 2, ARGS3)?
            ),
            _ => return Err(vm.create_exception("`circle` expects 2 arguments (circle position radius) or 3 arguments (circle x y radius)"))
        };

        let id = vm.create_ref(Ref::Shape(Shape::Circle(position, radius)))?;
        Ok(Value::Ref(id))
    }

    pub fn stroke(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 3 {
            return Err(
                vm.create_exception("`stroke` expects 3 arguments (stroke thickness color shape)")
            );
        }

        let thickness = args.get_number(
            vm,
            0,
            "1st argument to `stroke` must be a thickness in pixels (number)",
        )?;
        let color = args.get_rgba(vm, 1, "2nd argument to `stroke` must be a color (rgba)")?;
        if let Some(shape) = to_shape(args.get(vm, 2), vm) {
            let id = vm.create_ref(Ref::Scribble(Scribble::Stroke(Stroke {
                thickness,
                color,
                shape,
            })))?;
            Ok(Value::Ref(id))
        } else {
            Ok(Value::Nil)
        }
    }

    pub fn fill(vm: &mut Vm, args: FnArgs) -> Result<Value, Exception> {
        if args.num() != 2 {
            return Err(vm.create_exception("`fill` expects 2 arguments (fill color shape)"));
        }

        let color = args.get_rgba(vm, 0, "1st argument to `fill` must be a color (rgba)")?;
        if let Some(shape) = to_shape(args.get(vm, 1), vm) {
            let id = vm.create_ref(Ref::Scribble(Scribble::Fill(Fill { color, shape })))?;
            Ok(Value::Ref(id))
        } else {
            Ok(Value::Nil)
        }
    }
}
