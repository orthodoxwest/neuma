//! The engine's handles: items the glue holds by a number.
//!
//! A handle is a slot's index and its generation, the count of the slot's uses, packed into
//! a whole number below 2^52 so it crosses to JavaScript exactly as an `f64`. A handle kept
//! after its item was freed finds nothing rather than whatever took its slot later: the
//! generation is 32 bits, so a slot would have to be reused four billion times for an old
//! handle to name a new item.

/// Bits of a handle that name its slot.
const SLOT_BITS: u32 = 20;
const SLOT_MASK: u64 = (1 << SLOT_BITS) - 1;

/// A handle's value for "none".
pub(crate) const NONE: f64 = -1.0;

struct Slot<T> {
    generation: u32,
    /// When the item was last used, for [`Slab::evict_to`].
    used: u64,
    item: Option<T>,
}

/// Items the glue holds by handle.
pub(crate) struct Slab<T> {
    slots: Vec<Slot<T>>,
    live: usize,
    clock: u64,
}

impl<T> Slab<T> {
    pub(crate) const fn new() -> Slab<T> {
        Slab {
            slots: Vec::new(),
            live: 0,
            clock: 0,
        }
    }

    /// Puts `item` in the first free slot and returns its handle.
    pub(crate) fn put(&mut self, item: T) -> f64 {
        let i = match self.slots.iter().position(|s| s.item.is_none()) {
            Some(i) => i,
            None => {
                assert!((self.slots.len() as u64) < SLOT_MASK, "neuma: too many live handles");
                self.slots.push(Slot {
                    generation: 0,
                    used: 0,
                    item: None,
                });
                self.slots.len() - 1
            }
        };
        self.clock += 1;
        self.live += 1;
        let slot = &mut self.slots[i];
        slot.generation = slot.generation.wrapping_add(1);
        slot.used = self.clock;
        slot.item = Some(item);
        ((u64::from(slot.generation) << SLOT_BITS) | i as u64) as f64
    }

    fn slot(&mut self, handle: f64) -> Option<&mut Slot<T>> {
        // Anything but a whole number in range (a NaN, a negative, a fraction) finds nothing.
        if !(handle >= 0.0 && handle < (1u64 << 52) as f64 && handle.fract() == 0.0) {
            return None;
        }
        let h = handle as u64;
        let (generation, i) = ((h >> SLOT_BITS) as u32, (h & SLOT_MASK) as usize);
        self.slots.get_mut(i).filter(|s| s.generation == generation && s.item.is_some())
    }

    /// The item, marked as just used.
    pub(crate) fn get_mut(&mut self, handle: f64) -> Option<&mut T> {
        self.clock += 1;
        let clock = self.clock;
        self.slot(handle).and_then(|s| {
            s.used = clock;
            s.item.as_mut()
        })
    }

    pub(crate) fn free(&mut self, handle: f64) {
        if let Some(slot) = self.slot(handle) {
            slot.item = None;
            self.live -= 1;
        }
    }

    /// How many items are held.
    pub(crate) fn live(&self) -> usize {
        self.live
    }

    /// Frees the least recently used items until at most `budget` are held.
    pub(crate) fn evict_to(&mut self, budget: usize) {
        while self.live > budget {
            let Some(lru) = self.slots.iter_mut().filter(|s| s.item.is_some()).min_by_key(|s| s.used) else {
                return;
            };
            lru.item = None;
            self.live -= 1;
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
        assert_eq!(b as u64 & SLOT_MASK, a as u64 & SLOT_MASK, "the slot is reused");
        assert_ne!(a, b);
        assert_eq!(slab.get_mut(a), None);
        slab.free(a);
        assert_eq!(slab.get_mut(b).map(|s| *s), Some("b"));
        for bad in [NONE, f64::NAN, 0.5, f64::INFINITY, 1e300] {
            assert_eq!(slab.get_mut(bad), None);
        }
        assert_eq!(slab.live(), 1);
    }

    #[test]
    fn generations_outlast_many_reuses() {
        let mut slab = Slab::new();
        let first = slab.put(0);
        slab.free(first);
        for i in 1..100_000 {
            let h = slab.put(i);
            assert_ne!(h, first);
            slab.free(h);
        }
        assert_eq!(slab.get_mut(first), None);
    }

    #[test]
    fn evicts_the_least_recently_used() {
        let mut slab = Slab::new();
        let handles: Vec<f64> = (0..5).map(|i| slab.put(i)).collect();
        slab.get_mut(handles[0]);
        slab.evict_to(3);
        assert_eq!(slab.live(), 3);
        assert!(slab.get_mut(handles[0]).is_some(), "used last, so kept");
        assert!(slab.get_mut(handles[1]).is_none() && slab.get_mut(handles[2]).is_none());
        assert!(slab.get_mut(handles[3]).is_some() && slab.get_mut(handles[4]).is_some());
    }
}
