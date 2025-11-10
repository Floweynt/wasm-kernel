use crate::{arch::PAGE_SMALL_SIZE, mem::VM_LAYOUT};
use core::{
    fmt::{Debug, Display},
    ops::{Add, AddAssign, Sub, SubAssign},
};
use derive_more::{Add, AddAssign, Constructor, SubAssign};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor)]
pub struct VirtualAddress(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor)]
pub struct PhysicalAddress(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor)]
pub struct PageFrameNumber(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor)]
pub struct VirtualPageFrameNumber(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AddAssign, SubAssign, Add, Constructor)]
pub struct ByteSize(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AddAssign, SubAssign, Add, Constructor)]
pub struct ByteDiff(i64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AddAssign, SubAssign, Add, Constructor)]
pub struct PageSize(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AddAssign, SubAssign, Add, Constructor)]
pub struct PageDiff(i64);

pub trait SizeType {
    fn size(self) -> u64;
}

impl SizeType for PageSize {
    fn size(self) -> u64 {
        self.0 * PAGE_SMALL_SIZE
    }
}

impl SizeType for ByteSize {
    fn size(self) -> u64 {
        self.0
    }
}

// implementations

macro impl_assign($type:ident, $delta:ident) {
    impl AddAssign<$delta> for $type {
        fn add_assign(&mut self, other: $delta) {
            *self = self.clone() + other;
        }
    }

    impl SubAssign<$delta> for $type {
        fn sub_assign(&mut self, other: $delta) {
            *self = self.clone() - other;
        }
    }
}

macro impl_unwrap_into($type:ident, $value:ident) {
    impl Into<$value> for $type {
        fn into(self) -> $value {
            self.0
        }
    }
}

macro impl_conv($src:ident, $dest:ident) {
    impl From<$src> for $dest {
        fn from(value: $src) -> Self {
            $dest(value.0.try_into().unwrap_or_else(|_| {
                panic!(
                    concat!(
                        "could not convert ",
                        stringify!($dest),
                        " to ",
                        stringify!($src),
                        " because {} does not fit"
                    ),
                    value.0
                )
            }))
        }
    }
}

macro impl_math($op:ident, $delta:ident, $type:ident, $op_name: ident, $impl_name:ident) {
    impl $op<$delta> for $type {
        type Output = $type;

        fn $op_name(self, rhs: $delta) -> Self::Output {
            $type(self.0.$impl_name(rhs.0.try_into().unwrap()).unwrap())
        }
    }
}

macro impl_pagesize_math($type:ident) {
    impl Add<PageSize> for $type {
        type Output = $type;

        fn add(self, rhs: PageSize) -> Self::Output {
            self + Into::<ByteSize>::into(rhs)
        }
    }

    impl Sub<PageSize> for $type {
        type Output = $type;

        fn sub(self, rhs: PageSize) -> Self::Output {
            self - Into::<ByteSize>::into(rhs)
        }
    }
}

macro impl_diff($type:ident, $diff_type:ident) {
    impl Sub<$type> for $type {
        type Output = $diff_type;

        fn sub(self, rhs: $type) -> $diff_type {
            $diff_type(self.0.checked_signed_diff(rhs.0).unwrap())
        }
    }
}

impl_unwrap_into!(ByteSize, u64);
impl_unwrap_into!(ByteDiff, i64);
impl_unwrap_into!(PageSize, u64);
impl_unwrap_into!(PageDiff, i64);

impl_unwrap_into!(VirtualAddress, u64);
impl_unwrap_into!(PhysicalAddress, u64);
impl_unwrap_into!(PageFrameNumber, u64);
impl_unwrap_into!(VirtualPageFrameNumber, u64);

impl_conv!(ByteSize, ByteDiff);
impl_conv!(ByteDiff, ByteSize);
impl_conv!(PageDiff, PageSize);
impl_conv!(PageSize, PageDiff);

impl_math!(Add, ByteDiff, VirtualAddress, add, checked_add);
impl_math!(Sub, ByteDiff, VirtualAddress, sub, checked_sub);
impl_math!(Add, ByteSize, VirtualAddress, add, checked_add);
impl_math!(Sub, ByteSize, VirtualAddress, sub, checked_sub);
impl_assign!(VirtualAddress, ByteSize);
impl_assign!(VirtualAddress, ByteDiff);

impl_math!(Add, ByteDiff, PhysicalAddress, add, checked_add);
impl_math!(Sub, ByteDiff, PhysicalAddress, sub, checked_sub);
impl_math!(Add, ByteSize, PhysicalAddress, add, checked_add);
impl_math!(Sub, ByteSize, PhysicalAddress, sub, checked_sub);
impl_assign!(PhysicalAddress, ByteSize);
impl_assign!(PhysicalAddress, ByteDiff);

