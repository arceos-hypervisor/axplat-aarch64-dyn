use alloc::{format, string::String};
pub use arm_gic_driver::v3::Gic;
use arm_gic_driver::v3::*;
use lazyinit::LazyInit;
use log::*;
use spin::Mutex;

use crate::irq;

use super::IRQ_HANDLER_TABLE;

pub static CPU_IF: LazyInit<Mutex<CpuInterface>> = LazyInit::new();
pub static TRAP: LazyInit<TrapOp> = LazyInit::new();

fn use_gicd(f: impl FnOnce(&mut Gic)) {
    let mut gic = irq::get_gicd().lock().unwrap();
    f(gic.typed_mut::<Gic>().expect("GICD is not initialized"));
}

pub fn init_current_cpu() {
    let mut cpu = CPU_IF.lock();
    cpu.init_current_cpu().unwrap();
    #[cfg(feature = "hv")]
    cpu.set_eoi_mode(true);
    #[cfg(not(feature = "hv"))]
    cpu.set_eoi_mode(false);
}

pub fn handle(_unused: usize) {
    let ack = TRAP.ack1();
    let irq_num = ack.to_u32();
    // info!("IRQ {}", irq_num);
    if !IRQ_HANDLER_TABLE.handle(irq_num as _) {
        warn!("Unhandled IRQ {irq_num}");
    }

    TRAP.eoi1(ack);
    if TRAP.eoi_mode() {
        TRAP.dir(ack);
    }
}

pub(crate) fn set_enable(irq_raw: usize, trigger: Option<Trigger>, enabled: bool) {
    let c = CPU_IF.lock();
    debug!(
        "IRQ({:#x}) set enable: {}, {}",
        irq_raw,
        enabled,
        match trigger {
            Some(t) => format!("trigger: {t:?}"),
            None => String::new(),
        }
    );
    let id = unsafe { IntId::raw(irq_raw as _) };
    if id.is_private() {
        c.set_irq_enable(id, enabled);

        if let Some(t) = trigger {
            c.set_cfg(id, t);
        }
    } else {
        use_gicd(|gic| {
            gic.set_irq_enable(id, enabled);
            gic.set_target_cpu(id, Some(Affinity::current()));
            if let Some(t) = trigger {
                gic.set_cfg(id, t);
            }
        });
    }
    debug!("IRQ({irq_raw:#x}) set enable done");
}
