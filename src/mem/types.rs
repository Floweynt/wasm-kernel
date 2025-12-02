use crate::{
    arch::{PAGE_SMALL_SIZE, SMALL_PAGE_PAGE_SIZE, paging::get_higher_half_addr},
    mem::VM_LAYOUT,
};
use core::{
    iter::Step,
    ops::{Add, AddAssign, Range, Sub, SubAssign},
};
use derive_more::{Add, AddAssign, Constructor, Debug, Display, Mul, SubAssign};

pub trait Wrapper<T: Step>: Copy {
    fn new(inst: T) -> Self;
    fn value(self) -> T;
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor, Default, Display, Debug)]
#[display("{_0:#016x}")]
#[debug("VirtualAddress({_0:#016x})")]
pub struct VirtualAddress(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor, Default, Display, Debug)]
#[display("{_0:#x}")]
#[debug("PhysicalAddress({_0:#x})")]
pub struct PhysicalAddress(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor, Default, Display, Debug)]
#[display("{_0:#013x}")]
#[debug("VirtualPageFrameNumber({_0:#013x})")]
pub struct VirtualPageFrameNumber(u64);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Constructor, Default, Display, Debug)]
#[display("{_0:#x}")]
#[debug("PageFrameNumber({_0:#x})")]
pub struct PageFrameNumber(u64);

#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AddAssign,
    SubAssign,
    Add,
    Constructor,
    Mul,
    Default,
    Display,
    Debug,
)]
#[display("{_0:#x}")]
#[debug("ByteSize({_0:#x})")]
pub struct ByteSize(u64);

#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AddAssign,
    SubAssign,
    Add,
    Constructor,
    Mul,
    Default,
    Display,
    Debug,
)]
#[display("{_0:#x}")]
#[debug("ByteDiff({_0:#x})")]
pub struct ByteDiff(i64);

#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AddAssign,
    SubAssign,
    Add,
    Constructor,
    Mul,
    Default,
    Display,
    Debug,
)]
#[display("{_0:#x}")]
#[debug("PageSize({_0:#x})")]
pub struct PageSize(u64);

#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AddAssign,
    SubAssign,
    Add,
    Constructor,
    Mul,
    Default,
    Display,
    Debug,
)]
#[display("{_0:#x}")]
#[debug("PageDiff({_0:#x})")]
pub struct PageDiff(i64);

// size helpers

pub trait SizeType {
    fn size_bytes(self) -> u64;
}

impl SizeType for PageSize {
    fn size_bytes(self) -> u64 {
        self.0 * PAGE_SMALL_SIZE
    }
}

