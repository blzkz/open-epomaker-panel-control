//! Prints the foreground application a few times: `cargo run -p epomaker-host --example active_window`.
fn main() {
    let mut t = match epomaker_host::window::tracker() {
        Ok(t) => t,
        Err(e) => return eprintln!("{e}"),
    };
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(400));
        println!("{:?}", t.current());
    }
}
