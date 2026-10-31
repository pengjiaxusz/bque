pub mod checker;
pub mod updater;

pub use checker::{check_and_print_notice, clean_old_binaries, maybe_spawn_background_check};
pub use updater::handle_update;
