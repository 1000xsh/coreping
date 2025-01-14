use libc::{
    cpu_set_t, getpid, pthread_setaffinity_np, pthread_t, sched_setaffinity, CPU_SET, CPU_ZERO,
};
use std::os::unix::thread::JoinHandleExt;
use std::{
    env, process,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static ITERATIONS: u64 = 500_000_000;
static S1: AtomicU64 = AtomicU64::new(0);
static S2: AtomicU64 = AtomicU64::new(0);

unsafe fn set_main_thread_affinity(core_id: usize) {
    let pid = getpid();
    let mut cpu_set: cpu_set_t = std::mem::zeroed();
    CPU_ZERO(&mut cpu_set);
    CPU_SET(core_id, &mut cpu_set);

    let ret = sched_setaffinity(
        pid,
        std::mem::size_of::<cpu_set_t>(),
        &cpu_set as *const cpu_set_t,
    );
    if ret != 0 {
        eprintln!("failed to set affinity of main thread to cpu {core_id}: error code {ret}");
        process::exit(1);
    }
}

//> set the cpu affinity for the given pthread_t.
unsafe fn set_pthread_affinity(thread: pthread_t, core_id: usize) {
    let mut cpu_set: cpu_set_t = std::mem::zeroed();
    CPU_ZERO(&mut cpu_set);
    CPU_SET(core_id, &mut cpu_set);

    let ret = pthread_setaffinity_np(
        thread,
        std::mem::size_of::<cpu_set_t>(),
        &cpu_set as *const cpu_set_t,
    );
    if ret != 0 {
        eprintln!("failed to set affinity of child thread to cpu {core_id}: error code {ret}");
        process::exit(1);
    }
}

//> spawned thread runs:
//> busy-spins until s1 is incremented, then increments S2, etc.
fn run_thread() {
    let mut local_val = S2.load(Ordering::Relaxed);
    loop {
        // wait until s1 has advanced
        while local_val == S1.load(Ordering::Relaxed) {
            // busy spin
        }
        // once S1 has changed, increment S2
        local_val = S2.fetch_add(1, Ordering::SeqCst) + 1;
    }
}

fn main() {
    // expect 2 arguments: main_core and worker_core
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <main_core> <worker_core>", args[0]);
        process::exit(-1);
    }

    let main_core: usize = args[1].parse().expect("invalid main_core number");
    let worker_core: usize = args[2].parse().expect("invalid worker_core number");

    // pin the main thread to main_core
    unsafe {
        set_main_thread_affinity(main_core);
    }

    // spwan the thread, then pin it to the worker_core
    let handle = thread::spawn(|| {
        run_thread();
    });

    // raw pthread_t for affinity:
    let thread_id = handle.as_pthread_t();

    unsafe {
        set_pthread_affinity(thread_id, worker_core);
    }

    let start = Instant::now();

    // main busy spin loop
    let mut local_val = S1.load(Ordering::Relaxed);
    while S1.load(Ordering::Relaxed) < ITERATIONS {
        // wait until s2 matches our local_val
        while S2.load(Ordering::Relaxed) != local_val {
            // busy spin
        }
        // now increment s1
        local_val = S1.fetch_add(1, Ordering::SeqCst) + 1;
    }

    // stop timing
    let duration = start.elapsed();
    let nanos = duration.as_nanos();
    let ops = ITERATIONS * 2; // s1 increment + s2 increment per iteration "round-trip"

    println!("duration = {} ns", nanos);
    println!("ns per op = {}", nanos / ops as u128);
    println!("ops/sec = {}", (ops as u128 * 1_000_000_000) / nanos);
    println!(
        "S1 = {}, S2 = {}",
        S1.load(Ordering::SeqCst),
        S2.load(Ordering::SeqCst)
    );
    // no
    // handle.join().unwrap();
}
