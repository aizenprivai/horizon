//! Device Removed Extended Data (DRED) for UWP builds (Horizon's addition, for the Xbox's Dev Mode).
//!
//! On a console a lost device is the only failure there is to see: no debug layer, no PIX at
//! hand. DRED, turned on before the device is created, keeps the GPU's progress through each
//! command list (auto-breadcrumbs) and the allocation a page fault hit; [`report`] writes them to
//! the log with the reason `GetDeviceRemovedReason` gives, once.

use alloc::{format, string::String, vec::Vec};
use core::sync::atomic::{AtomicBool, Ordering};

use windows::{
    core::Interface as _,
    Win32::Graphics::Direct3D12::{
        ID3D12Device, ID3D12DeviceRemovedExtendedData, ID3D12DeviceRemovedExtendedDataSettings,
        D3D12_AUTO_BREADCRUMB_NODE, D3D12_DRED_ALLOCATION_NODE, D3D12_DRED_ENABLEMENT_FORCED_ON,
    },
};

/// Turns on breadcrumbs and page-fault tracking for the devices created after it.
pub(super) fn enable(lib: &super::D3D12Lib) {
    match lib.debug_interface_of::<ID3D12DeviceRemovedExtendedDataSettings>() {
        Some(settings) => {
            // SAFETY: plain setters on a live settings object, before device creation.
            unsafe {
                settings.SetAutoBreadcrumbsEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
                settings.SetPageFaultEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON);
            }
            log::info!("DX12: DRED on (breadcrumbs and page faults kept for a lost device)");
        }
        None => log::info!("DX12: DRED unavailable"),
    }
}

static REPORTED: AtomicBool = AtomicBool::new(false);

/// Logs why `device` was removed and what DRED kept, the first time a call sees it lost.
pub(super) fn report(device: &ID3D12Device) {
    if REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    // SAFETY: a query on a live (if removed) device.
    let reason = match unsafe { device.GetDeviceRemovedReason() } {
        Ok(()) => String::from("S_OK (not removed?)"),
        Err(e) => format!(
            "{:#010x} {}",
            e.code().0 as u32,
            removed_name(e.code().0 as u32)
        ),
    };
    log::error!("DX12: device removed: {reason}");
    let Ok(dred) = device.cast::<ID3D12DeviceRemovedExtendedData>() else {
        log::error!("DX12: no DRED data on this device");
        return;
    };
    // SAFETY: DRED's outputs point at lists the runtime keeps alive with the device.
    unsafe {
        match dred.GetPageFaultAllocationOutput() {
            Ok(fault) if fault.PageFaultVA != 0 => {
                log::error!(
                    "DX12: page fault at GPU address {:#x}; allocations there: {}; freed there: {}",
                    fault.PageFaultVA,
                    allocations(fault.pHeadExistingAllocationNode),
                    allocations(fault.pHeadRecentFreedAllocationNode),
                );
            }
            Ok(_) => log::error!("DX12: no page fault recorded"),
            Err(e) => log::error!("DX12: page fault data unavailable: {e}"),
        }
        match dred.GetAutoBreadcrumbsOutput() {
            Ok(out) => breadcrumbs(out.pHeadAutoBreadcrumbNode),
            Err(e) => log::error!("DX12: breadcrumbs unavailable: {e}"),
        }
    }
}

/// The command lists the GPU had not finished, with the operations around where it stopped.
unsafe fn breadcrumbs(mut node: *const D3D12_AUTO_BREADCRUMB_NODE) {
    let mut unfinished = 0;
    let mut total = 0;
    while let Some(n) = unsafe { node.as_ref() } {
        total += 1;
        let count = n.BreadcrumbCount;
        let done = unsafe { n.pLastBreadcrumbValue.as_ref() }
            .copied()
            .unwrap_or(0);
        if done < count && unfinished < 6 {
            unfinished += 1;
            let ops: Vec<String> = (done.saturating_sub(3)..(done + 4).min(count))
                .map(|i| {
                    let op = unsafe { *n.pCommandHistory.add(i as usize) }.0;
                    let mark = if i == done { ">>" } else { "" };
                    format!("{mark}{}", op_name(op))
                })
                .collect();
            log::error!(
                "DX12: unfinished command list {:?} on queue {:?}: {done} of {count} operations done; around there: {}",
                unsafe { name(n.pCommandListDebugNameA) },
                unsafe { name(n.pCommandQueueDebugNameA) },
                ops.join(", "),
            );
        }
        node = n.pNext;
    }
    log::error!("DX12: {total} command lists in the breadcrumbs, {unfinished} shown unfinished");
}

unsafe fn allocations(mut node: *const D3D12_DRED_ALLOCATION_NODE) -> String {
    let mut names = Vec::new();
    while let Some(n) = unsafe { node.as_ref() } {
        if names.len() < 8 {
            names.push(format!(
                "{:?} (type {})",
                unsafe { name(n.ObjectNameA) },
                n.AllocationType.0
            ));
        }
        node = n.pNext;
    }
    if names.is_empty() {
        String::from("none")
    } else {
        names.join(", ")
    }
}

unsafe fn name(text: *const u8) -> String {
    if text.is_null() {
        return String::from("(unnamed)");
    }
    unsafe { core::ffi::CStr::from_ptr(text.cast()) }
        .to_string_lossy()
        .into()
}

fn removed_name(code: u32) -> &'static str {
    match code {
        0x887A_0006 => "DXGI_ERROR_DEVICE_HUNG (the GPU took too long: a timeout)",
        0x887A_0005 => "DXGI_ERROR_DEVICE_REMOVED",
        0x887A_0007 => "DXGI_ERROR_DEVICE_RESET (a bad command)",
        0x887A_0020 => "DXGI_ERROR_DRIVER_INTERNAL_ERROR",
        0x887A_0001 => "DXGI_ERROR_INVALID_CALL",
        0x8007_000E => "E_OUTOFMEMORY",
        _ => "",
    }
}

fn op_name(op: i32) -> String {
    const NAMES: [&str; 21] = [
        "SetMarker",
        "BeginEvent",
        "EndEvent",
        "DrawInstanced",
        "DrawIndexedInstanced",
        "ExecuteIndirect",
        "Dispatch",
        "CopyBufferRegion",
        "CopyTextureRegion",
        "CopyResource",
        "CopyTiles",
        "ResolveSubresource",
        "ClearRenderTargetView",
        "ClearUnorderedAccessView",
        "ClearDepthStencilView",
        "ResourceBarrier",
        "ExecuteBundle",
        "Present",
        "ResolveQueryData",
        "BeginSubmission",
        "EndSubmission",
    ];
    NAMES
        .get(op as usize)
        .map_or_else(|| format!("op{op}"), |name| String::from(*name))
}