impl SizeType for ByteSize {
    fn size_bytes(self) -> u64 {
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

macro impl_value($type:ident, $value:ident) {
    impl Wrapper<$value> for $type {
        fn new(value: $value) -> Self {
            Self(value)
        }

        fn value(self) -> $value {
            self.0
        }
    }

    impl Step for $type {
        fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
            Step::steps_between(&start.value(), &end.value())
        }

        fn forward_checked(start: Self, count: usize) -> Option<Self> {
            Some(Self::new(Step::forward_checked(start.value(), count)?))
        }

        fn backward_checked(start: Self, count: usize) -> Option<Self> {
            Some(Self::new(Step::backward_checked(start.value(), count)?))
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

    impl_assign!($type, PageSize);
}

macro impl_diff($type:ident, $diff_type:ident) {
    impl Sub<$type> for $type {
        type Output = $diff_type;

        fn sub(self, rhs: $type) -> $diff_type {
            $diff_type(self.0.checked_signed_diff(rhs.0).unwrap())
        }
    }
}

impl_value!(ByteSize, u64);
impl_value!(ByteDiff, i64);
impl_value!(PageSize, u64);
impl_value!(PageDiff, i64);

impl_value!(VirtualAddress, u64);
impl_value!(PhysicalAddress, u64);
impl_value!(PageFrameNumber, u64);
impl_value!(VirtualPageFrameNumber, u64);

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

// impl

impl<T> From<*const T> for VirtualAddress {
    fn from(value: *const T) -> Self {
        Self(value as u64)
    }
}

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
    pub const fn size_of<T>() -> ByteSize {
        ByteSize::new(size_of::<T>() as u64)
    }

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

    pub fn is_higher_half(self) -> bool {
        self > get_higher_half_addr()
    }

    pub fn frame_containing(self) -> VirtualPageFrameNumber {
        VirtualPageFrameNumber(self.0 / PAGE_SMALL_SIZE)
    }

    pub fn frame_aligned(self) -> VirtualPageFrameNumber {
        assert!(self.is_aligned(SMALL_PAGE_PAGE_SIZE));
        VirtualPageFrameNumber(self.0 / PAGE_SMALL_SIZE)
    }

    pub fn as_ptr<T>(&self) -> *const T {
        return self.0 as *const T;
    }

    pub fn as_ptr_mut<T>(&self) -> *mut T {
        return self.0 as *mut T;
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.0 % size.size_bytes() == 0
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

    pub fn frame_containing(self) -> PageFrameNumber {
        PageFrameNumber(self.0 / PAGE_SMALL_SIZE)
    }

    pub fn frame_aligned(self) -> PageFrameNumber {
        assert!(self.is_aligned(SMALL_PAGE_PAGE_SIZE));
        PageFrameNumber(self.0 / PAGE_SMALL_SIZE)
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.0 % size.size_bytes() == 0
    }
}

impl PageFrameNumber {
    pub fn to_virtual(self) -> VirtualPageFrameNumber {
        self.address().to_virtual().frame_aligned()
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

    pub fn is_higher_half(self) -> bool {
        self.address().is_higher_half()
    }

    pub fn as_ptr<T>(self) -> *const T {
        self.address().as_ptr()
    }

    pub fn as_ptr_mut<T>(self) -> *mut T {
        self.address().as_ptr_mut()
    }

    pub fn is_aligned<T: SizeType>(self, size: T) -> bool {
        self.address().is_aligned(size)
    }
}

pub trait AddressRange<
    D,
    A: Sub<A, Output = D> + PartialOrd + Add<S, Output = A> + Copy + Wrapper<u64>,
    S: From<D>,
>: Sized + Copy
{
    fn as_rust_range(self) -> Range<A> {
        self.start()..self.end()
    }

    fn new(min: A, max: A) -> Self;

    fn sized(base: A, size: S) -> Self {
        Self::new(base, base + size)
    }

    fn start(&self) -> A;

    fn end(&self) -> A;

    fn size(&self) -> S {
        (self.end() - self.start()).into()
    }

    fn tup(&self) -> (A, A) {
        (self.start(), self.end())
    }

    fn empty(&self) -> bool {
        self.start() == self.end()
    }

    fn contains(&self, value: A) -> bool {
        self.start() <= value && value < self.end()
    }

    fn is_sub_range(&self, range: &Self) -> bool {
        self.start() <= range.start() && range.end() <= self.end()
    }

    fn intersects(&self, range: &Self) -> bool {
        self.start() < range.end() && range.start() < self.end()
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct VARange(VirtualAddress, VirtualAddress);

impl AddressRange<ByteDiff, VirtualAddress, ByteSize> for VARange {
    fn new(min: VirtualAddress, max: VirtualAddress) -> VARange {
        assert!(min <= max);
        VARange(min, max)
    }

    fn start(&self) -> VirtualAddress {
        self.0
    }

    fn end(&self) -> VirtualAddress {
        self.1
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct VFRange(VirtualPageFrameNumber, VirtualPageFrameNumber);

impl AddressRange<PageDiff, VirtualPageFrameNumber, PageSize> for VFRange {
    fn new(min: VirtualPageFrameNumber, max: VirtualPageFrameNumber) -> Self {
        assert!(min <= max);
        VFRange(min, max)
    }

    fn start(&self) -> VirtualPageFrameNumber {
        self.0
    }

    fn end(&self) -> VirtualPageFrameNumber {
        self.1
    }
}

impl VFRange {
    pub fn as_va_range(self) -> VARange {
        VARange(self.0.address(), self.1.address())
    }
}
