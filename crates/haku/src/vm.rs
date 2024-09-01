use core::{
    error::Error,
    fmt::{self, Display},
    iter,
};

use alloc::{string::String, vec::Vec};
use log::info;

use crate::{
    bytecode::{self, Defs, Opcode, CAPTURE_CAPTURE, CAPTURE_LOCAL},
    system::{ChunkId, System},
    value::{BytecodeLoc, Closure, FunctionName, List, Ref, RefId, Rgba, Value, Vec4},
};

pub struct VmLimits {
    pub stack_capacity: usize,
    pub call_stack_capacity: usize,
    pub ref_capacity: usize,
    pub fuel: usize,
    pub memory: usize,
}

#[derive(Debug, Clone)]
pub struct Vm {
    stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
    refs: Vec<Ref>,
    defs: Vec<Value>,
    fuel: usize,
    memory: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct VmImage {
    stack: usize,
    call_stack: usize,
    refs: usize,
    defs: usize,
    fuel: usize,
    memory: usize,
}

#[derive(Debug, Clone)]
struct CallFrame {
    closure_id: RefId,
    chunk_id: ChunkId,
    pc: usize,
    bottom: usize,
}

struct Context {
    fuel: usize,
}

impl Vm {
    pub fn new(defs: &Defs, limits: &VmLimits) -> Self {
        Self {
            stack: Vec::with_capacity(limits.stack_capacity),
            call_stack: Vec::with_capacity(limits.call_stack_capacity),
            refs: Vec::with_capacity(limits.ref_capacity),
            defs: Vec::from_iter(iter::repeat(Value::Nil).take(defs.len() as usize)),
            fuel: limits.fuel,
            memory: limits.memory,
        }
    }

    pub fn remaining_fuel(&self) -> usize {
        self.fuel
    }

    pub fn set_fuel(&mut self, fuel: usize) {
        self.fuel = fuel;
    }

    pub fn image(&self) -> VmImage {
        assert!(
            self.stack.is_empty() && self.call_stack.is_empty(),
            "cannot image VM while running code"
        );
        VmImage {
            stack: self.stack.len(),
            call_stack: self.call_stack.len(),
            refs: self.refs.len(),
            defs: self.defs.len(),
            fuel: self.fuel,
            memory: self.memory,
        }
    }

    pub fn restore_image(&mut self, image: &VmImage) {
        // NOTE: My initial assumption here was that system functions should not be able to restore
        // the VM if it's running code.
        // Turns out that was a bad assumption to make, because the VM may fail with an exception,
        // in which case the call stack and stack may not be empty.
        // assert!(
        //     self.stack.is_empty() && self.call_stack.is_empty(),
        //     "cannot restore VM image while running code"
        // );

        self.stack.resize_with(image.stack, || {
            panic!("image must be a subset of the current VM")
        });
        self.call_stack.resize_with(image.call_stack, || {
            panic!("image must be a subset of the current VM")
        });
        self.refs.resize_with(image.refs, || {
            panic!("image must be a subset of the current VM")
        });
        self.defs.resize_with(image.defs, || {
            panic!("image must be a subset of the current VM")
        });
        self.fuel = image.fuel;
        self.memory = image.memory;
    }

    pub fn apply_defs(&mut self, defs: &Defs) {
        assert!(
            defs.len() as usize >= self.defs.len(),
            "defs must be a superset of the current VM"
        );
        self.defs.resize(defs.len() as usize, Value::Nil);
    }

    fn push(&mut self, value: Value) -> Result<(), Exception> {
        if self.stack.len() >= self.stack.capacity() {
            return Err(self.create_exception(
                "too many temporary values (local variables and expression operands)",
            ));
        }
        self.stack.push(value);
        Ok(())
    }

    fn get(&mut self, index: usize) -> Result<Value, Exception> {
        self.stack.get(index).copied().ok_or_else(|| {
            self.create_exception("corrupted bytecode (local variable out of bounds)")
        })
    }

