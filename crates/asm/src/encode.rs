//! The instructions: their bytes and cycle counts, from juku-i8080's table.

use std::collections::HashMap;
use std::sync::OnceLock;

use juku_i8080::{Data, OPS};

use crate::asm::{Assembler, Error, Result};

/// The documented opcodes of each mnemonic.
fn opcodes(mn: &str) -> &'static [u8] {
    static BY_MNEMONIC: OnceLock<HashMap<&'static str, Vec<u8>>> = OnceLock::new();
    let table = BY_MNEMONIC.get_or_init(|| {
        let mut table: HashMap<&str, Vec<u8>> = HashMap::new();
        for (code, op) in OPS.iter().enumerate().filter(|(_, op)| op.documented) {
            table.entry(op.mnemonic).or_default().push(code as u8);
        }
        table
    });
    table.get(mn).map_or(&[], Vec::as_slice)
}

/// The bytes of instruction `mn` with operands `ops`, and its cycle count:
/// the taken count for a conditional call or return.
pub(crate) fn encode(asm: &mut Assembler, mn: &str, ops: &[&str]) -> Result<(Vec<i64>, u32)> {
    let codes = opcodes(mn);
    let Some(&first) = codes.first() else {
        return Err(Error::new(format!("unknown mnemonic {mn}")));
    };
    let shape = &OPS[first as usize];
    let n = shape.operand_count();
    if ops.len() != n {
        return Err(Error::new(format!("{mn} takes {n} operand(s)")));
    }
    if mn == "RST" {
        let n = asm.ev(ops[0])?;
        if !(0..=7).contains(&n) {
            return Err(Error::new("RST takes 0-7"));
        }
        return Ok((vec![0xC7 | n << 3], 11));
    }

    // The registers: the opcode whose are the ones written.
    let count = shape.reg_count();
    let regs: Vec<String> = ops[..count].iter().map(|r| r.to_uppercase()).collect();
    let found = codes.iter().copied().find(|&c| OPS[c as usize].regs[..count].iter().zip(&regs).all(|(a, b)| a == b));
    let Some(code) = found else {
        for (i, reg) in regs.iter().enumerate() {
            if !codes.iter().any(|&c| OPS[c as usize].regs[i] == reg) {
                let kind = if shape.pairs { "register pair" } else { "register" };
                return Err(Error::new(format!("not a {kind}: {}", ops[i])));
            }
        }
        // Each register can be, but not together: MOV M,M.
        return Err(Error::new(format!("{mn} {} is HLT", regs.join(","))));
    };
    let op = &OPS[code as usize];
    let mut bytes = vec![code as i64];
    match op.data {
        Data::None => {}
        Data::Byte => {
            let v = asm.ev(ops[count])?;
            bytes.push(asm.byte(v)?);
        }
        Data::Word => {
            let v = asm.ev(ops[count])? & 0xFFFF;
            bytes.extend([v & 0xFF, v >> 8]);
        }
    }
    Ok((bytes, op.cycles as u32))
}
