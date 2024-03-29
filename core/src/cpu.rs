use std::mem;
use crate::{concat_u16, Result, Memory};

pub const CARRY_FLAG: u8 = 1 << 0;
pub const PARITY_FLAG: u8 = 1 << 2;
pub const ZERO_FLAG: u8 = 1 << 6;
pub const SIGN_FLAG: u8 = 1 << 7;

macro_rules! mov {
    ($from:expr,$to:expr,$cycles:expr) => {
        {
            $to = $from;
            $cycles
        }
    };
    ($from:expr,$to:expr) => { mov!($from, $to, 1) };
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterruptStatus {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone)]
pub enum Event {
    Halt,
    PortWrite(u8, u8),
    PortRead(u8),
}

#[derive(Debug, Clone)]
pub struct CPU {
    pub memory: Memory,
    interrupt_status: InterruptStatus,
    event: Option<Event>,
    flags: u8,
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
}

impl CPU {
    pub fn new(program: &[u8]) -> Self {
        let mut rom = [0; 0x2000];
        for (i, val) in program.iter().enumerate() {
            rom[i] = *val;
        }

        Self {
            memory: Memory::new(rom),
            interrupt_status: InterruptStatus::Enabled,
            event: None,
            flags: 0,
            pc: 0,
            sp: 0,
            a: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
        }
    }

    pub fn reset(&mut self) {
        self.memory.reset_ram();
        self.interrupt_status = InterruptStatus::Enabled;
        self.event = None;
        self.flags = 0;
        self.pc = 0;
        self.sp = 0;
        self.a = 0;
        self.b = 0;
        self.c = 0;
        self.d = 0;
        self.e = 0;
        self.h = 0;
        self.l = 0;
    }

    pub fn interrupt(&mut self, interrupt_num: u8) {
        if let InterruptStatus::Enabled = self.interrupt_status {
            self.rst(interrupt_num);
        }
    }

