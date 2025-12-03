use core::{ffi::CStr, iter};

use spin::Once;
use static_assertions::const_assert;

pub struct SymbolModule<'a> {
    strings: &'a [u8],
    functions: &'a [u8],
    location_search: &'a [u8],
    function_search: &'a [u8],

    functions_count: usize,
    location_search_count: usize,
    function_search_count: usize,
}

const_assert!(size_of::<usize>() == size_of::<u64>());

macro generate_reader($name:ident, $ty:ty) {
    #[inline]
    fn $name(buf: &[u8], offset: usize) -> Option<$ty> {
        let size = core::mem::size_of::<$ty>();
        let bytes = buf.get(offset..offset + size)?;
        Some(<$ty>::from_le_bytes(bytes.try_into().ok()?))
    }
}

generate_reader!(read_usize, usize);
generate_reader!(read_u32, u32);
generate_reader!(read_u64, u64);

fn read_string<'a>(buf: &'a [u8], str_tab: &'a [u8], offset: usize) -> Option<&'a str> {
    let file = read_usize(buf, offset)?;

    if file != usize::MAX {
        Some(
            CStr::from_bytes_until_nul(&str_tab[file..])
                .ok()?
                .to_str()
                .ok()?,
        )
    } else {
        None
    }
}

trait TableEntry<'a>: Sized {
    const SIZE: usize;

    fn read(buf: &'a [u8], str_tab: &'a [u8]) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct LocationEntry<'a> {
    pub file: Option<&'a str>,
    pub row: u32,
    pub col: u32,
}

impl<'a> TableEntry<'a> for LocationEntry<'a> {
    const SIZE: usize = 8 + 4 + 4;

    fn read(buf: &'a [u8], str_tab: &'a [u8]) -> Option<Self> {
        Some(Self {
            file: read_string(buf, str_tab, 0),
            row: read_u32(buf, 8)?,
            col: read_u32(buf, 8 + 4)?,
        })
    }
}

impl<'a> TableEntry<'a> for Option<usize> {
    const SIZE: usize = 8;

    fn read(buf: &'a [u8], _str_tab: &'a [u8]) -> Option<Self> {
        let id = read_usize(buf, 0)?;
        Some(if id == usize::MAX { None } else { Some(id) })
    }
}

pub struct FunctionEntry<'a> {
    inline_parent: Option<usize>,
    pub name: Option<&'a str>,
    pub location: LocationEntry<'a>,
}

impl<'a> TableEntry<'a> for FunctionEntry<'a> {
    const SIZE: usize = 8 + 8 + LocationEntry::SIZE;

    fn read(buf: &'a [u8], str_tab: &'a [u8]) -> Option<Self> {
        let parent = read_usize(buf, 0)?;
        Some(Self {
            inline_parent: if parent == usize::MAX {
                None
            } else {
                Some(parent)
            },
            name: read_string(buf, str_tab, 8),
            location: LocationEntry::read(&buf[16..], str_tab)?,
        })
    }
}

pub fn parse<'a>(src: &'a [u8]) -> Option<SymbolModule<'a>> {
    let mut head = 0;

    let header = read_u64(src, head)?;
    assert!(header == 0);
    head += 8;

    let string_table_len = read_usize(src, head)?;
    head += 8;
    let string_table = &src[head..head + string_table_len];
    head += string_table_len;

    let function_table_len = read_usize(src, head)?;
    head += 8;
    let function_table = &src[head..head + function_table_len];
    head += function_table_len;

    let location_search_table_len = read_usize(src, head)?;
    head += 8;
    let location_search_table = &src[head..head + location_search_table_len];
    head += location_search_table_len;

    let function_search_table_len = read_usize(src, head)?;
    head += 8;
    let function_search_table = &src[head..head + function_search_table_len];
    head += function_search_table_len;

    assert!(head == src.len());

    Some(SymbolModule {
        strings: string_table,
        functions: function_table,
        location_search: location_search_table,
        function_search: function_search_table,

        functions_count: function_table_len / FunctionEntry::SIZE,
        location_search_count: location_search_table_len / (LocationEntry::SIZE + 4),
        function_search_count: function_search_table_len / (Option::<usize>::SIZE + 4),
    })
}

impl<'a> SymbolModule<'a> {
    fn do_read<T: TableEntry<'a>>(
        index: usize,
        len: usize,
        tab: &'a [u8],
        strings: &'a [u8],
    ) -> Option<(u32, T)> {
        assert!(index < len);
        let size = T::SIZE + 4;
        let offset = index * size;
        let slice = &tab[offset..offset + size];
        Some((read_u32(slice, 0)?, T::read(&slice[4..], strings)?))
    }

    fn get_function(&self, index: usize) -> Option<FunctionEntry<'a>> {
        assert!(index < self.functions_count);
        let offset = index * FunctionEntry::SIZE;
        let slice = &self.functions[offset..offset + FunctionEntry::SIZE];
        FunctionEntry::read(&slice, self.strings)
    }

    fn get_location_search(&self, index: usize) -> Option<(u32, LocationEntry<'a>)> {
        Self::do_read(
            index,
            self.location_search_count,
            self.location_search,
            self.strings,
        )
    }

    fn get_function_search(&self, index: usize) -> Option<(u32, Option<usize>)> {
        Self::do_read(
            index,
            self.function_search_count,
            self.function_search,
            self.strings,
        )
    }

    fn binary_search_table<T>(
        count: usize,
        get_entry: impl Fn(usize) -> Option<(u32, T)>,
        target: u32,
    ) -> Option<T> {
        let mut lo = 0;
        let mut hi = count;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (entry_offset, _) = get_entry(mid)?;
            if entry_offset <= target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo == 0 {
            None
        } else {
            let idx = lo - 1;
            let (_, entry_value) = get_entry(idx)?;
            Some(entry_value)
        }
    }

    fn symbolize<'b>(
        &'b self,
        addr: u64,
    ) -> (
        impl Iterator<Item = FunctionEntry<'b>> + 'b,
        Option<LocationEntry<'a>>,
    ) {
        let offset = (addr - 0xffffffff80000000) as u32;

        // Find the function containing the address
        let func_opt = Self::binary_search_table(
            self.function_search_count,
            |i| self.get_function_search(i),
            offset,
        );

        // Find the location entry
        let location = Self::binary_search_table(
            self.location_search_count,
            |i| self.get_location_search(i),
            offset,
        );

        (
            func_opt
                .flatten()
                .and_then(|f| self.get_function(f))
                .into_iter()
                .flat_map(move |func| {
                    iter::successors(Some(func), move |f| {
                        f.inline_parent.and_then(|idx| self.get_function(idx))
                    })
                }),
            location,
        )
    }
}

static GLOBAL_SYMBOLS: Once<SymbolModule<'static>> = Once::new();

pub fn try_init(data: SymbolModule<'static>) -> bool {
    if GLOBAL_SYMBOLS.is_completed() {
        return false;
    }

    GLOBAL_SYMBOLS.call_once(|| data);

    return true;
}

pub fn symbolize(
    addr: u64,
) -> (
    Option<impl Iterator<Item = FunctionEntry<'static>> + 'static>,
    Option<LocationEntry<'static>>,
) {
    if let Some(data) = GLOBAL_SYMBOLS.get() {
        let (iter, loc) = data.symbolize(addr);
        (Some(iter), loc)
    } else {
        (None, None)
    }
}
