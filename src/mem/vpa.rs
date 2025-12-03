// TODO: make malloc

extern crate alloc;

use super::{AddressRange, PageFrameAllocator, PageSize, VFRange, VirtualPageFrameNumber};
use crate::arch::paging::{PageFlags, PageTableSet};
use alloc::boxed::Box;
use arrayvec::ArrayVec;
use intrusive_collections::{Bound, KeyAdapter, RBTree, RBTreeLink, UnsafeRef, intrusive_adapter};
use spin::{Mutex, Once};

pub trait VirtualAllocatorHandler {
    fn allocate(&mut self, size: PageSize) -> Option<VirtualPageFrameNumber>;

    fn free(&mut self, range: VFRange) -> Result<(), ()>;

    fn free_list_iterator(&self) -> impl Iterator<Item = &VFRange>;
}

pub struct VirtualAllocator<T: VirtualAllocatorHandler> {
    inner: Mutex<T>,
}

unsafe impl<T: VirtualAllocatorHandler> Sync for VirtualAllocator<T> {}
unsafe impl<T: VirtualAllocatorHandler> Send for VirtualAllocator<T> {}

pub struct VirtualAllocation<'a, T: VirtualAllocatorHandler> {
    usable: VFRange,
    range: VFRange,
    alloc: &'a VirtualAllocator<T>,
    is_dropped: bool,
}

impl<'a, T: VirtualAllocatorHandler> VirtualAllocation<'a, T> {
    pub fn leak(&mut self) -> VFRange {
        self.is_dropped = true;
        self.usable
    }

    pub fn range(&self) -> VFRange {
        self.range
    }
}

impl<'a, T: VirtualAllocatorHandler> Drop for VirtualAllocation<'a, T> {
    fn drop(&mut self) {
        if !self.is_dropped {
            self.is_dropped = true;
            self.alloc.free(self.range).expect("range already freed?");
        }
    }
}

pub struct BackedVirtualAllocation<'a, T: VirtualAllocatorHandler> {
    virtual_allocation: VirtualAllocation<'a, T>,
}

impl<'a, T: VirtualAllocatorHandler> Drop for BackedVirtualAllocation<'a, T> {
    fn drop(&mut self) {
        if self.virtual_allocation.is_dropped {
            return;
        }

        todo!()
    }
}

impl<'a, T: VirtualAllocatorHandler> BackedVirtualAllocation<'a, T> {
    pub fn leak(&mut self) -> VFRange {
        self.virtual_allocation.leak()
    }

    pub fn range(&self) -> VFRange {
        self.virtual_allocation.range()
    }
}

impl VirtualAllocator<EarlyAllocator> {
    pub fn early(range: VFRange, reservations: &[VFRange]) -> Result<Self, ()> {
        let mut early = EarlyAllocator::new(range);

        for ele in reservations {
            early.reserve_range(*ele)?;
        }

        Ok(VirtualAllocator {
            inner: Mutex::new(early),
        })
    }
}

impl VirtualAllocator<TreeAllocator> {
    pub fn tree(range: VirtualAllocator<EarlyAllocator>) -> VirtualAllocator<TreeAllocator> {
        VirtualAllocator {
            inner: Mutex::new(TreeAllocator::new(&*range.inner.lock())),
        }
    }
}

