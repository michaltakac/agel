//! Checksummed append-head metadata; data sectors are never rewritten.

pub const SECTOR_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Head {
    pub generation: u64,
    pub sectors: u32,
}

#[inline(always)]
fn checksum(bytes: &[u8; SECTOR_BYTES]) -> u64 {
    let mut value = 0xcbf29ce484222325_u64;
    for byte in bytes {
        value = (value ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    value
}

impl Head {
    #[inline(always)]
    pub fn encode(self) -> [u8; SECTOR_BYTES] {
        let mut bytes = [0; SECTOR_BYTES];
        bytes[..8].copy_from_slice(b"AGELJNL1");
        bytes[8..16].copy_from_slice(&self.generation.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.sectors.to_le_bytes());
        let digest = checksum(&bytes);
        bytes[24..32].copy_from_slice(&digest.to_le_bytes());
        bytes
    }

    #[inline(always)]
    pub fn decode(mut bytes: [u8; SECTOR_BYTES], capacity: u32) -> Option<Self> {
        if &bytes[..8] != b"AGELJNL1" {
            return None;
        }
        let mut stored = [0; 8];
        stored.copy_from_slice(&bytes[24..32]);
        bytes[24..32].fill(0);
        if checksum(&bytes) != u64::from_le_bytes(stored) {
            return None;
        }
        let mut generation = [0; 8];
        generation.copy_from_slice(&bytes[8..16]);
        let mut sectors = [0; 4];
        sectors.copy_from_slice(&bytes[16..20]);
        let head = Self {
            generation: u64::from_le_bytes(generation),
            sectors: u32::from_le_bytes(sectors),
        };
        if head.sectors > capacity {
            None
        } else {
            Some(head)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torn_metadata_never_selects_an_invented_head() {
        let old = Head {
            generation: 1,
            sectors: 7,
        };
        let new = Head {
            generation: 2,
            sectors: 8,
        };
        for cut in 0..=512 {
            let mut torn = old.encode();
            torn[..cut].copy_from_slice(&new.encode()[..cut]);
            if let Some(head) = Head::decode(torn, 100) {
                assert!(head == old || head == new, "cut {cut}: {head:?}");
            }
        }
        assert_eq!(Head::decode(new.encode(), 7), None);
        let mut corrupt = new.encode();
        corrupt[400] = 1;
        assert_eq!(Head::decode(corrupt, 100), None);
    }
}
