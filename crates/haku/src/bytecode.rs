use core::{
    fmt::{self, Display},
    mem::transmute,
};

use alloc::{borrow::ToOwned, string::String, vec::Vec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    // Push literal values onto the stack.
    Nil,
    False,
    True,
    Number, // (float: f32)

    // Duplicate existing values.
    /// Push a value relative to the bottom of the current stack window.
    Local, // (index: u8)
    /// Set the value of a value relative to the bottom of the current stack window.
    SetLocal, // (index: u8)
    /// Push a captured value.
    Capture, // (index: u8)
    /// Get the value of a definition.
    Def, // (index: u16)
    /// Set the value of a definition.
    SetDef, // (index: u16)

    // Create literal functions.
    Function, // (params: u8, then: u16), at `then`: (local_count: u8, capture_count: u8, captures: [(source: u8, index: u8); capture_count])

    // Control flow.
    Jump,      // (offset: u16)
    JumpIfNot, // (offset: u16)

    // Function calls.
    Call, // (argc: u8)
    /// This is a fast path for system calls, which are quite common (e.g. basic arithmetic.)
    System, // (index: u8, argc: u8)

    Return,
    // NOTE: There must be no more opcodes after this.
    // They will get treated as invalid.
}

// Constants used by the Function opcode to indicate capture sources.
pub const CAPTURE_LOCAL: u8 = 0;
pub const CAPTURE_CAPTURE: u8 = 1;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub bytecode: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offset(u16);

impl Chunk {
    pub fn new(capacity: usize) -> Result<Chunk, ChunkSizeError> {
        if capacity <= (1 << 16) {
            Ok(Chunk {
                bytecode: Vec::with_capacity(capacity),
            })
        } else {
            Err(ChunkSizeError)
        }
    }

    pub fn offset(&self) -> Offset {
        Offset(self.bytecode.len() as u16)
    }

    pub fn emit_bytes(&mut self, bytes: &[u8]) -> Result<Offset, EmitError> {
        if self.bytecode.len() + bytes.len() > self.bytecode.capacity() {
            return Err(EmitError);
        }

        let offset = Offset(self.bytecode.len() as u16);
        self.bytecode.extend_from_slice(bytes);

        Ok(offset)
    }

    pub fn emit_opcode(&mut self, opcode: Opcode) -> Result<Offset, EmitError> {
        self.emit_bytes(&[opcode as u8])
    }

    pub fn emit_u8(&mut self, x: u8) -> Result<Offset, EmitError> {
        self.emit_bytes(&[x])
    }

    pub fn emit_u16(&mut self, x: u16) -> Result<Offset, EmitError> {
        self.emit_bytes(&x.to_le_bytes())
    }

    pub fn emit_u32(&mut self, x: u32) -> Result<Offset, EmitError> {
        self.emit_bytes(&x.to_le_bytes())
    }

    pub fn emit_f32(&mut self, x: f32) -> Result<Offset, EmitError> {
        self.emit_bytes(&x.to_le_bytes())
    }

    pub fn patch_u8(&mut self, offset: Offset, x: u8) {
        self.bytecode[offset.0 as usize] = x;
    }

    pub fn patch_u16(&mut self, offset: Offset, x: u16) {
        let b = x.to_le_bytes();
        let i = offset.0 as usize;
        self.bytecode[i] = b[0];
        self.bytecode[i + 1] = b[1];
    }

    pub fn patch_offset(&mut self, offset: Offset, x: Offset) {
        self.patch_u16(offset, x.0);
    }

    // NOTE: I'm aware these aren't the fastest implementations since they validate quite a lot
    // during runtime, but this is just an MVP. It doesn't have to be blazingly fast.

    pub fn read_u8(&self, pc: &mut usize) -> Result<u8, ReadError> {
        let x = self.bytecode.get(*pc).copied();
        *pc += 1;
        x.ok_or(ReadError)
    }

    pub fn read_u16(&self, pc: &mut usize) -> Result<u16, ReadError> {
        let xs = &self.bytecode[*pc..*pc + 2];
        *pc += 2;
        Ok(u16::from_le_bytes(xs.try_into().map_err(|_| ReadError)?))
    }

    pub fn read_u32(&self, pc: &mut usize) -> Result<u32, ReadError> {
        let xs = &self.bytecode[*pc..*pc + 4];
        *pc += 4;
        Ok(u32::from_le_bytes(xs.try_into().map_err(|_| ReadError)?))
    }

    pub fn read_f32(&self, pc: &mut usize) -> Result<f32, ReadError> {
        let xs = &self.bytecode[*pc..*pc + 4];
        *pc += 4;
        Ok(f32::from_le_bytes(xs.try_into().map_err(|_| ReadError)?))
    }

    pub fn read_opcode(&self, pc: &mut usize) -> Result<Opcode, ReadError> {
        let x = self.read_u8(pc)?;
        if x <= Opcode::Return as u8 {
            Ok(unsafe { transmute::<u8, Opcode>(x) })
        } else {
            Err(ReadError)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkSizeError;

impl Display for ChunkSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "chunk size must be less than 64 KiB")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitError;

impl Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "out of space in chunk")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadError;

impl Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid bytecode: out of bounds read or invalid opcode")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DefId(u16);

impl DefId {
    pub fn to_u16(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct Defs {
    defs: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct DefsImage {
    defs: usize,
}

impl Defs {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity < u16::MAX as usize + 1);
        Self {
            defs: Vec::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> u16 {
        self.defs.len() as u16
    }

    pub fn is_empty(&self) -> bool {
        self.len() != 0
    }

    pub fn get(&mut self, name: &str) -> Option<DefId> {
        self.defs
            .iter()
            .position(|n| *n == name)
            .map(|index| DefId(index as u16))
    }

    pub fn add(&mut self, name: &str) -> Result<DefId, DefError> {
        if self.defs.iter().any(|n| n == name) {
            Err(DefError::Exists)
        } else {
            if self.defs.len() >= self.defs.capacity() {
                return Err(DefError::OutOfSpace);
            }
            let id = DefId(self.defs.len() as u16);
            self.defs.push(name.to_owned());
            Ok(id)
        }
    }

    pub fn image(&self) -> DefsImage {
        DefsImage {
            defs: self.defs.len(),
        }
    }

    pub fn restore_image(&mut self, image: &DefsImage) {
        self.defs.resize_with(image.defs, || {
            panic!("image must be a subset of the current defs")
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefError {
    Exists,
    OutOfSpace,
}

impl Display for DefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DefError::Exists => "definition already exists",
            DefError::OutOfSpace => "too many definitions",
        })
    }
}
