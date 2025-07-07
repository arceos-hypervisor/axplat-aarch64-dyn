use core::error::Error;

use aarch64_cpu::asm::wfi;
use alloc::{boxed::Box, format};
use axplat::power::PowerIf;
use log::{debug, error};
use rdrive::{PlatformDevice, module_driver, probe::OnProbeError, register::FdtInfo};
use smccc::{Hvc, Smc, psci};
use spin::Once;

struct PowerImpl;

static METHOD: Once<Method> = Once::new();

#[impl_plat_interface]
impl PowerIf for PowerImpl {
    /// Bootstraps the given CPU core with the given initial stack (in physical
    /// address).
    ///
    /// Where `cpu_id` is the logical CPU ID (0, 1, ..., N-1, N is the number of
    /// CPU cores on the platform).
    #[cfg(feature = "smp")]
    fn cpu_boot(cpu_idx: usize, stack_top_paddr: usize) {
        use aarch64_cpu_ext::cache::{CacheOp, dcache_all};
        use log::info;

        let cpu_id = crate::smp::cpu_idx_to_id(cpu_idx);
        let entry = crate::smp::secondary_entry_phys_addr();
        info!(
            "booting CPU{cpu_idx} id {cpu_id:#x} with entry {entry:#x} and stack top {stack_top_paddr:#x}",
        );
        dcache_all(CacheOp::CleanAndInvalidate);
        cpu_on(cpu_id as _, entry.as_usize() as _, stack_top_paddr as _).unwrap();
    }

    /// Shutdown the whole system.
    fn system_off() -> ! {
        if let Err(e) = match METHOD.wait() {
            Method::Smc => psci::system_off::<Smc>(),
            Method::Hvc => psci::system_off::<Hvc>(),
        } {
            error!("shutdown failed: {e}");
        }
        loop {
            wfi();
        }
    }
}

module_driver!(
    name: "ARM PSCI",
    level: ProbeLevel::PreKernel,
    priority: ProbePriority::DEFAULT,
    probe_kinds: &[
        ProbeKind::Fdt {
            compatibles: &["arm,psci-1.0","arm,psci-0.2","arm,psci"],
            on_probe: probe
        }
    ],
);

#[derive(Debug, Clone, Copy)]
enum Method {
    Smc,
    Hvc,
}

impl TryFrom<&str> for Method {
    type Error = Box<dyn Error>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "smc" => Ok(Method::Smc),
            "hvc" => Ok(Method::Hvc),
            _ => Err(format!("method [{value}] not support").into()),
        }
    }
}

fn probe(fdt: FdtInfo<'_>, _dev: PlatformDevice) -> Result<(), OnProbeError> {
    let method = fdt
        .node
        .find_property("method")
        .ok_or(OnProbeError::Other("fdt no method property".into()))?
        .str();
    let method = Method::try_from(method)?;
    METHOD.call_once(|| method);
    debug!("PCSI [{method:?}]");
    Ok(())
}

fn cpu_on(cpu_id: u64, entry: u64, stack_top: u64) -> Result<(), alloc::boxed::Box<dyn Error>> {
    let method = METHOD.wait();
    match method {
        Method::Smc => psci::cpu_on::<Smc>(cpu_id, entry, stack_top)?,
        Method::Hvc => psci::cpu_on::<Hvc>(cpu_id, entry, stack_top)?,
    };
    Ok(())
}
