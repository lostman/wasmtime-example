use cranelift_codegen::ir::types::{Type, I32, I64};
use cranelift_codegen::{ir, isa};
use cranelift_entity::PrimaryMap;
use target_lexicon::HOST;
use wasmtime::{Instance, Store};
use wasmtime_environ::translate_signature;
use wasmtime_runtime::{Export, VMContext};
use wasmtime_runtime::{Imports, InstanceHandle, InstantiationError, VMFunctionBody};

use std::sync::Arc;

pub trait AbiRet {
    type Abi;
    fn convert(self) -> Self::Abi;
    fn codegen_tys() -> Vec<Type>;
}

pub trait AbiParam {
    type Abi;
    fn convert(arg: Self::Abi) -> Self;
    fn codegen_ty() -> Type;
}

impl AbiRet for () {
    type Abi = ();
    fn convert(self) {}
    fn codegen_tys() -> Vec<Type> {
        Vec::new()
    }
}

macro_rules! cast32 {
    ($($i:ident)*) => ($(
        impl AbiRet for $i {
            type Abi = i32;

            fn convert(self) -> Self::Abi {
                self as i32
            }

            fn codegen_tys() -> Vec<Type> { vec![I32] }
        }

        impl AbiParam for $i {
            type Abi = i32;

            fn convert(param: i32) -> Self {
                param as $i
            }

            fn codegen_ty() -> Type { I32 }
        }
    )*)
}

macro_rules! cast64 {
    ($($i:ident)*) => ($(
        impl AbiRet for $i {
            type Abi = i64;

            fn convert(self) -> Self::Abi {
                self as i64
            }

            fn codegen_tys() -> Vec<Type> { vec![I64] }
        }

        impl AbiParam for $i {
            type Abi = i64;

            fn convert(param: i64) -> Self {
                param as $i
            }

            fn codegen_ty() -> Type { I64 }
        }
    )*)
}

cast32!(i8 i16 i32 u8 u16 u32);
cast64!(i64 u64);

macro_rules! syscalls {
    ($(pub unsafe extern "C" fn $name:ident($ctx:ident: *mut VMContext, $caller_ctx:ident: *mut VMContext $(, $arg:ident: $ty:ty)*,) -> $ret:ty {
        $($body:tt)*
    })*) => ($(
        pub mod $name {
            use super::*;

            /// Returns the codegen types of all the parameters to the shim
            /// generated
            pub fn params() -> Vec<Type> {
                vec![$(<$ty as AbiParam>::codegen_ty()),*]
            }

            /// Returns the codegen types of all the results of the shim
            /// generated
            pub fn results() -> Vec<Type> {
                <$ret as AbiRet>::codegen_tys()
            }

            /// The actual function pointer to the shim for a syscall.
            ///
            /// NB: ideally we'd expose `shim` below, but it seems like there's
            /// a compiler bug which prvents that from being cast to a `usize`.
            pub static SHIM: unsafe extern "C" fn(
                *mut VMContext,
                *mut VMContext,
                $(<$ty as AbiParam>::Abi),*
            ) -> <$ret as AbiRet>::Abi = shim;

            unsafe extern "C" fn shim(
                $ctx: *mut VMContext,
                $caller_ctx: *mut VMContext,
                $($arg: <$ty as AbiParam>::Abi,)*
            ) -> <$ret as AbiRet>::Abi {
                let r = super::$name($ctx, $caller_ctx, $(<$ty as AbiParam>::convert($arg),)*);
                <$ret as AbiRet>::convert(r)
            }
        }

        pub unsafe extern "C" fn $name($ctx: *mut VMContext, $caller_ctx: *mut VMContext, $($arg: $ty,)*) -> $ret {
            $($body)*
        }
    )*)
}

fn get_memory(vmctx: &mut VMContext) -> &mut [u8] {
    unsafe {
        match InstanceHandle::from_vmctx(vmctx).lookup("memory") {
            Some(Export::Memory {
                definition,
                vmctx: _,
                memory: _,
            }) => std::slice::from_raw_parts_mut((*definition).base, (*definition).current_length),
            None => panic!("no export named \"memory\""),
            x => {
                panic!("export isn't a memory: {:?}", x);
            }
        }
    }
}

pub trait SystemApi {
    fn debug_print(&self, heap: &[u8], src: u32, length: u32);
}

fn get_system_api(vmctx: &VMContext) -> &dyn SystemApi {
    unsafe {
        vmctx
            .host_state()
            .downcast_ref::<*mut dyn SystemApi>()
            .expect("downcast failed")
            .as_mut()
            .expect("pointer is null")
    }
}

syscalls! {
    pub unsafe extern "C" fn debug_print(
        vmctx: *mut VMContext,
        caller_vmctx: *mut VMContext,
        src: u32,
        length: u32,
    ) -> () {
        get_system_api(&mut *vmctx)
            .debug_print(get_memory(&mut *caller_vmctx), src, length)
    }
}

pub fn create_instance(
    store: &Store,
    system_api: &mut (dyn SystemApi + 'static),
) -> Result<Instance, InstantiationError> {
    let mut finished_functions = PrimaryMap::new();
    let mut module = wasmtime_environ::Module::new();
    let call_conv = isa::CallConv::triple_default(&HOST);
    let pointer_type = ir::types::Type::triple_pointer_type(&HOST);

    macro_rules! signature {
        ($name:ident) => {{
            let sig = module.signatures.push(translate_signature(
                ir::Signature {
                    params: $name::params().into_iter().map(ir::AbiParam::new).collect(),
                    returns: $name::results()
                        .into_iter()
                        .map(ir::AbiParam::new)
                        .collect(),
                    call_conv,
                },
                pointer_type,
            ));
            let func = module.functions.push(sig);
            module.exports.insert(
                stringify!($name).to_owned(),
                wasmtime_environ::Export::Function(func),
            );
            finished_functions.push($name::SHIM as *const VMFunctionBody);
        }};
    }

    signature!(debug_print);
    let imports = Imports::none();
    let data_initializers = vec![];
    let signatures = PrimaryMap::new();
    let instance_handle = unsafe {
        InstanceHandle::new(
            Arc::new(module),
            finished_functions.into_boxed_slice(),
            imports,
            &data_initializers,
            signatures.into_boxed_slice(),
            None,
            Box::new(system_api as *mut _),
        )?
    };
    Ok(Instance::from_handle(store, instance_handle))
}