impl<T: VirtualAllocatorHandler> VirtualAllocator<T> {
    pub fn allocate_padded(
        &self,
        size: PageSize,
        padding: PageSize,
    ) -> Option<VirtualAllocation<'_, T>> {
        let alloc_size = padding + size + padding;
        self.inner
            .lock()
            .allocate(alloc_size)
            .map(|base| VirtualAllocation {
                range: VFRange::sized(base, alloc_size),
                usable: VFRange::sized(base + padding, size),
                alloc: self,
                is_dropped: false,
            })
    }

    pub fn allocate(&self, size: PageSize) -> Option<VirtualAllocation<'_, T>> {
        self.allocate_padded(size, PageSize::new(0))
    }

    pub fn allocate_backed_padded<P: PageFrameAllocator>(
        &self,
        pmm: &P,
        tables: &PageTableSet,
        size: PageSize,
        padding: PageSize,
        flags: PageFlags,
    ) -> Option<BackedVirtualAllocation<'_, T>> {
        let range = self.allocate_padded(size, padding)?;
        for addr in range.range().as_rust_range() {
            let phys = pmm.allocate_single_page();
            tables.map_page_small(pmm, addr, phys, &flags);
        }

        Some(BackedVirtualAllocation {
            virtual_allocation: range,
        })
    }

    pub fn allocate_backed<P: PageFrameAllocator>(
        &self,
        pmm: &P,
        tables: &PageTableSet,
        size: PageSize,
        flags: PageFlags,
    ) -> Option<BackedVirtualAllocation<'_, T>> {
        self.allocate_backed_padded(pmm, tables, size, PageSize::new(0), flags)
    }

    pub fn free(&self, range: VFRange) -> Result<(), ()> {
        self.inner.lock().free(range)
    }
}

// the "very early" virtual page allocator
pub struct EarlyAllocator {
    free_ranges: ArrayVec<VFRange, 8>,
}

impl EarlyAllocator {
    fn new(range: VFRange) -> EarlyAllocator {
        EarlyAllocator {
            free_ranges: {
                let mut vec = ArrayVec::new();
                vec.push(range);
                vec
            },
        }
    }

    fn reserve_range(&mut self, range: VFRange) -> Result<(), ()> {
        for (index, value) in self.free_ranges.iter().enumerate() {
            if value.intersects(&range) {
                if !value.is_sub_range(&range) {
                    return Err(());
                }

                let pre = VFRange::new(value.start(), range.start());
                let post = VFRange::new(range.end(), value.end());

                self.free_ranges.remove(index);
                self.free_ranges
                    .try_insert(index, pre)
                    .expect("EarlyVirtualAllocator: free range buffer full");
                self.free_ranges
                    .try_insert(index + 1, post)
                    .expect("EarlyVirtualAllocator: free range buffer full");

                assert!(self.free_ranges.iter().map(|f| f.start()).is_sorted());

                return Ok(());
            }
        }

        return Err(());
    }
}

impl VirtualAllocatorHandler for EarlyAllocator {
    fn allocate(&mut self, size: PageSize) -> Option<VirtualPageFrameNumber> {
        for (index, value) in self.free_ranges.iter().enumerate() {
            if value.size() >= size {
                let new_range = VFRange::new(value.start() + size, value.end());
                let start = value.start();

                if new_range.empty() {
                    self.free_ranges.remove(index);
                } else {
                    self.free_ranges[index] = new_range;
                }

                assert!(self.free_ranges.iter().map(|f| f.start()).is_sorted());

                return Some(start);
            }
        }

        None
    }

    fn free(&mut self, _: VFRange) -> Result<(), ()> {
        panic!("not implemented")
    }

    fn free_list_iterator(&self) -> impl Iterator<Item = &VFRange> {
        self.free_ranges.iter()
    }
}

// the "early" virtual page allocator implementation

struct VirtualPageNode {
    size_tree_link: RBTreeLink,
    base_tree_link: RBTreeLink,
    range: VFRange,
}

intrusive_adapter!(VirtualRangeSizeAdapter = Box<VirtualPageNode>: VirtualPageNode { size_tree_link: RBTreeLink });
intrusive_adapter!(VirtualRangeBaseAdapter = UnsafeRef<VirtualPageNode>: VirtualPageNode { base_tree_link: RBTreeLink });

impl<'a> KeyAdapter<'a> for VirtualRangeSizeAdapter {
    type Key = PageSize;

    fn get_key(&self, e: &'a VirtualPageNode) -> PageSize {
        e.range.size()
    }
}

impl<'a> KeyAdapter<'a> for VirtualRangeBaseAdapter {
    type Key = VirtualPageFrameNumber;

