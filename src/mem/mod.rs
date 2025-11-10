mod init;
mod requests;
mod types;

pub use init::*;
pub use requests::*;
pub use types::*;

// limine requests

// some memory management logic

pub trait PageFrameAllocator {
    fn allocate_single_page(&mut self) -> PageFrameNumber;
}
