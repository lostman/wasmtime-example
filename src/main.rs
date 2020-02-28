use anyhow::{anyhow, Context};
use structopt::StructOpt;
use wasmtime::{Config, Engine, Func, Instance, Module, Store};

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

fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let binary = read_wasm_source(&opt.source).context("failed to read Wasm binary")?;
    let config = Config::default();
    let engine = Engine::new(&config);
    let store = Store::new(&engine);
    let module = Module::new(&store, &binary).context("failed to instantiate module")?;

    let debug_print = Func::wrap3(
        &store,
        |heap: wasmtime_example::wasi_caller_memory::WasiCallerMemory, src: i32, length: i32| {
            let src = src as usize;
            let length = length as usize;
            let heap = unsafe { heap.get().unwrap() };
            eprintln!("debug.print: {}", unsafe {
                str::from_utf8_unchecked(&heap[src..src + length])
            });
        },
    );

    let externs = vec![wasmtime::Extern::Func(debug_print)];

    let instance = Instance::new(&module, &externs).context("failed to instantiate instance")?;

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
        .get_export("memory")
        .expect("memory")
        .memory()
        .expect("memory");

    unsafe {
        println!(
            "Wasmtime memory[..16] = {:?}",
            &wasmtime_memory.data_unchecked()[..16]
        );
    }

    let results = instance
        .get_export(&opt.export)
        .ok_or_else(|| anyhow!("export not found: {}", &opt.export))?
        .func()
        .ok_or_else(|| anyhow!("export is not a function: {}", &opt.export))?
        .call(&[]);
    println!("Invoked {}, result: {:?}", &opt.export, results);
    Ok(())
}
