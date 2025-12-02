use bitflags::bitflags;
use log::Level;
use proc_macros::CmdlineParsable;

use crate::cmdline::{CmdlineParsable, ParsableFlags};

bitflags! {
    #[derive(Clone, Copy)]
    pub struct LogSource: u8 {
        const INIT = 1 << 0;
        const INIT_LIMINE = 1 << 1;
        const INIT_SMP = 1 << 2;
        const INIT_MEMMAP = 1 << 3;
    }
}

impl ParsableFlags for LogSource {}

#[derive(CmdlineParsable, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Error => Self::Error,
            LogLevel::Warn => Self::Warn,
            LogLevel::Info => Self::Info,
            LogLevel::Debug => Self::Debug,
            LogLevel::Trace => Self::Trace,
        }
    }
}

#[derive(CmdlineParsable, Clone, Copy)]
pub struct LogMode(pub LogLevel, pub LogSource, pub LogLevel);

#[derive(CmdlineParsable, Clone, Copy)]
pub struct SerialOptions {
    pub enable: bool,
    pub port: u16,
    pub mode: LogMode,
}

#[derive(CmdlineParsable, Clone, Copy)]
pub struct FramebufferOptions {
    pub mode: LogMode,
}

#[derive(CmdlineParsable, Clone, Copy)]
pub struct FormatOptions {
    pub level: bool,
    pub target: bool,
    pub mod_path: bool,
    pub src: bool,
}

#[derive(CmdlineParsable, Clone, Copy)]
pub struct LogOptions {
    pub serial: SerialOptions,
    pub fb: FramebufferOptions,
    pub options: FormatOptions,
}
