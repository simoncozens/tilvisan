#[derive(Debug, Clone)]
pub struct Bytecode(pub(crate) Vec<u8>);
use crate::opcodes::*;

pub(crate) const CONTROL_DELTA_PPEM_MIN: u8 = 6;
// pub(crate) const CONTROL_DELTA_PPEM_MAX: u8 = 53;

pub(crate) fn high(x: u32) -> u8 {
    (((x) & 0xFF00) >> 8) as u8
}

pub(crate) fn low(x: u32) -> u8 {
    (x & 0xFF) as u8
}

impl IntoIterator for Bytecode {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl Default for Bytecode {
    fn default() -> Self {
        Self::new()
    }
}

impl Bytecode {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn extend<T: IntoIterator<Item = u8>>(&mut self, other: T) {
        self.0.extend(other);
    }
    pub fn push_u8(&mut self, byte: u8) {
        self.0.push(byte);
    }

    pub fn push(
        &mut self,
        args: &[u32],
        need_words: bool,
        optimize: bool,
    ) -> Result<(), &'static str> {
        let mut args_iter = args.iter().copied();
        if need_words {
            let mut i = 0;
            while i < args.len() {
                let nargs = if args.len() - i > 255 {
                    255
                } else {
                    args.len() - i
                };
                if optimize && nargs <= 8 {
                    self.push_u8(PUSHW_1 - 1 + nargs as u8);
                } else {
                    self.push_u8(NPUSHW);
                    self.push_u8(nargs as u8);
                }
                for _ in 0..nargs {
                    let arg = args_iter.next().ok_or("Not enough arguments")?;
                    self.push_u8(high(arg));
                    self.push_u8(low(arg))
                }
                i += 255;
            }
        } else {
            let mut i = 0;
            while i < args.len() {
                let nargs = if args.len() - i > 255 {
                    255
                } else {
                    args.len() - i
                };
                if optimize && nargs <= 8 {
                    self.push_u8(PUSHB_1 - 1 + nargs as u8);
                } else {
                    self.push_u8(NPUSHB);
                    self.push_u8(nargs as u8);
                }
                for _ in 0..nargs {
                    let arg = args_iter.next().ok_or("Not enough arguments")?;
                    self.push_u8(arg as u8);
                }
                i += 255;
            }
        }
        Ok(())
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
    pub fn push_word(&mut self, word: u32) {
        self.push_u8(high(word));
        self.push_u8(low(word));
    }

    pub fn extend_bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    pub fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push_words(&mut self, args: &[u32], optimize: bool) -> Result<(), &'static str> {
        self.push(args, true, optimize)
    }

    pub fn push_bytes_args(&mut self, args: &[u32], optimize: bool) -> Result<(), &'static str> {
        self.push(args, false, optimize)
    }

    /// Optimize three adjacent NPUSHB push-blocks in the buffer.
    ///
    /// `pos` contains three byte offsets into `self` pointing to the start of
    /// successive NPUSHB instructions: point-hints block (`pos[0]`), action-hints
    /// block (`pos[1]`), and glyph-segments block (`pos[2]`).  When
    /// `pos[0] == pos[1]` the point-hints block is absent.
    ///
    /// If the total push data fits in at most two NPUSHB blocks (≤ 510 bytes
    /// combined), replaces `self[pos[0]..]` with the merged block(s) plus a
    /// trailing `CALL`.  Returns `true` if the buffer was modified, `false` if
    /// the optimization was skipped (NPUSHW encountered, data too large, or
    /// invalid offsets).
    ///
    /// This implements the same algorithm as C `TA_optimize_push`.
    pub fn optimize_push(&mut self, pos: [usize; 3]) -> bool {
        // Validate: each pos must have at least 2 bytes (opcode + count)
        if pos.iter().any(|&p| p + 1 >= self.0.len()) {
            return false;
        }

        // Skip NPUSHW — handling not implemented for this path
        if self.0[pos[0]] == NPUSHW || self.0[pos[1]] == NPUSHW || self.0[pos[2]] == NPUSHW {
            return false;
        }

        // When point-hints block is absent, slide pos[1]←pos[2], pos[2]←None
        let (p1, p2) = if pos[0] == pos[1] {
            (pos[2], None)
        } else {
            (pos[1], Some(pos[2]))
        };

        let size0 = self.0[pos[0] + 1] as usize;
        let size1 = self.0[p1 + 1] as usize;
        let size2 = p2.map(|p| self.0[p + 1] as usize).unwrap_or(0);

        let sum = size0 + size1 + size2;
        if sum == 0 {
            return false;
        }
        if sum > 2 * 0xFF {
            return false; // would still need three NPUSHB
        }
        if p2.is_none() && sum > 0xFF {
            return false; // two sections, already needs two NPUSHB
        }

        let (new_size1, new_size2) = if sum > 0xFF {
            (0xFF, sum - 0xFF)
        } else {
            (sum, 0)
        };

        // Collect payload bytes from each block, skipping the NPUSHB opcode+count headers
        let d0 = self.0[pos[0] + 2..pos[0] + 2 + size0].to_vec();
        let d1 = self.0[p1 + 2..p1 + 2 + size1].to_vec();
        let d2 = if let Some(p) = p2 {
            self.0[p + 2..p + 2 + size2].to_vec()
        } else {
            Vec::new()
        };

        let all_data: Vec<u8> = d0.into_iter().chain(d1).chain(d2).collect();

        // Encode merged form
        let mut out: Vec<u8> = Vec::with_capacity(all_data.len() + 4);
        if new_size1 <= 8 {
            out.push(PUSHB_1 - 1 + new_size1 as u8);
        } else {
            out.push(NPUSHB);
            out.push(new_size1 as u8);
        }
        out.extend_from_slice(&all_data[..new_size1]);
        if new_size2 > 0 {
            if new_size2 <= 8 {
                out.push(PUSHB_1 - 1 + new_size2 as u8);
            } else {
                out.push(NPUSHB);
                out.push(new_size2 as u8);
            }
            out.extend_from_slice(&all_data[new_size1..]);
        }
        out.push(CALL);

        // Replace everything from pos[0] onwards with the merged form
        self.0.truncate(pos[0]);
        self.0.extend_from_slice(&out);
        true
    }
}
