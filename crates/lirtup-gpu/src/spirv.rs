//! The bits of SPIR-V this layer reads or patches (`vk_spirv_*` in
//! `vk_utils.cpp`): the compute entry point, its workgroup size and the
//! `NonReadable` decoration for BGRA storage images.

use crate::{GpuError, Result};

const MAGIC: u32 = 0x0723_0203;
const HEADER_WORDS: usize = 5;

const OP_ENTRY_POINT: u16 = 15;
const OP_EXECUTION_MODE: u16 = 16;
const OP_CONSTANT: u16 = 43;
const OP_CONSTANT_COMPOSITE: u16 = 44;
const OP_SPEC_CONSTANT: u16 = 50;
const OP_SPEC_CONSTANT_COMPOSITE: u16 = 51;
const OP_DECORATE: u16 = 71;
const OP_EXECUTION_MODE_ID: u16 = 331;

const EXECUTION_MODEL_GL_COMPUTE: u32 = 5;
const MODE_LOCAL_SIZE: u32 = 17;
const MODE_LOCAL_SIZE_ID: u32 = 38;
const DECORATION_BUILT_IN: u32 = 11;
const DECORATION_NON_READABLE: u32 = 25;
const DECORATION_BINDING: u32 = 33;
const BUILT_IN_WORKGROUP_SIZE: u32 = 25;

/// SPIR-V bytes as words (copied: `include_bytes!` data is not aligned).
pub(crate) fn words(bytes: &[u8]) -> Result<Vec<u32>> {
    if bytes.len() < HEADER_WORDS * 4 || bytes.len() % 4 != 0 {
        return Err(GpuError::Invalid(format!("SPIR-V of {} bytes", bytes.len())));
    }
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    if words[0] != MAGIC {
        return Err(GpuError::Invalid("not SPIR-V (bad magic number)".into()));
    }
    Ok(words)
}

/// `(opcode, operands)` of every instruction after the header; stops at a
/// malformed one.
fn instructions(words: &[u32]) -> impl Iterator<Item = (u16, &[u32])> {
    let mut offset = HEADER_WORDS;
    std::iter::from_fn(move || {
        let word = *words.get(offset)?;
        let len = (word >> 16) as usize;
        if len == 0 || offset + len > words.len() {
            return None;
        }
        let item = ((word & 0xffff) as u16, &words[offset + 1..offset + len]);
        offset += len;
        Some(item)
    })
}

/// A SPIR-V literal string (UTF-8, NUL-terminated, packed little-endian).
fn literal_string(operands: &[u32]) -> Option<String> {
    let bytes: Vec<u8> = operands.iter().flat_map(|w| w.to_le_bytes()).collect();
    let end = bytes.iter().position(|&b| b == 0)?;
    String::from_utf8(bytes[..end].to_vec()).ok()
}

/// The GLCompute entry point named `name` (any, if empty): `(name, id)`.
pub(crate) fn compute_entry(words: &[u32], name: &str) -> Option<(String, u32)> {
    instructions(words).find_map(|(op, ops)| {
        if op != OP_ENTRY_POINT || ops.len() < 3 || ops[0] != EXECUTION_MODEL_GL_COMPUTE {
            return None;
        }
        let entry = literal_string(&ops[2..])?;
        (name.is_empty() || entry == name).then_some((entry, ops[1]))
    })
}

/// Value of an `OpConstant`/`OpSpecConstant` with result `id`.
fn constant(words: &[u32], id: u32) -> Option<u32> {
    instructions(words).find_map(|(op, ops)| {
        ((op == OP_CONSTANT || op == OP_SPEC_CONSTANT) && ops.len() >= 3 && ops[1] == id).then_some(ops[2])
    })
}

/// Workgroup size of the entry point `entry_id`: the `WorkgroupSize`
/// built-in if declared (it wins, per the spec), else `LocalSize` or
/// `LocalSizeId`.
pub(crate) fn workgroup_size(words: &[u32], entry_id: u32) -> Option<[u32; 3]> {
    let builtin = instructions(words).find_map(|(op, ops)| {
        (op == OP_DECORATE
            && ops.len() >= 3
            && ops[1] == DECORATION_BUILT_IN
            && ops[2] == BUILT_IN_WORKGROUP_SIZE)
            .then_some(ops[0])
    });
    if let Some(id) = builtin {
        let composite = instructions(words).find_map(|(op, ops)| {
            ((op == OP_CONSTANT_COMPOSITE || op == OP_SPEC_CONSTANT_COMPOSITE)
                && ops.len() >= 5
                && ops[1] == id)
                .then(|| [ops[2], ops[3], ops[4]])
        });
        if let Some(ids) = composite {
            let resolved = [
                constant(words, ids[0]),
                constant(words, ids[1]),
                constant(words, ids[2]),
            ];
            if let [Some(x), Some(y), Some(z)] = resolved {
                return Some([x, y, z]);
            }
        }
    }
    instructions(words).find_map(|(op, ops)| {
        if ops.len() < 5 || ops[0] != entry_id {
            return None;
        }
        match (op, ops[1]) {
            (OP_EXECUTION_MODE, MODE_LOCAL_SIZE) => Some([ops[2], ops[3], ops[4]]),
            (OP_EXECUTION_MODE_ID, MODE_LOCAL_SIZE_ID) => Some([
                constant(words, ops[2])?,
                constant(words, ops[3])?,
                constant(words, ops[4])?,
            ]),
            _ => None,
        }
    })
}

