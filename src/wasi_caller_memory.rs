/// This is an internal structure used to acquire a handle on the caller's
/// wasm memory buffer.
///
/// This exploits how we can implement `WasmTy` for ourselves locally even
/// though crates in general should not be doing that. This is a crate in
/// the wasmtime project, however, so we should be able to keep up with our own
/// changes.
///
/// In general this type is wildly unsafe. We need to update the wasi crates to
/// probably work with more `wasmtime`-like APIs to grip with the unsafety
/// around dealing with caller memory.
pub struct WasiCallerMemory {
    base: *mut u8,
    len: usize,
}

impl wasmtime::WasmTy for WasiCallerMemory {
    type Abi = ();

    fn push(_dst: &mut Vec<wasmtime::ValType>) {}

    fn matches(_tys: impl Iterator<Item = wasmtime::ValType>) -> anyhow::Result<()> {
        Ok(())
    }

    fn from_abi(vmctx: *mut wasmtime_runtime::VMContext, _abi: ()) -> Self {
        unsafe {
            match wasmtime_runtime::InstanceHandle::from_vmctx(vmctx).lookup("memory") {
                Some(wasmtime_runtime::Export::Memory {
                    definition,
                    vmctx: _,
                    memory: _,
                }) => WasiCallerMemory {
                    base: (*definition).base,
                    len: (*definition).current_length,
                },
                _ => WasiCallerMemory {
                    base: std::ptr::null_mut(),
                    len: 0,
                },
            }
        }
    }

    fn into_abi(self) {}
}

impl WasiCallerMemory {
    pub unsafe fn get(&self) -> Option<&mut [u8]> {
        if self.base.is_null() {
            None
        } else {
            Some(std::slice::from_raw_parts_mut(self.base, self.len))
        }
    }
}
