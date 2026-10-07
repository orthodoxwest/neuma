//! The engine's handles: items the glue holds by a number.

/// Bits of a handle that name its slot; the rest count the slot's reuses, so a handle kept
/// after its item was freed finds nothing rather than whatever took its slot.
const SLOT_BITS: u32 = 20;
const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;

/// Items the glue holds by handle.
pub(crate) struct Slab<T> {
    slots: Vec<(u32, Option<T>)>,
}

impl<T> Slab<T> {
    pub(crate) const fn new() -> Slab<T> {
        Slab { slots: Vec::new() }
    }

    /// Puts `item` in the first free slot and returns its handle.
    pub(crate) fn put(&mut self, item: T) -> u32 {
        let i = match self.slots.iter().position(|s| s.1.is_none()) {
            Some(i) => i,
            None => {
                assert!(self.slots.len() < SLOT_MASK as usize, "neuma: too many live handles");
                self.slots.push((0, None));
                self.slots.len() - 1
            }
        };
        let slot = &mut self.slots[i];
        slot.0 = (slot.0 + 1) & (u32::MAX >> SLOT_BITS);
        slot.1 = Some(item);
        (slot.0 << SLOT_BITS) | i as u32
    }

    fn slot(&mut self, handle: u32) -> Option<&mut Option<T>> {
        let (reuse, i) = (handle >> SLOT_BITS, handle & SLOT_MASK);
        self.slots.get_mut(i as usize).filter(|s| s.0 == reuse).map(|s| &mut s.1)
    }

    pub(crate) fn get(&self, handle: u32) -> Option<&T> {
        let (reuse, i) = (handle >> SLOT_BITS, handle & SLOT_MASK);
        self.slots.get(i as usize).filter(|s| s.0 == reuse).and_then(|s| s.1.as_ref())
    }

    pub(crate) fn get_mut(&mut self, handle: u32) -> Option<&mut T> {
        self.slot(handle).and_then(Option::as_mut)
    }

    pub(crate) fn free(&mut self, handle: u32) {
        if let Some(slot) = self.slot(handle) {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_freed_handle_finds_nothing_after_its_slot_is_reused() {
        let mut slab = Slab::new();
        let a = slab.put("a");
        slab.free(a);
        let b = slab.put("b");
        assert_eq!(b & SLOT_MASK, a & SLOT_MASK, "the slot is reused");
        assert_ne!(a, b);
        assert_eq!(slab.get(a), None);
        slab.free(a);
        assert_eq!(slab.get(b), Some(&"b"));
        assert_eq!(slab.get_mut(b).map(|s| *s), Some("b"));
        assert_eq!(slab.get(u32::MAX), None);
        // Reuses wrap around without ever naming u32::MAX.
        for _ in 0..5000 {
            let h = slab.put("c");
            assert_ne!(h, u32::MAX);
            slab.free(h);
        }
        assert_eq!(slab.get(b), Some(&"b"));
    }
}