    fn get_key(&self, e: &'a VirtualPageNode) -> VirtualPageFrameNumber {
        e.range.start()
    }
}

pub struct TreeAllocator {
    by_size: RBTree<VirtualRangeSizeAdapter>,
    by_base: RBTree<VirtualRangeBaseAdapter>,
}

impl TreeAllocator {
    fn new<T: VirtualAllocatorHandler>(allocator: &T) -> TreeAllocator {
        let mut alloc = TreeAllocator {
            by_size: RBTree::default(),
            by_base: RBTree::default(),
        };

        for range in allocator.free_list_iterator() {
            alloc.insert(Box::new(VirtualPageNode {
                size_tree_link: RBTreeLink::default(),
                base_tree_link: RBTreeLink::default(),
                range: *range,
            }));
        }

        alloc
    }

    fn insert(&mut self, node_box: Box<VirtualPageNode>) {
        let raw: *mut VirtualPageNode = Box::into_raw(node_box);
        let unsafe_ref = unsafe { UnsafeRef::from_raw(raw) };
        self.by_base.insert(unsafe_ref);
        let node_box = unsafe { Box::from_raw(raw) };
        self.by_size.insert(node_box);
    }
}

impl VirtualAllocatorHandler for TreeAllocator {
    fn allocate(&mut self, size: PageSize) -> Option<VirtualPageFrameNumber> {
        let mut cursor = self.by_size.lower_bound_mut(Bound::Included(&size));

        if let Some(node) = cursor.get() {
            let allocated_base = node.range.start();

            unsafe { self.by_base.cursor_mut_from_ptr(node).remove() };
            let mut node_box = cursor.remove().unwrap();

            node_box.range = VFRange::new(node_box.range.start() + size, node_box.range.end());

            if !node_box.range.empty() {
                self.insert(node_box);
            }

            Some(allocated_base)
        } else {
            None
        }
    }

    fn free(&mut self, mut range: VFRange) -> Result<(), ()> {
        let left_cursor = self.by_base.lower_bound(Bound::Excluded(&range.start()));
        let right_cursor = self.by_base.lower_bound(Bound::Included(&range.end()));

        if let Some(left) = left_cursor.get() {
            if left.range.intersects(&range) {
                return Err(());
            }
        }

        if let Some(right) = right_cursor.get() {
            if right.range.intersects(&range) {
                return Err(());
            }
        }

        let mut left_cursor_mut = self
            .by_base
            .lower_bound_mut(Bound::Excluded(&range.start()));

        if let Some(left) = left_cursor_mut.get() {
            if left.range.end() == range.start() {
                range = VFRange::new(left.range.start(), range.end());

                let node_box = left_cursor_mut.remove().unwrap();
                unsafe {
                    self.by_size.cursor_mut_from_ptr(&*node_box).remove();
                }
            }
        }

        let mut right_cursor_mut = self.by_base.lower_bound_mut(Bound::Included(&range.end()));

        if let Some(right) = right_cursor_mut.get() {
            if right.range.start() == range.end() {
                range = VFRange::new(range.start(), right.range.end());

                let node_box = right_cursor_mut.remove().unwrap();
                unsafe {
                    self.by_size.cursor_mut_from_ptr(&*node_box).remove();
                }
            }
        }

        self.insert(Box::new(VirtualPageNode {
            size_tree_link: RBTreeLink::default(),
            base_tree_link: RBTreeLink::default(),
            range,
        }));

        Ok(())
    }

    fn free_list_iterator(&self) -> impl Iterator<Item = &VFRange> {
        self.by_base.iter().map(|f| &f.range)
    }
}

static GLOBAL_VPA: Once<VirtualAllocator<TreeAllocator>> = Once::new();

pub(super) fn initialize(alloc: VirtualAllocator<TreeAllocator>) {
    GLOBAL_VPA.call_once(|| alloc);
}

pub fn get_global_vpa() -> &'static VirtualAllocator<TreeAllocator> {
    GLOBAL_VPA.get().expect("vpa: GLOBAL_VPA not initialized")
}
