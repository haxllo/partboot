use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct Spinner {
    running: Arc<AtomicBool>,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let message = message.to_string();

        thread::spawn(move || {
            let frames = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut frame = 0;

            while running_clone.load(Ordering::Relaxed) {
                print!("\r{} {}", frames[frame % frames.len()], message);
                io::stdout().flush().ok();
                frame += 1;
                thread::sleep(Duration::from_millis(80));
            }
            // Clear spinner line
            print!("\r{}\r", " ".repeat(message.len() + 3));
            io::stdout().flush().ok();
        });

        Spinner { running }
    }

    pub fn finish(self, completion_message: &str) {
        self.running.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100)); // Let spinner finish
        println!("✓ {}", completion_message);
    }

    pub fn finish_error(self, error_message: &str) {
        self.running.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100));
        println!("✗ {}", error_message);
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100));
    }
}
