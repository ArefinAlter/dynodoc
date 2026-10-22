use dynodoctl::net::{Net, RealNet};
use dynodoctl::runner::SystemRunner;

/// The generated argument parser is large; a debug build needs more than a Windows main thread's
/// 1 MiB (the same reason engine-cli runs on its own thread).
const STACK_BYTES: usize = 16 * 1024 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let worker = std::thread::Builder::new()
        .name("dynodoctl".into())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            dynodoctl::cli::run(
                args,
                &SystemRunner,
                &|roots| Box::new(RealNet::new(roots)) as Box<dyn Net>,
                None,
                &mut std::io::stdout(),
                &mut std::io::stderr(),
            )
        })
        .expect("start the worker thread");
    let code = worker.join().unwrap_or(1);
    std::process::exit(code);
}
