const PAGE: usize = 4096;

#[repr(C)]
struct Node {
    next: usize,
    pages: usize,
}

/// Metadata lives in free pages, so a full metadata table cannot lose a run.
pub struct Runs {
    head: usize,
}
impl Runs {
    pub const fn new() -> Self {
        Self { head: 0 }
    }

    /// # Safety
    /// The range must be mapped, page-aligned, disjoint from other free ranges,
    /// and wholly unused until returned by `take` or `rewind`.
    pub unsafe fn insert(&mut self, address: usize, pages: usize) {
        debug_assert!(address != 0 && (address & (PAGE - 1)) == 0 && pages > 0);
        let mut link = &mut self.head as *mut usize;
        let mut previous = 0;
        while *link != 0 && *link < address {
            previous = *link;
            link = &mut (*(previous as *mut Node)).next;
        }
        let next = *link;
        (address as *mut Node).write(Node { next, pages });
        *link = address;
        let node = &mut *(address as *mut Node);
        if next != 0 && address + pages * PAGE == next {
            let following = &*(next as *const Node);
            node.pages += following.pages;
            node.next = following.next;
        }
        if previous != 0 {
            let prior = &mut *(previous as *mut Node);
            if previous + prior.pages * PAGE == address {
                prior.pages += node.pages;
                prior.next = node.next;
            }
        }
    }

    /// # Safety
    /// Stored ranges must still satisfy `insert`'s invariants. Alignment must
    /// be a nonzero power of two, and pages must be positive.
    pub unsafe fn take(&mut self, pages: usize, alignment: usize) -> Option<usize> {
        let alignment = alignment.max(PAGE);
        let mut link = &mut self.head as *mut usize;
        while *link != 0 {
            let address = *link;
            let node = &*(address as *const Node);
            let start = (address + alignment - 1) & !(alignment - 1);
            let prefix = (start - address) / PAGE;
            if node.pages >= prefix + pages {
                let next = node.next;
                let suffix = node.pages - prefix - pages;
                *link = next;
                if prefix > 0 {
                    self.insert(address, prefix);
                }
                if suffix > 0 {
                    self.insert(start + pages * PAGE, suffix);
                }
                return Some(start);
            }
            link = &mut (*(address as *mut Node)).next;
        }
        None
    }

    /// # Safety
    /// The bump must start the uncarved tail. Stored ranges obey `insert`.
    pub unsafe fn rewind(&mut self, bump: &mut usize) {
        let mut link = &mut self.head as *mut usize;
        while *link != 0 {
            let address = *link;
            let node = &*(address as *const Node);
            if address + node.pages * PAGE == *bump {
                *bump = address;
                *link = node.next;
                return;
            }
            link = &mut (*(address as *mut Node)).next;
        }
    }

    /// # Safety
    /// Stored ranges must remain mapped and valid.
    pub unsafe fn cached_pages(&self) -> usize {
        let mut address = self.head;
        let mut total = 0;
        while address != 0 {
            let node = &*(address as *const Node);
            total += node.pages;
            address = node.next;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{alloc_zeroed, dealloc, Layout};
    struct Memory {
        ptr: *mut u8,
        layout: Layout,
    }
    impl Memory {
        fn new(pages: usize) -> Self {
            let layout = Layout::from_size_align(pages * PAGE, 64 * PAGE).unwrap();
            let ptr = unsafe { alloc_zeroed(layout) };
            assert!(!ptr.is_null());
            Self { ptr, layout }
        }
        fn address(&self, page: usize) -> usize {
            self.ptr as usize + page * PAGE
        }
    }
    impl Drop for Memory {
        fn drop(&mut self) {
            unsafe { dealloc(self.ptr, self.layout) }
        }
    }
    #[test]
    fn coalesces_both_sides_and_reuses_more_than_64_runs() {
        let memory = Memory::new(300);
        let mut runs = Runs::new();
        unsafe {
            for page in (0..300).step_by(2) {
                runs.insert(memory.address(page), 1);
            }
            assert_eq!(runs.cached_pages(), 150);
            assert_eq!(runs.take(2, PAGE), None);
            for page in (1..300).step_by(2) {
                runs.insert(memory.address(page), 1);
            }
            assert_eq!(runs.take(300, PAGE), Some(memory.address(0)));
            assert_eq!(runs.cached_pages(), 0);
        }
    }
    #[test]
    fn alignment_splits_preserve_prefix_suffix_and_live_gap() {
        let memory = Memory::new(32);
        let mut runs = Runs::new();
        unsafe {
            runs.insert(memory.address(1), 31);
            let allocated = runs.take(3, 8 * PAGE).unwrap();
            assert_eq!(allocated, memory.address(8));
            assert_eq!(runs.cached_pages(), 28);
            assert_eq!(runs.take(22, PAGE), None);
            runs.insert(allocated, 3);
            let mut bump = memory.address(32);
            runs.rewind(&mut bump);
            assert_eq!(bump, memory.address(1));
            assert_eq!(runs.cached_pages(), 0);
        }
    }
}
