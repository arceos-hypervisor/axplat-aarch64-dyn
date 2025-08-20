use somehal::boot_info;
use rdrive::{Platform, init, probe_pre_kernel};

pub fn setup() {
    let fdt = boot_info().fdt.expect("FDT must be present");

    init(Platform::Fdt { addr: fdt }).unwrap();

    probe_pre_kernel().unwrap();
}
