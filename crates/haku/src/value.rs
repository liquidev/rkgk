use alloc::vec::Vec;

use crate::{compiler::ClosureSpec, system::ChunkId};

// TODO: Probably needs some pretty hardcore space optimization.
// Maybe when we have static typing.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Value {
    Nil,
    False,
    True,
    Number(f32),
    Vec4(Vec4),
    Rgba(Rgba),
    Ref(RefId),
}

impl Value {
    pub fn is_falsy(&self) -> bool {
        matches!(self, Self::Nil | Self::False)
    }

    pub fn is_truthy(&self) -> bool {
        !self.is_falsy()
    }

    pub fn to_number(&self) -> Option<f32> {
        match self {
            Self::Number(v) => Some(*v),
            _ => None,
        }
    }

    pub fn to_vec4(&self) -> Option<Vec4> {
        match self {
            Self::Vec4(v) => Some(*v),
            _ => None,
        }
    }

    pub fn to_rgba(&self) -> Option<Rgba> {
        match self {
            Self::Rgba(v) => Some(*v),
            _ => None,
        }
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Self::Nil
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        match value {
            true => Self::True,
            false => Self::False,
        }
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Self::Number(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl From<Vec4> for Vec2 {
    fn from(value: Vec4) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[repr(C)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

// NOTE: This is not a pointer, because IDs are safer and easier to clone.
//
// Since this only ever refers to refs inside the current VM, there is no need to walk through all
// the values and update pointers when a VM is cloned.
//
// This ensures it's quick and easy to spin up a new VM from an existing image, as well as being
// extremely easy to serialize a VM image into a file for quick loading back later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefId(pub(crate) u32);

impl RefId {
    // DO NOT USE outside tests!
    #[doc(hidden)]
    pub fn from_u32(x: u32) -> Self {
        Self(x)
    }
}

#[derive(Debug, Clone)]
pub enum Ref {
    Closure(Closure),
    List(List),
    Shape(Shape),
    Scribble(Scribble),
}

impl Ref {
    pub fn as_closure(&self) -> Option<&Closure> {
        match self {
            Self::Closure(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BytecodeLoc {
    pub chunk_id: ChunkId,
    pub offset: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct BytecodeSpan {
    pub loc: BytecodeLoc,
    pub len: u16,
}

#[derive(Debug, Clone, Copy)]
pub enum FunctionName {
    Anonymous,
}

#[derive(Debug, Clone)]
pub struct Closure {
    pub start: BytecodeLoc,
    pub name: FunctionName,
    pub param_count: u8,
    pub local_count: u8,
    pub captures: Vec<Value>,
}

impl Closure {
    pub fn chunk(chunk_id: ChunkId, spec: ClosureSpec) -> Self {
        Self {
            start: BytecodeLoc {
                chunk_id,
                offset: 0,
            },
            name: FunctionName::Anonymous,
            param_count: 0,
            local_count: spec.local_count,
            captures: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub elements: Vec<Value>,
}

#[derive(Debug, Clone)]
pub enum Shape {
    Point(Vec2),
    Line(Vec2, Vec2),
    Rect(Vec2, Vec2),
    Circle(Vec2, f32),
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub thickness: f32,
    pub color: Rgba,
    pub shape: Shape,
}

#[derive(Debug, Clone)]
pub struct Fill {
    pub color: Rgba,
    pub shape: Shape,
}

#[derive(Debug, Clone)]
pub enum Scribble {
    Stroke(Stroke),
    Fill(Fill),
}
