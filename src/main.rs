use anyhow::{anyhow, bail, Context};
use structopt::StructOpt;
use wasmtime::{Config, Engine, HostRef, Instance, Module, Store};
use wasmtime_example::system_api::{create_instance, SystemApi};

use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    str,
};

#[derive(StructOpt)]
struct Opt {
    source: PathBuf,
    export: String,
}

fn read_wasm_source(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut file = File::open(path).context("failed to open file")?;
    let mut contents = vec![];
    file.read_to_end(&mut contents)
        .context("failed to read file contents")?;
    Ok(wabt::wat2wasm(&contents)
        .context("failed to translate Wasm text source to Wasm binary format")?)
}

struct SystemApiImpl;

impl SystemApi for SystemApiImpl {
    fn debug_print(&self, heap: &[u8], src: u32, length: u32) {
        let src = src as usize;
        let length = length as usize;
        eprintln!("debug.print: {}", unsafe {
            str::from_utf8_unchecked(&heap[src..src + length])
        });
    }
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let binary = read_wasm_source(&opt.source).context("failed to read Wasm binary")?;
    let config = Config::default();
    let engine = Engine::new(&config);
    let store = HostRef::new(Store::new(&engine));
    let mut system_api_impl = SystemApiImpl;
    let system_api_instance = create_instance(&store, &mut system_api_impl)
        .context("failed to create system API instance")?;
    let module = Module::new(&store, &binary).context("failed to instantiate module")?;
    let mut externs = vec![];

    for import in module.imports().iter() {
        let export_name = format!("{}_{}", import.module(), import.name());
        match system_api_instance.find_export_by_name(&export_name) {
            Some(export) => externs.push(export.clone()),
            _ => bail!("export not found: {}", export_name),
        }
    }

    let instance = Instance::new(&store, &HostRef::new(module), &externs)
        .context("failed to instantiate instance")?;

    for export in instance.exports() {
        match export {
            wasmtime::Extern::Func(_) => {
                println!("Func");
            }
            wasmtime::Extern::Global(_) => {
                println!("Global");
            }
            wasmtime::Extern::Table(_) => {
                println!("Table");
            }
            wasmtime::Extern::Memory(_) => {
                println!("Memory");
            }
        }
    }

    let wasmtime_memory = instance
        .get_wasmtime_memory()
        .ok_or_else(|| anyhow!("Wasmtime memory not found"))?;

    match wasmtime_memory {
        wasmtime_runtime::Export::Memory {
            definition,
            vmctx,
            memory,
        } => unsafe {
            let base = (*definition).base;
            let current_length = (*definition).current_length;
            let slice = std::slice::from_raw_parts(base, current_length);
            println!("Wasmtime memory[..16] = {:?}", &slice[..16]);
        },
        _ => {
            panic!("Wasmtime memory is not a memory");
        }
    }

    let results = instance
        .find_export_by_name(&opt.export)
        .ok_or_else(|| anyhow!("export not found: {}", &opt.export))?
        .func()
        .ok_or_else(|| anyhow!("export is not a function: {}", &opt.export))?
        .borrow()
        .call(&[]);
    println!("Invoked {}, result: {:?}", &opt.export, results);
    Ok(())
}
