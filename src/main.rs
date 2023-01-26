#[cfg(target_os = "hermit")]
use hermit_sys as _;

mod channel_messages;
mod cli;
mod host_state;
mod http_handler;
mod http_server;
mod keyvalue;
mod settings;

use anyhow::Result;
use host_state::HostState;
use log::debug;
use wasmi::*;

use crate::http_server::start_http_server_loop;

fn main() -> Result<()> {
    let settings = match cli::parse_cli()? {
        Some(s) => s,
        None => return Ok(()),
    };

    if settings.verbose {
        simple_logger::init_with_level(log::Level::Trace)?;
    } else {
        simple_logger::init_with_level(log::Level::Warn)?;
    }

    debug!("Settings: {:?}", settings);

    let engine = Engine::default();

    // TODO: dirty workaround to get the WebAssembly module into
    // the VM. Find a way to inject the `.wasm` file into the VM
    // using another way
    let module_bytes = include_bytes!("../wasm/http-server-demo.wasm");
    let module = Module::new(&engine, &mut &module_bytes[..])?;

    let host_state = HostState::new(&settings)?;

    let mut store = wasmi::Store::new(&engine, host_state);

    let mut linker = wasmi::Linker::<HostState>::new();
    HostState::add_to_linker(&mut linker, &mut store).expect("cannot add host functions to linker");

    let instance = linker
        .instantiate(&mut store, &module)
        .expect("cannot instantiate module")
        .start(&mut store)
        .expect("cannot invoke _start function");

    let main_func = instance
        .get_typed_func::<(i32, i32), i32>(&mut store, "main")
        .expect("canont find 'main' exported function");
    main_func
        .call(&mut store, (0, 0))
        .expect("something went wrong while calling 'main' function");

    let host_state = store.data();
    if let Some(http_inner_server) = host_state.server() {
        // This starts a loop
        start_http_server_loop(&http_inner_server, &settings, &instance, &mut store)?;
    }

    println!("Leaving");

    Ok(())
}
