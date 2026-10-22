use dynodoctl::net::{Net, RealNet};
use dynodoctl::runner::SystemRunner;

/// The generated argument parser is large; a debug build needs more than a Windows main thread's
/// 1 MiB (the same reason engine-cli runs on its own thread).
const STACK_BYTES: usize = 16 * 1024 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Ctrl-C and SIGTERM set a flag that destructive sequences honour at step boundaries (see
    // `signals`); without this a kill between two steps leaves a half-done restore.
    dynodoctl::signals::install();
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
    // A panic is caught and reported with a message and a JSON error by `cli::run`; this is the
    // last line of defence for one that escapes it (it must never be a silent exit status).
    let code = worker.join().unwrap_or_else(|_| {
        eprintln!("ERROR: internal error: dynodoctl stopped unexpectedly (a bug)\n  What to do: run `dynodoctl status`; if a restore was running, `dynodoctl recover` puts the databases right. Please report this.");
        1
    });
    std::process::exit(code);
}
