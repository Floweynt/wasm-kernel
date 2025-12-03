use core::ptr::slice_from_raw_parts;

use crate::cmdline::{CmdlineLexer, CmdlineParsable};
use limine::request::ModuleRequest;
use log::warn;
use proc_macros::CmdlineParsable;
pub mod symbols;

// the main command line types
#[derive(CmdlineParsable)]
enum ModuleCmdline {
    InternalNull,
    Symbols,
}

#[used]
#[unsafe(link_section = ".limine_requests")]
static MODULE_REQUEST: ModuleRequest = ModuleRequest::new();

pub fn load_modules_early() {
    if let Some(res) = MODULE_REQUEST.get_response() {
        for module in res.modules() {
            let path = match module.path().to_str() {
                Ok(x) => x,
                Err(e) => {
                    warn!("failed to decode module path to utf8: {e}");
                    "<unk>"
                }
            };

            let cmdline_str = match module.string().to_str() {
                Ok(x) => x,
                Err(e) => {
                    warn!("mod({path}): failed to decode module cmdline to utf8: {e}");
                    continue;
                }
            };

            let mut cmdline = ModuleCmdline::InternalNull;

            match CmdlineLexer::parse(cmdline_str, &mut cmdline) {
                Ok(_) => {}
                Err(e) => {
                    warn!("mod({path}): failed to parse module cmdline `{cmdline_str}`: {e}");
                    continue;
                }
            };

            match cmdline {
                ModuleCmdline::InternalNull => {
                    warn!("mod({path}): do not use `internalnull` module type");
                    continue;
                }
                ModuleCmdline::Symbols => {
                    let Some(syms) = symbols::parse(unsafe {
                        &*slice_from_raw_parts(module.addr(), module.size() as usize)
                    }) else {
                        warn!("mod({path}): failed to parse symbols");
                        continue;
                    };

                    if !symbols::try_init(syms) {
                        warn!("mod({path}): cannot load multiple global symbol modules");
                    }
                }
            }
        }
    }
}
