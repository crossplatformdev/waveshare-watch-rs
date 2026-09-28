use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use wasmi::{Caller, CompilationMode, Config, Engine, Linker, Module, Store};

#[path = "../../../src/wasm_spike_payload.rs"]
mod wasm_spike_payload;

const HOST_CALL_ITERATIONS: u32 = 5_000;
const FUEL_BUDGET: u64 = 250_000;
const FUEL_LOOP_INPUT: i32 = 1_024;

struct CountingAlloc;

static CURRENT_ALLOC: AtomicUsize = AtomicUsize::new(0);
static PEAK_ALLOC: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL_ALLOC: CountingAlloc = CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        CURRENT_ALLOC.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = System.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() {
            if new_size >= layout.size() {
                record_alloc(new_size - layout.size());
            } else {
                CURRENT_ALLOC.fetch_sub(layout.size() - new_size, Ordering::SeqCst);
            }
        }
        new_ptr
    }
}

fn record_alloc(size: usize) {
    let current = CURRENT_ALLOC.fetch_add(size, Ordering::SeqCst) + size;
    let mut peak = PEAK_ALLOC.load(Ordering::SeqCst);
    while current > peak {
        match PEAK_ALLOC.compare_exchange(peak, current, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(observed) => peak = observed,
        }
    }
}

fn main() {
    let mut config = Config::default();
    config.consume_fuel(true);
    config.compilation_mode(CompilationMode::Eager);
    let engine = Engine::new(&config);

    let compile_start = Instant::now();
    let module = Module::new(&engine, wasm_spike_payload::WASM_SPIKE_MODULE)
        .expect("module should parse and validate");
    let compile_us = compile_start.elapsed().as_micros();

    let heap_before = CURRENT_ALLOC.load(Ordering::SeqCst);
    PEAK_ALLOC.store(heap_before, Ordering::SeqCst);

    let mut store = Store::new(&engine, 0u32);
    let mut linker = Linker::new(&engine);
    linker
        .func_wrap("env", "draw_pixel", |mut caller: Caller<'_, u32>, x: i32, y: i32, color: i32| -> i32 {
            *caller.data_mut() += 1;
            x ^ y ^ color
        })
        .expect("host function should link");

    let instantiate_start = Instant::now();
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instance should start");
    let compat = instance
        .get_typed_func::<(), i32>(&store, "compat")
        .expect("compat export should exist");
    let host_roundtrip = instance
        .get_typed_func::<i32, i32>(&store, "host_roundtrip")
        .expect("host_roundtrip export should exist");
    let fuel_loop = instance
        .get_typed_func::<i32, i32>(&store, "fuel_loop")
        .expect("fuel_loop export should exist");
    let instantiate_us = instantiate_start.elapsed().as_micros();

    store
        .set_fuel(FUEL_BUDGET * 4)
        .expect("fuel should be enabled");
    let compat_version = compat.call(&mut store, ()).expect("compat call should succeed");
    let host_start = Instant::now();
    let mut host_checksum = 0i64;
    for i in 0..HOST_CALL_ITERATIONS {
        host_checksum += i64::from(host_roundtrip.call(&mut store, i as i32).expect("host call should succeed"));
    }
    let host_call_us = host_start.elapsed().as_micros();

    store.set_fuel(FUEL_BUDGET).expect("fuel should be enabled");
    let fuel_before = store.get_fuel().expect("fuel should be readable");
    let fuel_loop_result = fuel_loop
        .call(&mut store, FUEL_LOOP_INPUT)
        .expect("fuel loop should finish");
    let fuel_after = store.get_fuel().expect("fuel should be readable");

    let heap_current = CURRENT_ALLOC.load(Ordering::SeqCst);
    let heap_peak = PEAK_ALLOC.load(Ordering::SeqCst);

    println!(
        "compat_version={compat_version} module_bytes={} compile_us={compile_us} instantiate_us={instantiate_us}",
        wasm_spike_payload::WASM_SPIKE_MODULE.len(),
    );
    println!(
        "heap_current_delta={} heap_peak_delta={} host_calls={} host_call_us={} host_call_us_per_call={:.3}",
        heap_current.saturating_sub(heap_before),
        heap_peak.saturating_sub(heap_before),
        *store.data(),
        host_call_us,
        host_call_us as f64 / HOST_CALL_ITERATIONS as f64,
    );
    println!(
        "fuel_before={fuel_before} fuel_after={fuel_after} fuel_consumed={} fuel_loop_result={fuel_loop_result} host_checksum={host_checksum}",
        fuel_before.saturating_sub(fuel_after),
    );
}