    pub fn step(&mut self) -> Result<u32> {
        let opcode = self.read_pc();

        macro_rules! mvi {
            ($to:expr,$cycles:expr) => {
                {
                    $to = self.read_pc();
                    $cycles
                }
            };
            ($to:expr) => { mvi!($to, 2) };
        }

        macro_rules! ret {
            () => {
                {
                    self.pc = self.stack_pop_u16();
                    3
                }
            };
            (!$flag:expr) => {
                if self.flag($flag) == 0 { ret!() } else { 1 }
            };
            ($flag:expr) => {
                if self.flag($flag) != 0 { ret!() } else { 1 }
            };
        }

        macro_rules! push {
            ($hi:expr,$lo:expr) => {
                {
                    self.stack_push($hi);
                    self.stack_push($lo);
                    3
                }
            };
        }

        macro_rules! pop {
            ($hi:expr,$lo:expr) => {
                {
                    $lo = self.stack_pop();
                    $hi = self.stack_pop();
                    3
                }
            };
        }

        Ok(match opcode {
            // Misc/control instructions
            0x00 | 0x10 | 0x20 | 0x30 | 0x08 | 0x18 | 0x28 | 0x38 => 1, // NOP
            0x76 => {                                                   // HLT
                self.event = Some(Event::Halt);
                1
            }
            0xD3 => {                                                   // OUT   d8
                let port = self.read_pc();
                self.event = Some(Event::PortWrite(port, self.a));
                3
            }
            0xDB => {                                                   // IN    d8
                let port = self.read_pc();
                self.event = Some(Event::PortRead(port));
                3
            }
            0xF3 => {                                                   // DI
                self.interrupt_status = InterruptStatus::Disabled;
                1
            }
            0xFB => {                                                   // EI
                self.interrupt_status = InterruptStatus::Enabled;
                1
            }

            // Jumps/calls
            0xC0 => ret!(!ZERO_FLAG),                         // RNZ
            0xD0 => ret!(!CARRY_FLAG),                        // RNC
            0xE0 => ret!(!PARITY_FLAG),                       // RPO
            0xF0 => ret!(!SIGN_FLAG),                         // RP
            0xC2 => self.jmp_if_not(ZERO_FLAG),                         // JNZ   a16
            0xD2 => self.jmp_if_not(CARRY_FLAG),                        // JNC   a16
            0xE2 => self.jmp_if_not(PARITY_FLAG),                       // JPO   a16
            0xF2 => self.jmp_if_not(SIGN_FLAG),                         // JP    a16
            0xC3 | 0xCB => {                                            // JMP   a16
                self.pc = self.read_pc_u16();
                3
            }
            0xC4 => self.call_if_not(ZERO_FLAG),                        // CNZ   a16
            0xD4 => self.call_if_not(CARRY_FLAG),                       // CNC   a16
            0xE4 => self.call_if_not(PARITY_FLAG),                      // CPO   a16
            0xF4 => self.call_if_not(SIGN_FLAG),                        // CP    a16
            0xC7 => self.rst(0),                                        // RST   0
            0xCF => self.rst(1),                                        // RST   1
            0xD7 => self.rst(2),                                        // RST   2
            0xDF => self.rst(3),                                        // RST   3
            0xE7 => self.rst(4),                                        // RST   4
            0xEF => self.rst(5),                                        // RST   5
            0xF7 => self.rst(6),                                        // RST   6
            0xFF => self.rst(7),                                        // RST   7
            0xC8 => ret!(ZERO_FLAG),                             // RZ
            0xD8 => ret!(CARRY_FLAG),                            // RC
            0xE8 => ret!(PARITY_FLAG),                           // RPE
            0xF8 => ret!(SIGN_FLAG),                             // RM
            0xC9 | 0xD9 => ret!(),                                  // RET
            0xE9 => {                                                   // PCHL
                self.pc = concat_u16!(self.h, self.l);
                1
            }
            0xCA => self.jmp_if(ZERO_FLAG),                             // JZ    a16
            0xDA => self.jmp_if(CARRY_FLAG),                            // JC    a16
            0xEA => self.jmp_if(PARITY_FLAG),                           // JPE   a16
            0xFA => self.jmp_if(SIGN_FLAG),                             // JM    a16
            0xCC => self.call_if(ZERO_FLAG),                            // CZ    a16
            0xDC => self.call_if(CARRY_FLAG),                           // CC    a16
            0xEC => self.call_if(PARITY_FLAG),                          // CPE   a16
            0xFC => self.call_if(SIGN_FLAG),                            // CM    a16
            0xCD | 0xDD | 0xED | 0xFD => {                              // CALL  a16
                let adr = self.read_pc_u16();
                self.call(adr)
            }

            // 8-bit load/store/move instructions
            0x12 => {                                                   // STAX  D
                *self.de_val_mut() = self.a;
                2
            }
            0x02 => {                                                   // STAX  B
                *self.bc_val_mut() = self.a;
                2
            }
            0x32 => {                                                   // STA   a16
                let adr = self.read_pc_u16();
                self.memory[adr] = self.a;
                4
            }
            0x06 => mvi!(self.b),                                                   // MVI   B,d8
            0x0E => mvi!(self.c),                                                   // MVI   C,d8
            0x16 => mvi!(self.d),                                                   // MVI   D,d8
            0x1E => mvi!(self.e),                                                   // MVI   E,d8
            0x26 => mvi!(self.h),                                                   // MVI   H,d8
            0x2E => mvi!(self.l),                                                   // MVI   L,d8
            0x36 => mvi!(*self.m_val_mut(), 3),                                     // MVI   M,d8
            0x3E => mvi!(self.a),                                                   // MVI   A,d8
            0x0A => {                                                   // LDAX  B
                self.a = self.bc_val();
                2
            }
            0x1A => {                                                   // LDAX  D
                self.a = self.de_val();
                2
            }
            0x3A => {                                                   // LDA   a16
                let adr = self.read_pc_u16();
                self.a = self.memory[adr];
                4
            }
            0x40 => mov!(self.b, self.b),                     // MOV   B,B
            0x41 => mov!(self.c, self.b),                     // MOV   B,C
            0x42 => mov!(self.d, self.b),                     // MOV   B,D
            0x43 => mov!(self.e, self.b),                     // MOV   B,E
            0x44 => mov!(self.h, self.b),                     // MOV   B,H
            0x45 => mov!(self.l, self.b),                     // MOV   B,L
            0x46 => mov!(self.m_val(), self.b, 2),             // MOV   B,M
            0x47 => mov!(self.a, self.b),                     // MOV   B,A
            0x48 => mov!(self.b, self.c),                     // MOV   C,B
            0x49 => mov!(self.c, self.c),                     // MOV   C,C
            0x4A => mov!(self.d, self.c),                     // MOV   C,D
            0x4B => mov!(self.e, self.c),                     // MOV   C,E
            0x4C => mov!(self.h, self.c),                     // MOV   C,H
            0x4D => mov!(self.l, self.c),                     // MOV   C,L
            0x4E => mov!(self.m_val(), self.c, 2),             // MOV   C,M
            0x4F => mov!(self.a, self.c),                     // MOV   C,A
            0x50 => mov!(self.b, self.d),                     // MOV   D,B
            0x51 => mov!(self.c, self.d),                     // MOV   D,C
            0x52 => mov!(self.d, self.d),                     // MOV   D,D
            0x53 => mov!(self.e, self.d),                     // MOV   D,E
            0x54 => mov!(self.h, self.d),                     // MOV   D,H
            0x55 => mov!(self.l, self.d),                     // MOV   D,L
            0x56 => mov!(self.m_val(), self.d, 2),             // MOV   D,M
            0x57 => mov!(self.a, self.d),                     // MOV   D,A
            0x58 => mov!(self.b, self.e),                     // MOV   E,B
            0x59 => mov!(self.c, self.e),                     // MOV   E,C
            0x5A => mov!(self.d, self.e),                     // MOV   E,D
            0x5B => mov!(self.e, self.e),                     // MOV   E,E
            0x5C => mov!(self.h, self.e),                     // MOV   E,H
            0x5D => mov!(self.l, self.e),                     // MOV   E,L
            0x5E => mov!(self.m_val(), self.e, 2),             // MOV   E,M
            0x5F => mov!(self.a, self.e),                     // MOV   E,A
            0x60 => mov!(self.b, self.h),                     // MOV   H,B
            0x61 => mov!(self.c, self.h),                     // MOV   H,C
            0x62 => mov!(self.d, self.h),                     // MOV   H,D
            0x63 => mov!(self.e, self.h),                     // MOV   H,E
            0x64 => mov!(self.h, self.h),                     // MOV   H,H
            0x65 => mov!(self.l, self.h),                     // MOV   H,L
            0x66 => mov!(self.m_val(), self.h, 2),             // MOV   H,M
            0x67 => mov!(self.a, self.h),                     // MOV   H,A
            0x68 => mov!(self.b, self.l),                     // MOV   L,B
            0x69 => mov!(self.c, self.l),                     // MOV   L,C
            0x6A => mov!(self.d, self.l),                     // MOV   L,D
            0x6B => mov!(self.e, self.l),                     // MOV   L,E
            0x6C => mov!(self.h, self.l),                     // MOV   L,H
            0x6D => mov!(self.l, self.l),                     // MOV   L,L
            0x6E => mov!(self.m_val(), self.l, 2),             // MOV   L,M
            0x6F => mov!(self.a, self.l),                     // MOV   L,A
            0x70 => mov!(self.b, *self.m_val_mut(), 2),         // MOV   M,B
            0x71 => mov!(self.c, *self.m_val_mut(), 2),         // MOV   M,C
            0x72 => mov!(self.d, *self.m_val_mut(), 2),         // MOV   M,D
            0x73 => mov!(self.e, *self.m_val_mut(), 2),         // MOV   M,E
            0x74 => mov!(self.h, *self.m_val_mut(), 2),         // MOV   M,H
            0x75 => mov!(self.l, *self.m_val_mut(), 2),         // MOV   M,L
            0x77 => mov!(self.a, *self.m_val_mut(), 2),         // MOV   M,A
            0x78 => mov!(self.b, self.a),                     // MOV   A,B
            0x79 => mov!(self.c, self.a),                     // MOV   A,C
            0x7A => mov!(self.d, self.a),                     // MOV   A,D
            0x7B => mov!(self.e, self.a),                     // MOV   A,E
            0x7C => mov!(self.h, self.a),                     // MOV   A,H
            0x7D => mov!(self.l, self.a),                     // MOV   A,L
            0x7E => mov!(self.m_val(), self.a, 2),             // MOV   A,M
            0x7F => mov!(self.a, self.a),                   // MOV   A,A

            // 16-bit load/store/move instructions
            0x01 => {                                                   // LXI   B,d16
                self.c = self.read_pc();
                self.b = self.read_pc();
                3
            }
            0x11 => {                                                   // LXI   D,d16
                self.e = self.read_pc();
                self.d = self.read_pc();
                3
            }
            0x21 => {                                                   // LXI   H,d16
                self.l = self.read_pc();
                self.h = self.read_pc();
                3
            }
            0x31 => {                                                   // LXI   SP,d16
                self.sp = self.read_pc_u16();
                3
            }
            0x22 => {                                                   // SHLD
                let adr = self.read_pc_u16();
                self.memory[adr] = self.l;
                self.memory[adr + 1] = self.h;
                5
            }
            0x2A => {                                                   // LHLD
                let adr = self.read_pc_u16();
                self.l = self.memory[adr];
                self.h = self.memory[adr + 1];
                5
            }
            0xC1 => pop!(self.b, self.c),                                                   // POP  B
            0xD1 => pop!(self.d, self.e),                                                   // POP  D
            0xE1 => pop!(self.h, self.l),                                                   // POP  H
            0xF1 => pop!(self.a, self.flags),                                               // POP  PSW
            0xC5 => push!(self.b, self.c),                                                   // PUSH  B
            0xD5 => push!(self.d, self.e),                                                   // PUSH  D
            0xE5 => push!(self.h, self.l),                                                   // PUSH  H
            0xF5 => push!(self.a, self.flags),                                               // PUSH  PSW
            0xE3 => {                                                   // XTHL
                mem::swap(&mut self.h, &mut self.memory[self.sp + 1]);
                mem::swap(&mut self.l, &mut self.memory[self.sp]);
                5
            }
            0xF9 => {                                                   // SPHL
                self.sp = self.m();
                1
            }
            0xEB => {                                                   // XCHG
                mem::swap(&mut self.h, &mut self.d);
                mem::swap(&mut self.l, &mut self.e);
                1
            }

            // 8-bit arithmetic/logical instructions
            0x04 => {                                                   // INR   B
                self.b = self.inr(self.b);
                1
            }
            0x0C => {                                                   // INR   C
                self.c = self.inr(self.c);
                1
            }
            0x14 => {                                                   // INR   D
                self.d = self.inr(self.d);
                1
            }
            0x1C => {                                                   // INR   E
                self.e = self.inr(self.e);
                1
            }
            0x24 => {                                                   // INR   H
                self.h = self.inr(self.h);
                1
            }
            0x2C => {                                                   // INR   L
                self.l = self.inr(self.l);
                1
            }
            0x34 => {                                                   // INR   M
                *self.m_val_mut() = self.inr(self.m_val());
                3
            }
            0x3C => {                                                   // INR   A
                self.a = self.inr(self.a);
                1
            }
            0x05 => {                                                   // DCR   B
                self.b = self.dcr(self.b);
                1
            }
            0x0D => {                                                   // DCR   C
                self.c = self.dcr(self.c);
                1
            }
            0x15 => {                                                   // DCR   D
                self.d = self.dcr(self.d);
                1
            }
            0x1D => {                                                   // DCR   E
                self.e = self.dcr(self.e);
                1
            }
            0x25 => {                                                   // DCR   H
                self.h = self.dcr(self.h);
                1
            }
            0x2D => {                                                   // DCR   L
                self.l = self.dcr(self.l);
                1
            }
            0x35 => {                                                   // DCR   M
                *self.m_val_mut() = self.dcr(self.m_val());
                3
            }
            0x3D => {                                                   // DCR   A
                self.a = self.dcr(self.a);
                1
            }
            0x07 => {                                                   // RLC
                self.set_flag(CARRY_FLAG, self.a & (1 << 7));
                self.a = self.a.rotate_left(1);
                1
            }
            0x0F => {                                                   // RRC
                self.set_flag(CARRY_FLAG, self.a & 1);
                self.a = self.a.rotate_right(1);
                1
            }
            0x17 => {                                                   // RAL
                let carry = self.a & (1 << 7);
                self.a = (self.a << 1) | self.flag(CARRY_FLAG);
                self.set_flag(CARRY_FLAG, carry);
                1
            }
            0x1F => {                                                   // RAR
                let carry = self.a & 1;
                self.a = (self.a >> 1) | (self.flag(CARRY_FLAG) << 7);
                self.set_flag(CARRY_FLAG, carry);
                1
            }
            0x27 => {                                                   // DAA
                if self.a & 0x0F > 9 {
                    self.a += 6;
                }

                if self.a & 0xF0 > 0x90 {
                    let (result, carry) = self.a.overflowing_add(0x60);
                    self.set_flags(self.a, carry as u8);
                    self.a = result;
                }

                1
            }
            0x37 => {                                                   // STC
                self.set_flag(CARRY_FLAG, 1);
                1
            }
            0x2F => {                                                   // CMA
                self.a = !self.a;
                1
            }
            0x3F => {                                                   // CMC
                self.flags ^= CARRY_FLAG;
                1
            }
            0x80 => self.add_a(self.b),                                 // ADD   B
            0x81 => self.add_a(self.c),                                 // ADD   C
            0x82 => self.add_a(self.d),                                 // ADD   D
            0x83 => self.add_a(self.e),                                 // ADD   E
            0x84 => self.add_a(self.h),                                 // ADD   H
            0x85 => self.add_a(self.l),                                 // ADD   L
            0x86 => {                                                         // ADD   M
                self.add_a(self.m_val());
                2
            }
            0x87 => self.add_a(self.a),                                 // ADD   A
            0x88 => self.add_a(self.b + self.flag(CARRY_FLAG)),         // ADC   B
            0x89 => self.add_a(self.c + self.flag(CARRY_FLAG)),         // ADC   C
            0x8A => self.add_a(self.d + self.flag(CARRY_FLAG)),         // ADC   D
            0x8B => self.add_a(self.e + self.flag(CARRY_FLAG)),         // ADC   E
            0x8C => self.add_a(self.h + self.flag(CARRY_FLAG)),         // ADC   H
            0x8D => self.add_a(self.l + self.flag(CARRY_FLAG)),         // ADC   L
            0x8E => {                                                   // ADC   M
                self.add_a(self.m_val() + self.flag(CARRY_FLAG));
                2
            }
            0x8F => self.add_a(self.a + self.flag(CARRY_FLAG)),         // ADC   A
            0x90 => self.sub_a(self.b),                                 // SUB   B
            0x91 => self.sub_a(self.c),                                 // SUB   C
            0x92 => self.sub_a(self.d),                                 // SUB   D
            0x93 => self.sub_a(self.e),                                 // SUB   E
            0x94 => self.sub_a(self.h),                                 // SUB   H
            0x95 => self.sub_a(self.l),                                 // SUB   L
            0x96 => {                                                   // SUB   M
                self.sub_a(self.m_val());
                2
            }
            0x97 => self.sub_a(self.a),                                 // SUB   A
            0x98 => self.sub_a(self.b + self.flag(CARRY_FLAG)),         // SBB   B
            0x99 => self.sub_a(self.c + self.flag(CARRY_FLAG)),         // SBB   C
            0x9A => self.sub_a(self.d + self.flag(CARRY_FLAG)),         // SBB   D
            0x9B => self.sub_a(self.e + self.flag(CARRY_FLAG)),         // SBB   E
            0x9C => self.sub_a(self.h + self.flag(CARRY_FLAG)),         // SBB   H
            0x9D => self.sub_a(self.l + self.flag(CARRY_FLAG)),         // SBB   L
            0x9E => {                                                   // SBB   M
                self.sub_a(self.m_val() + self.flag(CARRY_FLAG));
                2
            }
            0x9F => self.sub_a(self.a + self.flag(CARRY_FLAG)),         // SBB   A
            0xA0 => self.and_a(self.b),                                 // ANA   B
            0xA1 => self.and_a(self.c),                                 // ANA   C
            0xA2 => self.and_a(self.d),                                 // ANA   D
            0xA3 => self.and_a(self.e),                                 // ANA   E
            0xA4 => self.and_a(self.h),                                 // ANA   H
            0xA5 => self.and_a(self.l),                                 // ANA   L
            0xA6 => {                                                   // ANA   M
                self.and_a(self.m_val());
                2
            }
            0xA7 => self.and_a(self.a),                                 // ANA   A
            0xA8 => self.xor_a(self.b),                                 // XRA   B
            0xA9 => self.xor_a(self.c),                                 // XRA   C
            0xAA => self.xor_a(self.d),                                 // XRA   D
            0xAB => self.xor_a(self.e),                                 // XRA   E
            0xAC => self.xor_a(self.h),                                 // XRA   H
            0xAD => self.xor_a(self.l),                                 // XRA   L
            0xAE => {                                                   // XRA   M
                self.xor_a(self.m_val());
                2
            }
            0xAF => self.xor_a(self.a),                                 // XRA   A
            0xB0 => self.or_a(self.b),                                  // ORA   B
            0xB1 => self.or_a(self.c),                                  // ORA   C
            0xB2 => self.or_a(self.d),                                  // ORA   D
            0xB3 => self.or_a(self.e),                                  // ORA   E
            0xB4 => self.or_a(self.h),                                  // ORA   H
            0xB5 => self.or_a(self.l),                                  // ORA   L
            0xB6 => {                                                   // ORA   M
                self.or_a(self.m_val());
                2
            }
            0xB7 => self.or_a(self.a),                                  // ORA   A
            0xB8 => self.cmp_a(self.b),                                 // CMP   B
            0xB9 => self.cmp_a(self.c),                                 // CMP   C
            0xBA => self.cmp_a(self.d),                                 // CMP   D
            0xBB => self.cmp_a(self.e),                                 // CMP   E
            0xBC => self.cmp_a(self.h),                                 // CMP   H
            0xBD => self.cmp_a(self.l),                                 // CMP   L
            0xBE => {                                                   // CMP   M
                self.cmp_a(self.m_val());
                2
            }
            0xBF => self.cmp_a(self.a),                                 // CMP   A
            0xC6 => {                                                   // ADI   d8
                let d8 = self.read_pc();
                self.add_a(d8);
                2
            }
            0xD6 => {                                                   // SUI   d8
                let d8 = self.read_pc();
                self.sub_a(d8);
                2
            }
            0xE6 => {                                                   // ANI   d8
                let d8 = self.read_pc();
                self.and_a(d8);
                2
            }
            0xF6 => {                                                   // ORI   d8
                let d8 = self.read_pc();
                self.or_a(d8);
                2
            }
            0xCE => {                                                   // ACI   d8
                let d8 = self.read_pc();
                self.add_a(d8 + self.flag(CARRY_FLAG));
                2
            }
            0xDE => {                                                   // SBI   d8
                let d8 = self.read_pc();
                self.sub_a(d8 + self.flag(CARRY_FLAG));
                2
            }
            0xEE => {                                                   // XRI   d8
                let d8 = self.read_pc();
                self.xor_a(d8);
                2
            }
            0xFE => {                                                   // CPI   d8
                let d8 = self.read_pc();
                self.cmp_a(d8);
                2
            }

            // 16-bit arithmetic/logical instructions
            0x03 => Self::inx(&mut self.b, &mut self.c),                // INX   B
            0x13 => Self::inx(&mut self.d, &mut self.e),                // INX   D
            0x23 => Self::inx(&mut self.h, &mut self.l),                // INX   H
            0x33 => {                                                   // INX   SP
                self.sp = self.sp.wrapping_add(1);
                1
            }
            0x09 => self.dad(self.b, self.c),                           // DAD   B
            0x19 => self.dad(self.d, self.e),                           // DAD   D
            0x29 => self.dad(self.h, self.l),                           // DAD   H
            0x39 => self.dad((self.sp >> 8) as u8, self.sp as u8),      // DAD   SP
            0x0B => Self::dcx(&mut self.b, &mut self.c),                // DCX   B
            0x1B => Self::dcx(&mut self.d, &mut self.e),                // DCX   D
            0x2B => Self::dcx(&mut self.h, &mut self.l),                // DCX   H
            0x3B => {                                                   // DCX   SP
                self.sp = self.sp.wrapping_sub(1);
                1
            }
        })
    }