/// Add `OpDecorate %var NonReadable` to the variable bound at `binding`
/// (`vk_spirv_patch_nonreadable_uav`): without
/// `shaderStorageImageReadWithoutFormat`, a write-only BGRA storage image is
/// only valid when the shader says so. `None` if there is nothing to patch.
pub(crate) fn patch_non_readable(words: &[u32], binding: u32) -> Option<Vec<u32>> {
    let mut offset = HEADER_WORDS;
    let mut found = None;
    while offset < words.len() {
        let word = words[offset];
        let len = (word >> 16) as usize;
        if len == 0 || offset + len > words.len() {
            return None;
        }
        let op = (word & 0xffff) as u16;
        if op == OP_DECORATE
            && len >= 4
            && words[offset + 2] == DECORATION_BINDING
            && words[offset + 3] == binding
        {
            found = Some((words[offset + 1], offset + len));
            break;
        }
        offset += len;
    }
    let (target, inject_at) = found?;
    let already = instructions(words).any(|(op, ops)| {
        op == OP_DECORATE && ops.len() >= 2 && ops[0] == target && ops[1] == DECORATION_NON_READABLE
    });
    if already {
        return None;
    }
    let mut patched = Vec::with_capacity(words.len() + 3);
    patched.extend_from_slice(&words[..inject_at]);
    patched.extend_from_slice(&[(3 << 16) | OP_DECORATE as u32, target, DECORATION_NON_READABLE]);
    patched.extend_from_slice(&words[inject_at..]);
    Some(patched)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn inst(op: u16, operands: &[u32]) -> Vec<u32> {
        let mut v = vec![((operands.len() as u32 + 1) << 16) | op as u32];
        v.extend_from_slice(operands);
        v
    }

    fn string(s: &str) -> Vec<u32> {
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// A tiny module shaped like dxc output: entry `main` (%1), LocalSize
    /// 16x8x1, a storage image %5 at binding 2048.
    pub(crate) fn sample_module() -> Vec<u32> {
        let mut w = vec![MAGIC, 0x0001_0300, 0, 100, 0];
        let mut entry = vec![EXECUTION_MODEL_GL_COMPUTE, 1];
        entry.extend(string("main"));
        w.extend(inst(OP_ENTRY_POINT, &entry));
        w.extend(inst(OP_EXECUTION_MODE, &[1, MODE_LOCAL_SIZE, 16, 8, 1]));
        w.extend(inst(OP_DECORATE, &[5, 34, 0])); // DescriptorSet 0
        w.extend(inst(OP_DECORATE, &[5, DECORATION_BINDING, 2048]));
        w
    }

    fn bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn reads_words() {
        let m = sample_module();
        assert_eq!(words(&bytes(&m)).unwrap(), m);
        assert!(words(&[1, 2, 3]).is_err());
        assert!(words(&[0; 20]).is_err());
    }

    #[test]
    fn finds_entry_point() {
        let m = sample_module();
        assert_eq!(compute_entry(&m, ""), Some(("main".into(), 1)));
        assert_eq!(compute_entry(&m, "main"), Some(("main".into(), 1)));
        assert_eq!(compute_entry(&m, "other"), None);
    }

    #[test]
    fn reads_local_size() {
        let m = sample_module();
        assert_eq!(workgroup_size(&m, 1), Some([16, 8, 1]));
        assert_eq!(workgroup_size(&m, 2), None);
    }

    #[test]
    fn reads_local_size_id_and_builtin() {
        let mut m = vec![MAGIC, 0x0001_0300, 0, 100, 0];
        let mut entry = vec![EXECUTION_MODEL_GL_COMPUTE, 1];
        entry.extend(string("main"));
        m.extend(inst(OP_ENTRY_POINT, &entry));
        m.extend(inst(OP_EXECUTION_MODE_ID, &[1, MODE_LOCAL_SIZE_ID, 10, 11, 12]));
        m.extend(inst(OP_CONSTANT, &[9, 10, 32]));
        m.extend(inst(OP_CONSTANT, &[9, 11, 4]));
        m.extend(inst(OP_CONSTANT, &[9, 12, 1]));
        assert_eq!(workgroup_size(&m, 1), Some([32, 4, 1]));

        m.extend(inst(
            OP_DECORATE,
            &[20, DECORATION_BUILT_IN, BUILT_IN_WORKGROUP_SIZE],
        ));
        m.extend(inst(OP_CONSTANT_COMPOSITE, &[8, 20, 12, 12, 12]));
        assert_eq!(workgroup_size(&m, 1), Some([1, 1, 1]));
    }

    #[test]
    fn patches_non_readable_once() {
        let m = sample_module();
        let p = patch_non_readable(&m, 2048).unwrap();
        assert_eq!(p.len(), m.len() + 3);
        assert_eq!(
            &p[m.len()..],
            &[(3 << 16) | OP_DECORATE as u32, 5, DECORATION_NON_READABLE]
        );
        // The original instructions are all still there.
        assert_eq!(compute_entry(&p, ""), Some(("main".into(), 1)));
        assert_eq!(workgroup_size(&p, 1), Some([16, 8, 1]));
        assert_eq!(patch_non_readable(&p, 2048), None);
        assert_eq!(patch_non_readable(&m, 2049), None);
    }
}