impl_math!(Add, PageDiff, PageFrameNumber, add, checked_add);
impl_math!(Sub, PageDiff, PageFrameNumber, sub, checked_sub);
impl_math!(Add, PageSize, PageFrameNumber, add, checked_add);
impl_math!(Sub, PageSize, PageFrameNumber, sub, checked_sub);
impl_assign!(PageFrameNumber, PageSize);
impl_assign!(PageFrameNumber, PageDiff);

impl_math!(Add, PageDiff, VirtualPageFrameNumber, add, checked_add);
impl_math!(Sub, PageDiff, VirtualPageFrameNumber, sub, checked_sub);
impl_math!(Add, PageSize, VirtualPageFrameNumber, add, checked_add);
impl_math!(Sub, PageSize, VirtualPageFrameNumber, sub, checked_sub);
impl_assign!(VirtualPageFrameNumber, PageSize);
impl_assign!(VirtualPageFrameNumber, PageDiff);

impl_pagesize_math!(VirtualAddress);
impl_pagesize_math!(PhysicalAddress);

impl_diff!(VirtualAddress, ByteDiff);
impl_diff!(PhysicalAddress, ByteDiff);
impl_diff!(PageFrameNumber, PageDiff);
impl_diff!(VirtualPageFrameNumber, PageDiff);

// display/debug impl

impl Display for VirtualAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl Debug for VirtualAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtualAddress({:016x})", self.0)
    }
}

impl Display for PhysicalAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl Debug for PhysicalAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PhysicalAddress({:x})", self.0)
    }
}

impl Display for PageFrameNumber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for PageFrameNumber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PageFrameNumber({})", self.0)
    }
}

// impl

impl TryFrom<ByteSize> for PageSize {
    type Error = ();

    fn try_from(value: ByteSize) -> Result<Self, Self::Error> {
        if value.0 % PAGE_SMALL_SIZE != 0 {
            Err(())
        } else {
            Ok(PageSize(value.0 / PAGE_SMALL_SIZE))
        }
    }
}

impl ByteSize {
    pub fn page_size_roundup(self) -> PageSize {
        PageSize((self.0 + PAGE_SMALL_SIZE - 1) / PAGE_SMALL_SIZE)
    }
}

impl From<PageSize> for ByteSize {
    fn from(value: PageSize) -> Self {
        ByteSize(
            value
                .0
                .checked_mul(PAGE_SMALL_SIZE.try_into().unwrap())
                .unwrap(),
        )
    }
}

impl VirtualAddress {
    pub fn hhdm_to_physical(self) -> PhysicalAddress {
        let layout = VM_LAYOUT.get().expect("vm layout not initialized");
        assert!(layout.hhdm_base <= self && self < layout.hhdm_end);
        PhysicalAddress::new(0) + (self - layout.hhdm_base)
    }

    pub fn kernel_to_physical(self) -> PhysicalAddress {
        let layout = VM_LAYOUT.get().expect("vm layout not initialized");
        assert!(layout.kernel_base <= self && self < layout.kernel_end);
        layout.kernel_phys_base + (self - layout.kernel_base)
    }

    pub fn frame_number(self) -> VirtualPageFrameNumber {
        VirtualPageFrameNumber(self.0 / PAGE_SMALL_SIZE)
    }

    pub fn as_pointer<T>(&self) -> *mut T {
        return self.0 as *mut T;
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.0 % size.size() == 0
    }
}

impl PhysicalAddress {
    pub fn to_virtual(self) -> VirtualAddress {
        let layout = VM_LAYOUT.get().expect("vm layout not initialized");
        let res = layout.hhdm_base + (self - PhysicalAddress::new(0));
        assert!(
            res < layout.hhdm_end,
            "virtual address for physical address ({}) out-of-bounds (hhdm only maps {}-{})",
            res,
            layout.hhdm_base,
            layout.hhdm_end
        );
        res
    }

    pub fn frame_number(self) -> PageFrameNumber {
        PageFrameNumber(self.0 / PAGE_SMALL_SIZE)
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.0 % size.size() == 0
    }
}

impl PageFrameNumber {
    pub fn to_virtual(self) -> VirtualPageFrameNumber {
        self.address().to_virtual().frame_number()
    }

    pub fn address(self) -> PhysicalAddress {
        PhysicalAddress(self.0.checked_mul(PAGE_SMALL_SIZE).expect(""))
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.address().is_aligned(size)
    }
}

impl VirtualPageFrameNumber {
    pub fn address(self) -> VirtualAddress {
        VirtualAddress(self.0.checked_mul(PAGE_SMALL_SIZE).expect(""))
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.address().is_aligned(size)
    }
}
