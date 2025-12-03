mod init;
mod malloc;
mod pmm;
mod requests;
mod types;
pub mod vpa;

pub use init::*;
pub use pmm::*;
pub use requests::*;
use spin::Once;
pub use types::*;

use crate::{arch::paging::PageTableSet, mp::core_local};

core_local! {
    pub LOCAL_PAGE_TABLE: Once<PageTableSet> = Once::new();
}
