use std::{io::{self, Write}, sync::{Arc, Mutex}, thread::{self, JoinHandle}, time::Duration};

use uuid::Uuid;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const RESET: &str = "\x1b[0m";

struct TerminalWriter {
    running_processes: Arc<Mutex<Vec<Process>>>,
    spinner: Option<Spinner>
}

struct Process {
    id: Uuid,
    message: String
}

impl TerminalWriter {
    pub fn new() -> TerminalWriter {
        TerminalWriter {
            running_processes: Arc::new(Mutex::new(vec![])),
            spinner: None
        }
    }

    pub fn write_async_process(&mut self, input: String) -> Uuid {
        let process_id = Uuid::new_v4();
        self.running_processes.lock().unwrap().push(Process { id: process_id, message: input });

        process_id
    }

    pub fn end_async_process(&self, process_id: Uuid) {
        let mut processes = self.running_processes.lock().unwrap();
        if let Some(index) = processes.iter().position(|p| p.id == process_id) {
            processes.swap_remove(index);
        }

        self.print();
    }

    fn print(&self) -> Spinner {
        let stop_flag = Arc::new(Mutex::new(false));
        let stop_clone = Arc::clone(&stop_flag);
        let mutex_guard = self.running_processes.clone();
        let handle = thread::spawn(move || {
            const FRAMES: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
            let mut i = 0;
            while !*stop_clone.lock().unwrap() {

                for item in mutex_guard.lock().unwrap().iter() {
                    // TODO: handle cursor location
                    print!(
                        "{} {}{}{} ",
                        item.message,
                        YELLOW,
                        FRAMES[i % FRAMES.len()],
                        RESET
                    );
                }

                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(100));
                i += 1;
            }
        });

        Spinner {
            stop_flag,
            handle
        }
    }
}

struct Spinner {
    stop_flag: Arc<Mutex<bool>>,
    handle: JoinHandle<()>,
}

impl Spinner {
    fn stop(self) {
        *self.stop_flag.lock().unwrap() = true;
        self.handle.join().unwrap();
    }
}

fn move_cursor(x: usize, y: usize) {
    print!("\x1B[{};{}H", y + 1, x + 1);
}

