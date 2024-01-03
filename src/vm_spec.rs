use crate::io;
use crate::ops::*;
use crate::ops_parse;
use crate::vm;

const R0: Register = Register(0);
const R7: Register = Register(7);
const R_PC: Register = Register(8);
const R_COND: Register = Register(9);
const R_PC_INIT: u16 = 0x3000;

const COND_P: u16 = 1 << 0 as u16;
const COND_Z: u16 = 1 << 1 as u16;
const COND_N: u16 = 1 << 2 as u16;

pub enum TickError {
    Io(io::IoError),
    Parse(ops_parse::ParseError),
}

pub fn run(vm: &mut impl VmSpec) -> Result<(), TickError> {
    loop {
        match vm.tick() {
            Ok(true) => continue,
            Ok(false) => return Ok(()),
            Err(e) => return Err(e),
        }
    }
}

pub trait VmSpec {
    fn init(&mut self);
    fn tick(&mut self) -> Result<bool, TickError>; 
    fn tick_op(&mut self, op: Operation) -> Result<bool, io::IoError>;
    fn trap(&mut self, trap_vector: u16) -> Result<bool, io::IoError>;
}

fn set_cond_reg(vm_mem: &mut impl vm::VmMem, register: Register) {
    let value = vm_mem.read_reg(register);
    if value == 0 {
        vm_mem.write_reg(R_COND, COND_Z);
    } else if value < 1 << 15 {
        vm_mem.write_reg(R_COND, COND_P);
    } else {
        vm_mem.write_reg(R_COND, COND_N);
    }
}

impl<T: vm::VmMem> VmSpec for T {
    fn init(&mut self) {
        self.write_reg(R_PC, R_PC_INIT);
        self.write_reg(R_COND, COND_Z);
    }
    fn trap(&mut self, trap_vector: u16) -> Result<bool, io::IoError> {
        match trap_vector {
            0x20 /* getc */ => self.write_reg(R0, io::getc()? as u16),
            0x21 /* out */ => io::putc(self.read_reg(R0) as u8)?,
            0x22 /* puts */ => io::puts(&self.c_str(self.read_reg(R0)))?,
            0x25 /* halt */ => return Ok(false),
            _ => panic!("not implemented trap vector: {:#x}", trap_vector)
        }
        return Ok(true);
    }
    fn tick(&mut self) -> Result<bool, TickError> {
        let pc = self.read_reg(R_PC);
        let op = Operation::parse(self.read_mem(pc)).map_err(|e| TickError::Parse(e))?;
        self.write_reg(R_PC, pc.wrapping_add(1));
        return self.tick_op(op).map_err(|e| TickError::Io(e));
    }
    fn tick_op(&mut self, op: Operation) -> Result<bool, io::IoError> {
        match op {
            Operation::OpAdd { dr, sr1, arg: Argument::Register(sr2) } => {
                self.write_reg(dr, self.read_reg(sr1).wrapping_add(self.read_reg(sr2)));
                set_cond_reg(self, dr);
            }
            Operation::OpAdd { dr, sr1, arg: Argument::Immediate(imm) } => {
                self.write_reg(dr, self.read_reg(sr1).wrapping_add(imm));
                set_cond_reg(self, dr);
            }
            Operation::OpAnd { dr, sr1, arg: Argument::Register(sr2) } => {
                self.write_reg(dr, self.read_reg(sr1) & self.read_reg(sr2));
                set_cond_reg(self, dr);
            }
            Operation::OpAnd { dr, sr1, arg: Argument::Immediate(imm) } => {
                self.write_reg(dr, self.read_reg(sr1) & imm);
                set_cond_reg(self, dr);
            }
            Operation::OpBr { n, z, p, pc_offset } => {
                let cond = self.read_reg(R_COND);
                if n && (COND_N & cond) != 0 || z && (COND_Z & cond) != 0 || p && (COND_P & cond) != 0 {
                    self.write_reg(R_PC, self.read_reg(R_PC).wrapping_add(pc_offset));
                }
            }
            Operation::OpJmp { base_r } => {
                self.write_reg(R_PC, self.read_reg(base_r));
            }
            Operation::OpJsr { pc_offset } => {
                self.write_reg(R7, self.read_reg(R_PC));
                self.write_reg(R_PC, self.read_reg(R_PC).wrapping_add(pc_offset));
            }
            Operation::OpJsrr { base_r } => {
                self.write_reg(R7, self.read_reg(R_PC));
                self.write_reg(R_PC, self.read_reg(base_r));
            }
            Operation::OpLd { dr, pc_offset } => {
                self.write_reg(dr, self.read_mem(self.read_reg(R_PC).wrapping_add(pc_offset)));
                set_cond_reg(self, dr);
            }
            Operation::OpLdi { dr, pc_offset } => {
                let address = self.read_mem(self.read_reg(R_PC).wrapping_add(pc_offset));
                self.write_reg(dr, self.read_mem(address));
                set_cond_reg(self, dr);
            }
            Operation::OpLdr { dr, base_r, offset } => {
                self.write_reg(dr, self.read_mem(self.read_reg(base_r).wrapping_add(offset)));
                set_cond_reg(self, dr);
            }
            Operation::OpLea { dr, pc_offset } => {
                self.write_reg(dr, self.read_reg(R_PC).wrapping_add(pc_offset));
                set_cond_reg(self, dr);
            }
            Operation::OpNot { dr, sr } => {
                self.write_reg(dr, !self.read_reg(sr));
                set_cond_reg(self, dr);
            }
            Operation::OpRti => panic!("rti operation is not implemented"),
            Operation::OpSt { sr, pc_offset } => {
                self.write_mem(self.read_reg(R_PC).wrapping_add(pc_offset), self.read_reg(sr));
            }
            Operation::OpSti { sr, pc_offset } => {
                let address = self.read_mem(self.read_reg(R_PC).wrapping_add(pc_offset));
                self.write_mem(address, self.read_reg(sr));
            }
            Operation::OpStr { sr, base_r, offset } => {
                self.write_mem(self.read_reg(base_r).wrapping_add(offset), self.read_reg(sr));
            }
            Operation::OpTrap { trap_vector } => {
                self.write_reg(R7, self.read_reg(R_PC));
                return self.trap(trap_vector);
            }
        }
        return Ok(true);
    }
}