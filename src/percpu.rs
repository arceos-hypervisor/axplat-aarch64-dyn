use core::{cell::UnsafeCell, ops::Deref, sync::atomic::AtomicBool};

use alloc::collections::btree_map::BTreeMap;
use log::debug;
use somehal::mem::cpu_id_list;

use crate::smp::current_cpu;

pub struct PerCpu<T> {
    inner: UnsafeCell<BTreeMap<usize, T>>,
    is_init: AtomicBool,
}

unsafe impl<T: Send> Send for PerCpu<T> {}
unsafe impl<T: Sync> Sync for PerCpu<T> {}

impl<T> PerCpu<T> {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(BTreeMap::new()),
            is_init: AtomicBool::new(false),
        }
    }

    pub unsafe fn cpu0_init<F>(&self, v: F)
    where
        F: Fn() -> T,
    {
        // 初始化所有CPU的数据
        for id in cpu_id_list() {
            unsafe {
                debug!("PerCpu init for CPU {}", id);
                (*self.inner.get()).insert(id, v());
            }
        }

        self.is_init
            .store(true, core::sync::atomic::Ordering::Release);
    }
}

impl<T> Deref for PerCpu<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        if !self.is_init.load(core::sync::atomic::Ordering::Acquire) {
            panic!("PerCpu is not initialized");
        }
        let cpu_id = current_cpu();
        unsafe {
            (*self.inner.get())
                .get(&cpu_id)
                .unwrap_or_else(|| panic!("PerCpu is not initialized for CPU {}", cpu_id))
        }
    }
}