    pub fn event(&mut self) -> Option<Event> {
        mem::replace(&mut self.event, None)
    }

    pub fn port_in(&mut self, val: u8) {
        self.a = val;
    }

    fn jmp_if(&mut self, flag: u8) -> u32 {
        let adr = self.read_pc_u16();
        if self.flag(flag) != 0 { self.pc = adr; }
        3
    }

    fn jmp_if_not(&mut self, flag: u8) -> u32 {
        let adr = self.read_pc_u16();
        if self.flag(flag) == 0 { self.pc = adr; }
        3
    }

    fn rst(&mut self, val: u8) -> u32 {
        self.call((val as u16) << 3)
    }

    fn call(&mut self, adr: u16) -> u32 {
        self.stack_push_u16(self.pc);
        self.pc = adr;
        5
    }

    fn call_if(&mut self, flag: u8) -> u32 {
        let adr = self.read_pc_u16();
        if self.flag(flag) != 0 { self.call(adr) } else { 3 }
    }

    fn call_if_not(&mut self, flag: u8) -> u32 {
        let adr = self.read_pc_u16();
        if self.flag(flag) == 0 { self.call(adr) } else { 3 }
    }

    fn inr(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        self.set_flags(result, self.flag(CARRY_FLAG));
        result
    }

    fn dcr(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        self.set_flags(result, self.flag(CARRY_FLAG));
        result
    }

