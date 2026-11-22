//! The Intel 8080's instructions, one for each of the 256 opcodes: the
//! mnemonic, the register operands, the data after the opcode, the cycle
//! count. juku-asm assembles from this table and juku-dis disassembles
//! with it.
//!
//! The twelve undocumented opcodes do what a documented one does (08h-38h
//! as NOP, CBh as JMP, D9h as RET, DDh, EDh and FDh as CALL) and are marked
//! so: an assembler never writes them.

/// What follows the opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Data {
    None,
    Byte,
    Word,
}

/// One opcode's instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Op {
    /// Upper case, as Intel writes it: `MOV`, `JNZ`.
    pub mnemonic: &'static str,
    /// The register operands, upper case, `""` past the last: `["A", "M"]`
    /// for MOV A,M, `["SP", ""]` for LXI SP, `["3", ""]` for RST 3.
    pub regs: [&'static str; 2],
    /// Whether the register operands name pairs: B, D, H, SP or PSW.
    pub pairs: bool,
    pub data: Data,
    /// The cycle count; the taken one for a conditional call or return.
    pub cycles: u8,
    pub documented: bool,
}

impl Op {
    /// Its length in bytes.
    pub const fn size(&self) -> usize {
        match self.data {
            Data::None => 1,
            Data::Byte => 2,
            Data::Word => 3,
        }
    }

    /// How many register operands it has.
    pub fn reg_count(&self) -> usize {
        self.regs.iter().filter(|r| !r.is_empty()).count()
    }

    /// How many operands it is written with: its registers, then its data.
    pub fn operand_count(&self) -> usize {
        self.reg_count() + (self.data != Data::None) as usize
    }
}

/// Every opcode's instruction, by opcode.
pub static OPS: [Op; 256] = {
    let mut ops = [decode(0); 256];
    let mut i = 0;
    while i < 256 {
        ops[i] = decode(i as u8);
        i += 1;
    }
    ops
};

const R: [&str; 8] = ["B", "C", "D", "E", "H", "L", "M", "A"];
const RP: [&str; 4] = ["B", "D", "H", "SP"];
const RP_PSW: [&str; 4] = ["B", "D", "H", "PSW"];
const ALU: [&str; 8] = ["ADD", "ADC", "SUB", "SBB", "ANA", "XRA", "ORA", "CMP"];
const ALU_IMM: [&str; 8] = ["ADI", "ACI", "SUI", "SBI", "ANI", "XRI", "ORI", "CPI"];
const ROT: [&str; 8] = ["RLC", "RRC", "RAL", "RAR", "DAA", "CMA", "STC", "CMC"];
const RET_CC: [&str; 8] = ["RNZ", "RZ", "RNC", "RC", "RPO", "RPE", "RP", "RM"];
const JMP_CC: [&str; 8] = ["JNZ", "JZ", "JNC", "JC", "JPO", "JPE", "JP", "JM"];
const CALL_CC: [&str; 8] = ["CNZ", "CZ", "CNC", "CC", "CPO", "CPE", "CP", "CM"];
const RST: [&str; 8] = ["0", "1", "2", "3", "4", "5", "6", "7"];

const fn op(mnemonic: &'static str, regs: [&'static str; 2], data: Data, cycles: u8) -> Op {
    Op { mnemonic, regs, pairs: false, data, cycles, documented: true }
}

/// An instruction with no operands.
const fn plain(mnemonic: &'static str, cycles: u8) -> Op {
    op(mnemonic, ["", ""], Data::None, cycles)
}

/// One whose register operand is a pair.
const fn pair(mnemonic: &'static str, reg: &'static str, data: Data, cycles: u8) -> Op {
    Op { pairs: true, ..op(mnemonic, [reg, ""], data, cycles) }
}

/// `fast` cycles, or `with_m` when the instruction reads or writes M.
const fn m(uses_m: bool, fast: u8, with_m: u8) -> u8 {
    if uses_m { with_m } else { fast }
}

const fn undocumented(op: Op) -> Op {
    Op { documented: false, ..op }
}

