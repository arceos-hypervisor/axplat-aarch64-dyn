use aarch64_cpu_ext::cache::{CacheOp, dcache_all};
use alloc::vec::Vec;
use axplat::mem::{PhysAddr, va, virt_to_phys};
use fdt_parser::Status;
use log::{debug, info};
use pie_boot::boot_info;
use spin::Once;

use crate::{config::plat::CPU_NUM, fdt};

static CPU_ID_LIST: Once<Vec<usize>> = Once::new();
static mut PHYS_VIRT_OFFSET: usize = 0;
static mut BOOT_PT: usize = 0;

pub fn init() {
    CPU_ID_LIST.call_once(|| {
        let mut ls = Vec::new();
        let current = boot_info().cpu_id;
        ls.push(current);
        let cpu_id_ls = cpu_id_list();
        for cpu_id in cpu_id_ls {
            if cpu_id != current {
                ls.push(cpu_id);
            }
        }
        ls
    });

    debug!("CPU ID list: {:#x?}", CPU_ID_LIST.wait());

    if CPU_ID_LIST.wait().len() < CPU_NUM {
        panic!(
            "CPU count {} is less than expected `cpu_num` in `.axconfig.toml` is {}",
            CPU_ID_LIST.wait().len(),
            CPU_NUM
        );
    }

    if CPU_ID_LIST.wait().len() > CPU_NUM {
        info!(
            "CPU count {} is more than expected `cpu_num` in `.axconfig.toml` is {}",
            CPU_ID_LIST.wait().len(),
            CPU_NUM
        );
    }

    unsafe {
        let offset = boot_info().kcode_offset();
        PHYS_VIRT_OFFSET = offset;
        BOOT_PT = boot_info().pg_start as usize;
    }
}

fn cpu_id_list() -> Vec<usize> {
    let fdt = fdt();
    let nodes = fdt.find_nodes("/cpus/cpu");
    nodes
        .filter(|node| node.name().contains("cpu@"))
        .filter(|node| !matches!(node.status(), Some(Status::Disabled)))
        .map(|node| {
            let reg = node
                .reg()
                .unwrap_or_else(|| panic!("cpu {} reg not found", node.name()))
                .next()
                .expect("cpu reg 0 not found");
            reg.address as usize
        })
        .collect()
}

pub fn cpu_idx_to_id(cpu_idx: usize) -> usize {
    let cpu_id_list = CPU_ID_LIST.wait();
    if cpu_idx < cpu_id_list.len() {
        cpu_id_list[cpu_idx]
    } else {
        panic!("CPU index {} out of range", cpu_idx);
    }
}

pub fn cpu_id_to_idx(cpu_id: usize) -> usize {
    let cpu_id_list = CPU_ID_LIST.wait();
    if let Some(idx) = cpu_id_list.iter().position(|&id| id == cpu_id) {
        idx
    } else {
        panic!("CPU ID {} not found in the list", cpu_id);
    }
}

pub(crate) fn secondary_entry_phys_addr() -> PhysAddr {
    virt_to_phys(va!(_start_secondary as usize))
}

/// The earliest entry point for the secondary CPUs.
#[cfg(feature = "smp")]
#[unsafe(naked)]
#[unsafe(link_section = ".text.boot")]
unsafe extern "C" fn _start_secondary() -> ! {
    // X0 = stack pointer

    // use crate::paging::BOOT_PT;
    core::arch::naked_asm!("
        mrs     x19, mpidr_el1
        and     x19, x19, #0xffffff     // get current CPU id

        mov     sp, x0
        bl      {switch_to_el1}
        bl      {enable_fp}
        ldr     x0, {boot_pt}
        bl      {init_mmu}

        ldr     x8, {phys_virt_offset}  // load PHYS_VIRT_OFFSET global variable
        add     sp, sp, x8

        mov     x0, x19                 // call_secondary_main(cpu_id)
        ldr     x8, ={entry}
        blr     x8
        b      .",
        switch_to_el1 = sym axcpu::init::switch_to_el1,
        init_mmu = sym axcpu::init::init_mmu,
        enable_fp = sym axcpu::asm::enable_fp,
        boot_pt = sym BOOT_PT,
        phys_virt_offset = sym PHYS_VIRT_OFFSET,
        entry = sym _secondary_main,
    )
}

fn _secondary_main(cpu_id: usize) -> ! {
    dcache_all(CacheOp::CleanAndInvalidate);
    let cpu_idx = cpu_id_to_idx(cpu_id);
    axplat::call_secondary_main(cpu_idx)
}