    fn add_a(&mut self, right: u8) -> u32 {
        let (result, overflow) = self.a.overflowing_add(right);
        self.set_flags(result, overflow as u8);
        self.a = result;
        1
    }

    fn sub_a(&mut self, val: u8) -> u32 {
        let (result, underflow) = self.a.overflowing_sub(val);
        self.set_flags(result, underflow as u8);
        self.a = result;
        1
    }

    fn and_a(&mut self, val: u8) -> u32 {
        self.a &= val;
        self.set_flags(self.a, 0);
        1
    }

    fn xor_a(&mut self, val: u8) -> u32 {
        self.a ^= val;
        self.set_flags(self.a, 0);
        1
    }

    fn or_a(&mut self, val: u8) -> u32 {
        self.a |= val;
        self.set_flags(self.a, 0);
        1
    }

    fn cmp_a(&mut self, val: u8) -> u32 {
        let (result, underflow) = self.a.overflowing_sub(val);
        self.set_flags(result, underflow as u8);
        1
    }

    fn inx(hi: &mut u8, lo: &mut u8) -> u32 {
        let (result_lo, carry) = lo.overflowing_add(1);
        *lo = result_lo;
        *hi = hi.wrapping_add(carry as u8);
        1
    }

    fn dad(&mut self, hi: u8, lo: u8) -> u32 {
        let val = concat_u16!(hi, lo);
        let hl = concat_u16!(self.h, self.l);

        let (result, carry) = hl.overflowing_add(val);
        self.h = (result >> 8) as u8;
        self.l = (result & 0xFF) as u8;
        self.set_flag(CARRY_FLAG, carry as u8);
        3
    }