/// Opcode `o` as x, y, z: bits 7-6, 5-3, 2-0.
const fn decode(o: u8) -> Op {
    let (x, y, z) = (o >> 6, ((o >> 3) & 7) as usize, (o & 7) as usize);
    let p = y >> 1;
    if o == 0x76 {
        return plain("HLT", 7);
    }
    match x {
        1 => op("MOV", [R[y], R[z]], Data::None, m(y == 6 || z == 6, 5, 7)),
        2 => op(ALU[y], [R[z], ""], Data::None, m(z == 6, 4, 7)),
        0 => match z {
            0 if y == 0 => plain("NOP", 4),
            0 => undocumented(plain("NOP", 4)),
            1 if y % 2 == 0 => pair("LXI", RP[p], Data::Word, 10),
            1 => pair("DAD", RP[p], Data::None, 10),
            2 => match y {
                0 => pair("STAX", "B", Data::None, 7),
                1 => pair("LDAX", "B", Data::None, 7),
                2 => pair("STAX", "D", Data::None, 7),
                3 => pair("LDAX", "D", Data::None, 7),
                4 => op("SHLD", ["", ""], Data::Word, 16),
                5 => op("LHLD", ["", ""], Data::Word, 16),
                6 => op("STA", ["", ""], Data::Word, 13),
                _ => op("LDA", ["", ""], Data::Word, 13),
            },
            3 if y % 2 == 0 => pair("INX", RP[p], Data::None, 5),
            3 => pair("DCX", RP[p], Data::None, 5),
            4 => op("INR", [R[y], ""], Data::None, m(y == 6, 5, 10)),
            5 => op("DCR", [R[y], ""], Data::None, m(y == 6, 5, 10)),
            6 => op("MVI", [R[y], ""], Data::Byte, m(y == 6, 7, 10)),
            _ => plain(ROT[y], 4),
        },
        _ => match z {
            0 => plain(RET_CC[y], 11),
            1 if y % 2 == 0 => pair("POP", RP_PSW[p], Data::None, 10),
            1 => match p {
                0 => plain("RET", 10),
                1 => undocumented(plain("RET", 10)),
                2 => plain("PCHL", 5),
                _ => plain("SPHL", 5),
            },
            2 => op(JMP_CC[y], ["", ""], Data::Word, 10),
            3 => match y {
                0 => op("JMP", ["", ""], Data::Word, 10),
                1 => undocumented(op("JMP", ["", ""], Data::Word, 10)),
                2 => op("OUT", ["", ""], Data::Byte, 10),
                3 => op("IN", ["", ""], Data::Byte, 10),
                4 => plain("XTHL", 18),
                5 => plain("XCHG", 4),
                6 => plain("DI", 4),
                _ => plain("EI", 4),
            },
            4 => op(CALL_CC[y], ["", ""], Data::Word, 17),
            5 if y % 2 == 0 => pair("PUSH", RP_PSW[p], Data::None, 11),
            5 if y == 1 => op("CALL", ["", ""], Data::Word, 17),
            5 => undocumented(op("CALL", ["", ""], Data::Word, 17)),
            6 => op(ALU_IMM[y], ["", ""], Data::Byte, 7),
            _ => op("RST", [RST[y], ""], Data::None, 11),
        },
    }
}

/// Whether the instruction goes to its word operand: a jump or a call.
pub fn goes_to(op: &Op) -> bool {
    op.data == Data::Word && (op.mnemonic.starts_with('J') || op.mnemonic.starts_with('C')) && op.mnemonic != "CPI"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(OPS.iter().filter(|o| !o.documented).count(), 12);
        assert_eq!(OPS[0x7E], Op { mnemonic: "MOV", regs: ["A", "M"], pairs: false, data: Data::None, cycles: 7, documented: true });
        assert_eq!((OPS[0x31].mnemonic, OPS[0x31].regs, OPS[0x31].size()), ("LXI", ["SP", ""], 3));
        assert_eq!((OPS[0xF5].mnemonic, OPS[0xF5].regs, OPS[0xF5].cycles), ("PUSH", ["PSW", ""], 11));
        assert_eq!((OPS[0xDF].mnemonic, OPS[0xDF].regs), ("RST", ["3", ""]));
        assert_eq!((OPS[0xCB].mnemonic, OPS[0xCB].documented), ("JMP", false));
        assert!(goes_to(&OPS[0xCD]) && goes_to(&OPS[0xC2]) && !goes_to(&OPS[0x2A]) && !goes_to(&OPS[0xFE]));
        // Every documented mnemonic and set of registers once.
        let documented: Vec<_> = OPS.iter().filter(|o| o.documented).map(|o| (o.mnemonic, o.regs)).collect();
        let mut unique = documented.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), documented.len());
        assert_eq!(documented.len(), 244);
    }
}
