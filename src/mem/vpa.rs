// TODO: make malloc

extern crate alloc;
use core::default;

use super::{AddressRange, PageSize, VFRange, VirtualPageFrameNumber};
use crate::arch::paging::PageTableSet;
use alloc::boxed::Box;
use arrayvec::ArrayVec;
use intrusive_collections::{
    Bound, KeyAdapter, RBTree, RBTreeLink, UnsafeRef, intrusive_adapter, linked_list::CursorMut,
};

pub trait VirtualAllocator {
    fn allocate(&mut self, size: PageSize) -> Option<VirtualPageFrameNumber>;

    fn free(&mut self, base: VirtualPageFrameNumber, size: PageSize) -> Result<(), ()>;

    fn free_list_iterator(&self) -> impl Iterator<Item = &VFRange>;
}

// the "very early" virtual page allocator
pub struct EarlyVirtualAllocator {
    free_ranges: ArrayVec<VFRange, 8>,
}

impl EarlyVirtualAllocator {
    pub fn new(
        start: VirtualPageFrameNumber,
        end: VirtualPageFrameNumber,
    ) -> EarlyVirtualAllocator {
        EarlyVirtualAllocator {
            free_ranges: {
                let mut vec = ArrayVec::new();
                vec.push(VFRange::new(start, end));
                vec
            },
        }
    }

    pub fn reserve_range(&mut self, range: VFRange) -> Result<(), ()> {
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

impl VirtualAllocator for EarlyVirtualAllocator {
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

    fn free(&mut self, _: VirtualPageFrameNumber, _: PageSize) -> Result<(), ()> {
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

pub struct TreeVirtualAllocator {
    by_size: RBTree<VirtualRangeSizeAdapter>,
    by_base: RBTree<VirtualRangeBaseAdapter>,
}

impl TreeVirtualAllocator {
    pub fn new<T: VirtualAllocator>(allocator: T) -> TreeVirtualAllocator {
        let mut alloc = TreeVirtualAllocator {
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

impl VirtualAllocator for TreeVirtualAllocator {
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

    fn free(&mut self, base: VirtualPageFrameNumber, size: PageSize) -> Result<(), ()> {
        let mut range = VFRange::new(base, base + size);

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

pub struct AddressSpace<T: VirtualAllocator> {
    pub virtual_alloc: T,
    pub tables: PageTableSet,
}