    fn dcx(hi: &mut u8, lo: &mut u8) -> u32 {
        let (result_lo, carry) = lo.overflowing_sub(1);
        *lo = result_lo;
        *hi = hi.wrapping_sub(carry as u8);
        1
    }

    fn stack_push(&mut self, val: u8) {
        self.sp -= 1;
        self.memory[self.sp] = val;
    }

    fn stack_push_u16(&mut self, val: u16) {
        self.stack_push((val >> 8) as u8);
        self.stack_push((val & 0xFF) as u8);
    }

    fn stack_pop(&mut self) -> u8 {
        let val = self.memory[self.sp];
        self.sp += 1;
        val
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;
        (hi << 8) | lo
    }

    fn set_flags(&mut self, val: u8, carry: u8) {
        self.set_flag(CARRY_FLAG, carry);
        self.set_flag(PARITY_FLAG, crate::even_parity(val) as u8);
        self.set_flag(ZERO_FLAG, (val == 0) as u8);
        self.set_flag(SIGN_FLAG, val & (1 << 7));
    }

    fn read_pc(&mut self) -> u8 {
        let val = self.memory[self.pc];
        self.pc += 1;
        val
    }

    fn read_pc_u16(&mut self) -> u16 {
        let val = concat_u16!(self.memory[self.pc + 1], self.memory[self.pc]);
        self.pc += 2;
        val
    }

    fn flag(&self, flag: u8) -> u8 {
        (self.flags & flag != 0).into()
    }

    fn set_flag(&mut self, flag: u8, value: u8) {
        if value != 0 {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    fn bc(&self) -> u16 { concat_u16!(self.b, self.c) }

    fn bc_val(&self) -> u8 { self.memory[self.bc()] }

    fn bc_val_mut(&mut self) -> &mut u8 {
        let adr = self.bc();
        &mut self.memory[adr]
    }

    fn de(&self) -> u16 { concat_u16!(self.d, self.e) }

    fn de_val(&self) -> u8 { self.memory[self.de()] }

    fn de_val_mut(&mut self) -> &mut u8 {
        let adr = self.de();
        &mut self.memory[adr]
    }

    fn m(&self) -> u16 { concat_u16!(self.h, self.l) }

    fn m_val(&self) -> u8 { self.memory[self.m()] }

    fn m_val_mut(&mut self) -> &mut u8 {
        let adr = self.m();
        &mut self.memory[adr]
    }
}