    fn get_mut(&mut self, index: usize) -> Result<&mut Value, Exception> {
        if self.stack.get(index).is_some() {
            Ok(&mut self.stack[index])
        } else {
            Err(self.create_exception("corrupted bytecode (set local variable out of bounds)"))
        }
    }

    fn pop(&mut self) -> Result<Value, Exception> {
        self.stack
            .pop()
            .ok_or_else(|| self.create_exception("corrupted bytecode (value stack underflow)"))
    }

    fn push_call(&mut self, frame: CallFrame) -> Result<(), Exception> {
        if self.call_stack.len() >= self.call_stack.capacity() {
            return Err(self.create_exception("too much recursion"));
        }
        self.call_stack.push(frame);
        Ok(())
    }

    fn pop_call(&mut self) -> Result<CallFrame, Exception> {
        self.call_stack
            .pop()
            .ok_or_else(|| self.create_exception("corrupted bytecode (call stack underflow)"))
    }

    pub fn run(&mut self, system: &System, mut closure_id: RefId) -> Result<Value, Exception> {
        let closure = self
            .get_ref(closure_id)
            .as_closure()
            .expect("a Closure-type Ref must be passed to `run`");

        let mut chunk_id = closure.start.chunk_id;
        let mut chunk = system.chunk(chunk_id);
        let mut pc = closure.start.offset as usize;
        let mut bottom = self.stack.len();
        let mut fuel = self.fuel;

        let init_bottom = bottom;
        for _ in 0..closure.local_count {
            self.push(Value::Nil)?;
        }

        #[allow(unused)]
        let closure = (); // Do not use `closure` after this! Use `get_ref` on `closure_id` instead.

        self.push_call(CallFrame {
            closure_id,
            chunk_id,
            pc,
            bottom,
        })?;

        loop {
            fuel = fuel
                .checked_sub(1)
                .ok_or_else(|| self.create_exception("code ran for too long"))?;

            let opcode = chunk.read_opcode(&mut pc)?;
            match opcode {
                Opcode::Nil => self.push(Value::Nil)?,
                Opcode::False => self.push(Value::False)?,
                Opcode::True => self.push(Value::True)?,

                Opcode::Number => {
                    let x = chunk.read_f32(&mut pc)?;
                    self.push(Value::Number(x))?;
                }

                Opcode::Rgba => {
                    let r = chunk.read_u8(&mut pc)?;
                    let g = chunk.read_u8(&mut pc)?;
                    let b = chunk.read_u8(&mut pc)?;
                    let a = chunk.read_u8(&mut pc)?;
                    self.push(Value::Rgba(Rgba {
                        r: r as f32 / 255.0,
                        g: g as f32 / 255.0,
                        b: b as f32 / 255.0,
                        a: a as f32 / 255.0,
                    }))?;
                }

                Opcode::Local => {
                    let index = chunk.read_u8(&mut pc)? as usize;
                    let value = self.get(bottom + index)?;
                    self.push(value)?;
                }

                Opcode::SetLocal => {
                    let index = chunk.read_u8(&mut pc)? as usize;
                    let new_value = self.pop()?;
                    *self.get_mut(index)? = new_value;
                }

                Opcode::Capture => {
                    let index = chunk.read_u8(&mut pc)? as usize;
                    let closure = self.get_ref(closure_id).as_closure().unwrap();
                    self.push(closure.captures.get(index).copied().ok_or_else(|| {
                        self.create_exception("corrupted bytecode (capture index out of bounds)")
                    })?)?;
                }

                Opcode::Def => {
                    let index = chunk.read_u16(&mut pc)? as usize;
                    self.push(self.defs.get(index).copied().ok_or_else(|| {
                        self.create_exception("corrupted bytecode (def index out of bounds)")
                    })?)?
                }

                Opcode::SetDef => {
                    let index = chunk.read_u16(&mut pc)? as usize;
                    let value = self.pop()?;
                    if let Some(def) = self.defs.get_mut(index) {
                        *def = value;
                    } else {
                        return Err(self
                            .create_exception("corrupted bytecode (set def index out of bounds)"));
                    }
                }

                Opcode::List => {
                    let len = chunk.read_u16(&mut pc)? as usize;
                    let bottom = self.stack.len().checked_sub(len).ok_or_else(|| {
                        self.create_exception(
                            "corrupted bytecode (list has more elements than stack)",
                        )
                    })?;
                    let elements = self.stack[bottom..].to_vec();
                    self.stack.resize_with(bottom, || unreachable!());
                    self.track_array(&elements)?;
                    let id = self.create_ref(Ref::List(List { elements }))?;
                    self.push(Value::Ref(id))?;
                }

                Opcode::Function => {
                    let param_count = chunk.read_u8(&mut pc)?;
                    let then = chunk.read_u16(&mut pc)? as usize;
                    let body = pc;
                    pc = then;

                    let local_count = chunk.read_u8(&mut pc)?;

                    let capture_count = chunk.read_u8(&mut pc)? as usize;
                    let mut captures = Vec::with_capacity(capture_count);
                    for _ in 0..capture_count {
                        let capture_kind = chunk.read_u8(&mut pc)?;
                        let index = chunk.read_u8(&mut pc)? as usize;
                        captures.push(match capture_kind {
                            CAPTURE_LOCAL => self.get(bottom + index)?,
                            CAPTURE_CAPTURE => {
                                let closure = self.get_ref(closure_id).as_closure().unwrap();
                                closure.captures.get(index).copied().ok_or_else(|| {
                                    self.create_exception(
                                        "corrupted bytecode (captured capture index out of bounds)",
                                    )
                                })?
                            }
                            _ => Value::Nil,
                        })
                    }

                    let id = self.create_ref(Ref::Closure(Closure {
                        start: BytecodeLoc {
                            chunk_id,
                            offset: body as u16,
                        },
                        name: FunctionName::Anonymous,
                        param_count,
                        local_count,
                        captures,
                    }))?;
                    self.push(Value::Ref(id))?;
                }

                Opcode::Jump => {
                    let offset = chunk.read_u16(&mut pc)? as usize;
                    pc = offset;
                }

                Opcode::JumpIfNot => {
                    let offset = chunk.read_u16(&mut pc)? as usize;
                    let value = self.pop()?;
                    if !value.is_truthy() {
                        pc = offset;
                    }
                }

                Opcode::Call => {
                    let argument_count = chunk.read_u8(&mut pc)? as usize;

                    let function_value = self.pop()?;
                    let Some((called_closure_id, Ref::Closure(closure))) =
                        self.get_ref_value(function_value)
                    else {
                        return Err(self.create_exception("attempt to call non-function value"));
                    };

                    // TODO: Varargs?
                    if argument_count != closure.param_count as usize {
                        // Would be nice if we told the user the exact counts.
                        return Err(self.create_exception("function parameter count mismatch"));
                    }

                    let frame = CallFrame {
                        closure_id,
                        chunk_id,
                        pc,
                        bottom,
                    };

                    closure_id = called_closure_id;
                    chunk_id = closure.start.chunk_id;
                    chunk = system.chunk(chunk_id);
                    pc = closure.start.offset as usize;
                    bottom = self
                        .stack
                        .len()
                        .checked_sub(argument_count)
                        .ok_or_else(|| {
                            self.create_exception(
                                "corrupted bytecode (not enough values on the stack for arguments)",
                            )
                        })?;

                    // NOTE: Locals are only pushed _after_ we do any stack calculations.
                    for _ in 0..closure.local_count {
                        self.push(Value::Nil)?;
                    }

                    self.push_call(frame)?;
                }

                Opcode::System => {
                    let index = chunk.read_u8(&mut pc)? as usize;
                    let argument_count = chunk.read_u8(&mut pc)? as usize;
                    let system_fn = system.fns.get(index).copied().flatten().ok_or_else(|| {
                        self.create_exception("corrupted bytecode (invalid system function index)")
                    })?;

                    self.store_context(Context { fuel });
                    let result = system_fn(
                        self,
                        FnArgs {
                            base: self
                                .stack
                                .len()
                                .checked_sub(argument_count)
                                .ok_or_else(|| self.create_exception("corrupted bytecode (not enough values on the stack for arguments)"))?,
                            len: argument_count,
                        },
                    )?;
                    Context { fuel } = self.restore_context();

                    self.stack
                        .resize_with(self.stack.len() - argument_count, || unreachable!());
                    self.push(result)?;
                }

                Opcode::Return => {
                    let value = self.pop()?;
                    let frame = self.pop_call()?;

                    debug_assert!(bottom <= self.stack.len());
                    self.stack.resize_with(bottom, || unreachable!());
                    self.push(value)?;

                    // Once the initial frame is popped, halt the VM.
                    if self.call_stack.is_empty() {
                        self.store_context(Context { fuel });
                        break;
                    }

                    CallFrame {
                        closure_id,
                        chunk_id,
                        pc,
                        bottom,
                    } = frame;
                    chunk = system.chunk(chunk_id);
                }
            }
        }

        let result = self
            .stack
            .pop()
            .expect("there should be a result at the top of the stack");
        self.stack.resize_with(init_bottom, || unreachable!());

        Ok(result)
    }

