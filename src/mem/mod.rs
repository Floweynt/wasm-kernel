mod init;
mod requests;
mod types;
mod pmm;
mod malloc;
mod vpa;

pub use init::*;
pub use requests::*;
pub use types::*;
pub use pmm::*;
pub use malloc::*;

pub trait PageFrameAllocator {
    fn allocate_single_page(&mut self) -> PageFrameNumber;
}
