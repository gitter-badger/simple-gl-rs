use gl;
use gl_init;
use native::NativeTaskBuilder;
use std::sync::{Mutex, Future};
use std::task::TaskBuilder;
use time;

enum Message {
    EndFrame,
    Execute(proc():Send),
}

pub struct Context {
    commands: Mutex<Sender<Message>>,
    events: Mutex<Receiver<gl_init::Event>>,
}

impl Context {
    pub fn new(window: gl_init::Window) -> Context {
        let (tx_events, rx_events) = channel();
        let (tx_commands, rx_commands) = channel();

        let context = Context {
            commands: Mutex::new(tx_commands),
            events: Mutex::new(rx_events),
        };

        TaskBuilder::new().native().spawn(proc() {
            unsafe { window.make_current(); }

            gl::load_with(|symbol| window.get_proc_address(symbol));

            let mut next_loop = time::precise_time_ns();
            'main: loop {
                // sleeping until next frame must be drawn
                use std::io::timer;
                timer::sleep({ 
                    let now = time::precise_time_ns();
                    if next_loop < now {
                        0
                    } else {
                        (next_loop - now) / (1000 * 1000)
                    }
                });

                // processing commands
                loop {
                    use std::comm::{Disconnected, Empty};

                    match rx_commands.recv_opt() {
                        Ok(EndFrame) => break,
                        Ok(Execute(cmd)) => cmd(),
                        Err(_) => break 'main
                    }
                }

                // swapping
                window.swap_buffers();

                // getting events
                for event in window.poll_events() {
                    if tx_events.send_opt(event.clone()).is_err() {
                        break 'main;
                    }

                    match event {
                        gl_init::Closed => break 'main,
                        _ => ()
                    }
                }

                // finding time to next loop
                next_loop += 16666667;
            }
        });

        context
    }

    pub fn exec<T:Send>(&self, f: proc(): Send -> T) -> Future<T> {
        let (tx, rx) = channel();
        self.commands.lock().send(Execute(proc() {
            let _ = tx.send_opt(f());
        }));
        Future::from_receiver(rx)
    }

    pub fn swap_buffers(&self) {
        self.commands.lock().send(EndFrame);
    }

    pub fn recv(&self) -> Vec<gl_init::Event> {
        let mut events = self.events.lock();

        let mut result = Vec::new();
        loop {
            match events.try_recv() {
                Ok(ev) => result.push(ev),
                Err(_) => break
            }
        }
        result
    }
}