    fn store_context(&mut self, context: Context) {
        self.fuel = context.fuel;
    }

    fn restore_context(&mut self) -> Context {
        Context { fuel: self.fuel }
    }

    pub fn create_ref(&mut self, r: Ref) -> Result<RefId, Exception> {
        if self.refs.len() >= self.refs.capacity() {
            return Err(self.create_exception("too many value allocations"));
        }

        let id = RefId(self.refs.len() as u32);
        self.refs.push(r);
        Ok(id)
    }

    pub fn get_ref(&self, id: RefId) -> &Ref {
        &self.refs[id.0 as usize]
    }

    pub fn get_ref_value(&self, value: Value) -> Option<(RefId, &Ref)> {
        match value {
            Value::Ref(id) => Some((id, self.get_ref(id))),
            _ => None,
        }
    }

    pub fn create_exception(&self, message: impl Into<String>) -> Exception {
        Exception {
            message: message.into(),
        }
    }

    pub fn track_array<T>(&mut self, array: &[T]) -> Result<(), Exception> {
        self.memory = self
            .memory
            .checked_sub(core::mem::size_of_val(array))
            .ok_or_else(|| self.create_exception("out of heap memory"))?;
        Ok(())
    }
}

pub struct FnArgs {
    base: usize,
    len: usize,
}

impl FnArgs {
    pub fn num(&self) -> usize {
        self.len
    }

