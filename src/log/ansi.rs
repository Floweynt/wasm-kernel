use core::fmt::{
    Binary, Debug, Display, Formatter, LowerExp, LowerHex, Octal, Pointer, Result, UpperExp,
    UpperHex,
};

use bitflags::bitflags;

#[derive(Clone, Copy)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const fn rgb(&self) -> u32 {
        ((self.0 as u32) << 16) | ((self.1 as u32) << 8) | (self.2 as u32)
    }

    pub const fn from_rgb(data: u32) -> Color {
        Color((data >> 16) as u8, (data >> 8) as u8, data as u8)
    }

    pub const BACKGROUND: Color = Self::from_rgb(0x131a1c);
    pub const FOREGROUND: Color = Self::from_rgb(0xc5c8c9);
    pub const CURSOR: Color = Self::from_rgb(0x808080);

    pub const BLACK: Color = Self::from_rgb(0x131a1c);
    pub const RED: Color = Self::from_rgb(0xe74c4c);
    pub const GREEN: Color = Self::from_rgb(0x6bb05d);
    pub const YELLOW: Color = Self::from_rgb(0xe59e67);
    pub const BLUE: Color = Self::from_rgb(0x5b98a9);
    pub const PURPLE: Color = Self::from_rgb(0xb185db);
    pub const CYAN: Color = Self::from_rgb(0x51a39f);
    pub const WHITE: Color = Self::from_rgb(0xc4c4c4);

    pub const BRIGHT_BLACK: Color = Self::from_rgb(0x343636);
    pub const BRIGHT_RED: Color = Self::from_rgb(0xc26f6f);
    pub const BRIGHT_GREEN: Color = Self::from_rgb(0x8dc776);
    pub const BRIGHT_YELLOW: Color = Self::from_rgb(0xe7ac7e);
    pub const BRIGHT_BLUE: Color = Self::from_rgb(0x7ab3c3);
    pub const BRIGHT_PURPLE: Color = Self::from_rgb(0xbb84e5);
    pub const BRIGHT_CYAN: Color = Self::from_rgb(0x6db0ad);
    pub const BRIGHT_WHITE: Color = Self::from_rgb(0xcccccc);
}

bitflags! {
    struct ANSIFormatFlags: u8 {
        const BOLD = 1 << 0;
        const ITALIC = 1 << 1;
    }
}

pub struct ANSIFormatter<'a, T> {
    data: &'a T,
    flags: ANSIFormatFlags,
    color: Option<Color>,
}

impl<'a, T> ANSIFormatter<'a, T> {
    pub fn new(data: &'a T) -> ANSIFormatter<'a, T> {
        return ANSIFormatter {
            data,
            flags: ANSIFormatFlags::empty(),
            color: None,
        };
    }

    pub fn color(&mut self, color: Color) -> &mut Self {
        self.color = Some(color);
        self
    }

    pub fn bold(&mut self) -> &mut Self {
        self.flags.insert(ANSIFormatFlags::BOLD);
        self
    }

    pub fn italic(&mut self) -> &mut Self {
        self.flags.insert(ANSIFormatFlags::ITALIC);
        self
    }
}

macro impl_for($trait:ident) {
    impl<'a, T: $trait> $trait for ANSIFormatter<'a, T> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result {
            if self.flags.contains(ANSIFormatFlags::BOLD) {
                f.write_str("\x1b[1m")?;
            }

            if self.flags.contains(ANSIFormatFlags::ITALIC) {
                f.write_str("\x1b[3m")?;
            }

            if let Some(color) = &self.color {
                write!(f, "\x1b[38;2;{};{};{}m", color.0, color.1, color.2)?;
            }

            self.data.fmt(f)?;

            f.write_str("\x1b[0m")
        }
    }
}

impl_for!(Display);
impl_for!(Debug);
impl_for!(Octal);
impl_for!(LowerHex);
impl_for!(UpperHex);
impl_for!(Pointer);
impl_for!(Binary);
impl_for!(LowerExp);
impl_for!(UpperExp);
