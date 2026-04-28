use std::{io::{self, Write}, sync::{Arc, Mutex}, thread::{self, JoinHandle}, time::Duration};

use uuid::Uuid;

const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

pub struct TerminalWriter {
    running_processes: Arc<Mutex<Vec<Process>>>,
    spinner: Mutex<Option<Spinner>>
}

struct Process {
    id: Uuid,
    message: String
}

impl TerminalWriter {
    pub fn new() -> TerminalWriter {
        TerminalWriter {
            running_processes: Arc::new(Mutex::new(vec![])),
            spinner: Mutex::new(None)
        }
    }

    pub fn write_async_process(&self, input: String) -> Uuid {
        let process_id = Uuid::new_v4();
        self.running_processes.lock().unwrap().push(Process { id: process_id, message: input });

        self.print();

        process_id
    }

    pub fn end_async_process(&self, process_id: Uuid) {
        let mut processes = self.running_processes.lock().unwrap();
        if let Some(index) = processes.iter().position(|p| p.id == process_id) {
            processes.remove(index);
        }

        self.print();
    }

    fn print(&self) -> Spinner {
        if let Some(spinner) = self.spinner.lock().unwrap().take() {
            spinner.stop();
        }

        let stop_flag = Arc::new(Mutex::new(false));
        let running_processes = self.running_processes.clone();
        let handle = thread::spawn({
            let stop_flag = Arc::clone(&stop_flag);
            move || {
                const FRAMES: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
                let mut i = 0;
                while !*stop_flag.lock().unwrap() {
                    clear_console();
                    let mut line = 0;

                    for item in running_processes.lock().unwrap().iter() {
                        move_cursor(0, line);
                        print!(
                            "{} {}{}{} ",
                            item.message,
                            YELLOW,
                            FRAMES[i % FRAMES.len()],
                            RESET
                        );

                        line += 1;
                    }

                    io::stdout().flush().unwrap();
                    thread::sleep(Duration::from_millis(250));
                    i += 1;
                }
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

fn clear_console() {
    print!("\x1B[2J\x1B[3J\x1B[1;1H");
}
