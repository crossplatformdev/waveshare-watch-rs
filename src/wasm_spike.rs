use core::mem::size_of;
use core::fmt;

use embassy_time::Instant;
use wasmi::errors::LinkerError;
use wasmi::{Caller, CompilationMode, Config, Engine, Linker, Module, Store};

use crate::wasm_spike_payload::WASM_SPIKE_MODULE;

const HOST_CALL_ITERATIONS: u32 = 128;
const FUEL_BUDGET: u64 = 20_000;
const FUEL_LOOP_INPUT: i32 = 64;

#[derive(Clone, Copy, Debug, Default)]
pub struct WasmSpikeMetrics {
    pub module_bytes: usize,
    pub engine_bytes: usize,
    pub linker_bytes: usize,
    pub store_bytes: usize,
    pub compile_us: u64,
    pub instantiate_us: u64,
    pub compat_version: i32,
    pub host_call_iterations: u32,
    pub host_call_us: u64,
    pub host_call_count: u32,
    pub host_call_checksum: i32,
    pub fuel_before: u64,
    pub fuel_after: u64,
    pub fuel_loop_result: i32,
}

#[derive(Debug)]
pub enum WasmSpikeError {
    Wasmi(wasmi::Error),
    Linker(LinkerError),
    FuelSetup,
    FuelRead,
}

impl From<wasmi::Error> for WasmSpikeError {
    fn from(value: wasmi::Error) -> Self {
        Self::Wasmi(value)
    }
}

impl fmt::Display for WasmSpikeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wasmi(err) => write!(f, "wasmi error: {err}"),
            Self::Linker(err) => write!(f, "linker error: {err}"),
            Self::FuelSetup => f.write_str("fuel metering could not be enabled"),
            Self::FuelRead => f.write_str("fuel metering could not be read"),
        }
    }
}

#[derive(Default)]
struct SpikeHostState {
    host_call_count: u32,
    checksum: i32,
}

pub fn run_boot_probe() -> Result<WasmSpikeMetrics, WasmSpikeError> {
    let mut config = Config::default();
    config.consume_fuel(true);
    config.compilation_mode(CompilationMode::Eager);

    let engine = Engine::new(&config);
    let compile_start = Instant::now();
    let module = Module::new(&engine, WASM_SPIKE_MODULE)?;
    let compile_us = compile_start.elapsed().as_micros();

    let mut store = Store::new(&engine, SpikeHostState::default());
    let mut linker = Linker::new(&engine);
    linker.func_wrap(
        "env",
        "draw_pixel",
        |mut caller: Caller<'_, SpikeHostState>, x: i32, y: i32, color: i32| -> i32 {
            let result = x ^ y ^ color;
            let state = caller.data_mut();
            state.host_call_count += 1;
            state.checksum ^= result;
            result
        },
    )
    .map_err(WasmSpikeError::Linker)?;

    let instantiate_start = Instant::now();
    let instance = linker.instantiate_and_start(&mut store, &module)?;
    let compat = instance.get_typed_func::<(), i32>(&store, "compat")?;
    let host_roundtrip = instance.get_typed_func::<i32, i32>(&store, "host_roundtrip")?;
    let fuel_loop = instance.get_typed_func::<i32, i32>(&store, "fuel_loop")?;
    let instantiate_us = instantiate_start.elapsed().as_micros();

    store
        .set_fuel(FUEL_BUDGET * 4)
        .map_err(|_| WasmSpikeError::FuelSetup)?;
    let compat_version = compat.call(&mut store, ())?;

    let host_start = Instant::now();
    for i in 0..HOST_CALL_ITERATIONS {
        let _ = host_roundtrip.call(&mut store, i as i32)?;
    }
    let host_call_us = host_start.elapsed().as_micros();

    store
        .set_fuel(FUEL_BUDGET)
        .map_err(|_| WasmSpikeError::FuelSetup)?;
    let fuel_before = store.get_fuel().map_err(|_| WasmSpikeError::FuelRead)?;
    let fuel_loop_result = fuel_loop.call(&mut store, FUEL_LOOP_INPUT)?;
    let fuel_after = store.get_fuel().map_err(|_| WasmSpikeError::FuelRead)?;

    let host_state = store.data();
    Ok(WasmSpikeMetrics {
        module_bytes: WASM_SPIKE_MODULE.len(),
        engine_bytes: size_of::<Engine>(),
        linker_bytes: size_of::<Linker<SpikeHostState>>(),
        store_bytes: size_of::<Store<SpikeHostState>>(),
        compile_us,
        instantiate_us,
        compat_version,
        host_call_iterations: HOST_CALL_ITERATIONS,
        host_call_us,
        host_call_count: host_state.host_call_count,
        host_call_checksum: host_state.checksum,
        fuel_before,
        fuel_after,
        fuel_loop_result,
    })
}