    pub fn try_get(&self, vm: &Vm, index: usize) -> Option<Value> {
        if index < self.len {
            Some(vm.stack[self.base + index])
        } else {
            None
        }
    }

    // The following are #[inline(never)] wrappers for common operations to reduce code size.

    #[inline(never)]
    pub fn get(&self, vm: &Vm, index: usize) -> Value {
        self.try_get(vm, index)
            .expect("argument was expected, but got None")
    }

    #[inline(never)]
    pub fn get_number(
        &self,
        vm: &Vm,
        index: usize,
        message: &'static str,
    ) -> Result<f32, Exception> {
        self.get(vm, index)
            .to_number()
            .ok_or_else(|| vm.create_exception(message))
    }

    #[inline(never)]
    pub fn get_vec4(
        &self,
        vm: &Vm,
        index: usize,
        message: &'static str,
    ) -> Result<Vec4, Exception> {
        self.get(vm, index)
            .to_vec4()
            .ok_or_else(|| vm.create_exception(message))
    }

    #[inline(never)]
    pub fn get_rgba(
        &self,
        vm: &Vm,
        index: usize,
        message: &'static str,
    ) -> Result<Rgba, Exception> {
        self.get(vm, index)
            .to_rgba()
            .ok_or_else(|| vm.create_exception(message))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub message: String,
}

impl From<bytecode::ReadError> for Exception {
    fn from(_: bytecode::ReadError) -> Self {
        Self {
            message: "corrupted bytecode".into(),
        }
    }
}

impl Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NOTE: This is not a user-friendly representation!
        write!(f, "{self:#?}")
    }
}

impl Error for Exception {}
