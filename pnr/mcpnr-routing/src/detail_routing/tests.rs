use log::info;

use super::*;

fn init(size_x: u32, size_y: u32, size_z: u32) -> DetailRouter {
    let _ = env_logger::builder().is_test(true).try_init();
    DetailRouter::new(size_x, size_y, size_z)
}
