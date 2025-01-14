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

fn run_thread(timeout: Instant) {
    let mut local_val = S2.load(Ordering::Relaxed);
    loop {
        //> check if the timeout is reached
        if Instant::now().duration_since(timeout) > Duration::from_secs(0) {
            println!("timeout reached in worker thread. exiting.");
            break;
        }

        //> wait until s1 advances
        while local_val == S1.load(Ordering::Relaxed) {
            if Instant::now().duration_since(timeout) > Duration::from_secs(0) {
                return;
            }
        }

        //> increment s2 once s1 changes
        local_val = S2.fetch_add(1, Ordering::SeqCst) + 1;
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "usage: {} <main_core> <worker_core> <timeout_seconds>",
            args[0]
        );
        process::exit(-1);
    }

    let main_core: usize = args[1].parse().expect("invalid main_core number");
    let worker_core: usize = args[2].parse().expect("invalid worker_core number");
    let timeout_secs: u64 = args[3].parse().expect("invalid timeout value");

    //> calculate timeout as an instant in the future
    let timeout = Instant::now() + Duration::from_secs(timeout_secs);

    unsafe {
        set_main_thread_affinity(main_core);
    }

    //> spawn the worker thread and pass the timeout
    let handle = thread::spawn(move || {
        run_thread(timeout);
    });

    //> get the raw pthread id for affinity setting
    let thread_id = handle.as_pthread_t();
    unsafe {
        set_pthread_affinity(thread_id, worker_core);
    }

    let start = Instant::now();
    let mut local_val = S1.load(Ordering::Relaxed);

    //> main loop: wait for s2 to match s1, then increment s1
    while S1.load(Ordering::Relaxed) < ITERATIONS {
        if Instant::now() >= timeout {
            println!("timeout reached in main thread. exiting.");
            break;
        }

        //> busy spin until s2 matches local_val
        while S2.load(Ordering::Relaxed) != local_val {
            if Instant::now() >= timeout {
                println!("timeout reached during busy spin in main thread. exiting.");
                return;
            }
        }

        local_val = S1.fetch_add(1, Ordering::SeqCst) + 1;
    }

    //> compute final metrics
    let duration = start.elapsed();
    let nanos = duration.as_nanos();

    //> how many iterations actually completed
    let final_s1 = S1.load(Ordering::SeqCst);
    let final_s2 = S2.load(Ordering::SeqCst);

    //> each iteration has 2 ops (increment s1 + increment s2)
    let actual_ops = final_s1 * 2;

    if actual_ops > 0 {
        let ns_per_op = nanos / actual_ops as u128;
        let ops_sec = (actual_ops as u128 * 1_000_000_000) / nanos;

        println!("duration = {} ns", nanos);
        println!("ns per op = {}", ns_per_op);
        println!("ops/sec = {}", ops_sec);
    } else {
        println!("no operations completed before timeout");
    }

    println!("s1 = {}, s2 = {}", final_s1, final_s2);
}
