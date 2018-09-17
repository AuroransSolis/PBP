use std::io::{self, Read, prelude::*, stdout, stdin};
use std::net::{TcpListener, TcpStream};
use std::mem::transmute_copy;
use std::sync::{mpsc::{channel, Sender, Receiver}, Arc, Mutex};
use std::time::Duration;
use std::thread::{self, JoinHandle};
use std::io::ErrorKind::WouldBlock;

const MAX_X: u64 = 1000;
const TRY_RECV_DATA_TIMEOUT: u64 = 5;

fn spawn_io_manager(inst_sender: Sender<u8>) {
    thread::spawn(move || {
        'io_management: loop {
            let mut input_buffer = String::new();
            stdout().flush().unwrap();
            stdin().read_line(&mut input_buffer).unwrap();
            input_buffer = input_buffer.trim().to_owned();
            match input_buffer.as_str() {
                "qs" => {
                    inst_sender.send(3).unwrap();
                    println!("Sent terminate signal to main thread.");
                    break 'io_management;
                },
                "qp" => {
                    inst_sender.send(0).unwrap();
                    println!("Sent progress query signal to main thread.");
                },
                "pause" => {
                    inst_sender.send(1).unwrap();
                    println!("Sent pause signal to main thread.");
                },
                "play" => {
                    inst_sender.send(2).unwrap();
                    println!("Sent play signal to main thread.");
                },
                "nc" => {
                    inst_sender.send(4).unwrap();
                    println!("Sent query about number of connections to main thread.");
                },
                "qf" => {
                    inst_sender.send(5).unwrap();
                },
                "h" => {
                    println!("Available commands:\n\
                        Format:\n\
                    Name(command): description\n\
                    Quit(q): forces the program to exit, regardless of whether or not clients are\
                    \n\tfinished with execution or not.\n\
                    Query progress(qp): query the progress of each client.\n\
                    Pause(pause): pauses execution of clients.\n\
                        \tpause, stop, halt\n\
                    Play(play): resumes execution of clients.\n\
                        \tplay, continue\n\
                    Number of connections(nc): print out how many clients are connected.\n\
                        \tnum connections, number of connections\n\
                    Help(h): prints this dialog\n\
                        \th, help");
                },
                _ => {}
            }
        }
    });
}

fn spawn_handler_thread(c_no: usize, mut tcp_stream: TcpStream, manager_inst_recv: Receiver<u8>,
                        arc_mut_iter: Arc<Mutex<Iterator<Item = u64>>>) {
    thread::spawn(move || {
        'main_loop: loop {
            // Handle receiving instructions from the TCP stream
            let mut recv_instr_byte: [u8; 1] = [0];
            if let Err(e) = tcp_stream.read_exact(&mut recv_instr_byte) {
                if e.kind() != WouldBlock {
                    println!("TCP stream {} disconnected.", c_no);
                    break 'main_loop;
                }
                continue 'main_loop;
            }
            let instr = recv_instr_byte[0] as char;
            match instr {
                'a' => {
                    let mut recv_at_bytes: [u8; 24] = [0; 24];
                    let tmr = std::time::Instant::now();
                    while let Err(e) = tcp_stream.read_exact(&mut recv_at_bytes) {
                        if e.kind() != WouldBlock {
                            println!("TCP stream {} disconnected.", c_no);
                            break 'main_loop;
                        }
                        if tmr.elapsed().as_secs() > TRY_RECV_DATA_TIMEOUT {
                            println!("Was unable to receive XYZ from client {}.", i);
                            continue 'main_loop;
                        }
                    }
                    let recv_at_xyz = unsafe {
                        transmute_copy::<[u8; 24], [u64; 3]>(&recv_at_bytes)
                    };
                    if recv_xyz == ['e' as u64, 'r' as u64, 'r' as u64] {
                        println!("Client {}: error receiving progress - no current x value.");
                    } else {
                        println!("Client {} progress at {:?} | x: {}, y: {}, z: {}", i,
                                 timer.elapsed(), recv_at_xyz[0], recv_at_xyz[1], recv_at_xyz[2]);
                    }
                },
                'r' => {
                    // I put getting a lock in its own scope to try to facilitate dropping the lock
                    // as soon as possible.
                    let x = {
                        let mut iterator = arc_mut_iter.lock().unwrap();
                        if let Some(x) = iterator.next() {
                            x
                        } else {
                            // Write 't' to the TCP stream for "Terminate"
                            drop(tcp_stream.write_all(&['t' as u8; 1]));
                            // Write 'f' to the TCP stream for "iterator Finished"
                            drop(tcp_stream.write_all(&['f' as u8; 1]));
                            break 'main_loop;
                        }
                    };
                    // Write 'x' for new x
                    drop(tcp_stream.write_all(&['x' as u8; 1]));
                    // Get bytes for 'x'
                    let mut x_bytes = unsafe {
                        transmute_copy::<u64, [u8; 8]>(&x)
                    };
                    // Write bytes for 'x'
                    drop(tcp_stream.write_all(&x_bytes));
                },
                's' => {
                    let mut recv_solution_bytes: [u8; 24] = [0; 24];
                    if let Err(e) = tcp_stream.read_exact(&mut recv_solution_bytes) {
                        if e.kind() != WouldBlock {
                            ;
                            println!("TCP stream {} disconnected.", i);
                        }
                        continue 'main_loop;
                    }
                    let recv_solution_xyz = unsafe {
                        transmute_copy::<[u8; 24], [u64; 3]>(&recv_solution_bytes)
                    };
                    if test_squares(recv_solution_xyz[0], recv_solution_xyz[1],
                                    recv_solution_xyz[2]) {
                        println!("Found solution! x: {}, y: {}, z: {}", recv_solution_xyz[0],
                                 recv_solution_xyz[1], recv_solution_xyz[2]);
                    }
                },
                't' => {
                    // Shutdown TCP stream.
                    break 'main_loop;
                },
                _ => {}
            }
            if let Ok(inst) = manager_inst_recv.try_recv() {
                match inst {
                    0 => {

                    }
                }
            }
        }
    });
}

fn main() {
    let listener = TcpListener::bind("169.254.36.152:1337").unwrap();
    listener.set_nonblocking(true).expect("Could not set listener to be non-blocking.");
    let timer = std::time::Instant::now();
    let mut iterator = Arc::new(Mutex::new((2..MAX_X).map(|x| x * x).filter(|&x| x % 24 == 1)));
    let (inst_sender, inst_receiver): (Sender<u8>, Receiver<u8>) = channel();
    // I won't actually join() this at the end, since working in a proper receiving loop would be
    // a pain. So I'll just ignore joining it.
    spawn_io_manager(inst_sender);
    let mut handler_thread_inst_senders = Vec::new();
    let mut handler_thread_handles = Vec::new();
    let mut send_term_signal = false;
    let mut soft_term = true;
    'main_loop: loop {
        if send_term_signal {
            if soft_term {

            }
            for wrap_up_sender in wrap_up_senders {
                wrap_up_sender.send(()).unwrap();
            }
            println!("Sent wrap-up signal to all TCP handler threads.");
            break 'main_loop;
        };
        if let Ok((tcpstream, _)) = listener.accept() {
            tcpstream.set_read_timeout(Some(Duration::from_millis(5000))).expect("Unable to set \
            write timeout on new connection.");
            println!("    Accepted new connection! {:?}", tcpstream.peer_addr());
            let (handler_inst_send, handler_inst_recv): (Sender<u8>, Receiver<u8>) = channel();
            handler_thread_inst_senders.push(handler_inst_send);
            let client_number = handler_thread_handles.len();
            let arc_pointer = Arc::clone(&iterator);
            handler_thread_handles.push(
                spawn_handler_thread(client_number, tcpstream, handler_inst_recv, arc_pointer)
            );
        }
        // Check for instruction on the IO channel
        if let Ok(instr) = inst_receiver.try_recv() {
            match instr {
                0 => { // Progress query
                    // Try to write a progress query to all connected streams
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        // Pass on the '0' to the TCP stream handler threads.
                        if let Err(_) = handler_thread_inst_sender.send(0) {
                            println!("Client {} disconnected.", i);
                        }
                    }
                    println!("Passed on progress query to all handler threads.");
                },
                1 => { // Pause
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(1) {
                            println!("Client {} disconnected.", i);
                        }
                    }
                    println!("Passed on pause command to all handler threads.");
                },
                2 => { // Play
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(2) {
                            println!("Client {} disconnected.", i);
                        }
                    }
                    println!("Passed on play command to all handler threads.");
                },
                3 => { // Terminate
                    send_term_signal = true;
                    println!("Set send_term_signal to true.");
                    continue 'main_loop;
                },
                4 => { // Query number of connections
                    println!("Current number of connections: {}", handler_thread_handles.len());
                },
                5 => {

                }
                _ => {}
            }
        }
        for (i, connected_tf) in still_connected.into_iter().enumerate() {
            if !connected_tf {
                tcpstreams.remove(i);
            }
        }
    }
    terminal_manager.join().unwrap();
    println!("Execution completed.");
}

// Using method found on SE
const GOOD_MASK: u64 = 0xC840C04048404040;

fn is_valid_square(mut n: u64) -> bool {
    if n % 24 != 1 {
        return false;
    }
    if (GOOD_MASK << n) as i64 >= 0 {
        return false;
    }
    let zeros = n.trailing_zeros();
    if zeros & 1 != 0 {
        return false;
    }
    n >>= zeros;
    if n & 7 != 1 {
        return n == 0;
    }
    ((n as f64).sqrt() as u64).pow(2) == n
}

fn test_squares(x: u64, y: u64, z: u64) -> bool {
    is_valid_square(x + y) && is_valid_square(x - y - z) && is_valid_square(x + z)
        && is_valid_square(x - y + z) && is_valid_square(x + y - z)
        && is_valid_square(x - z) && is_valid_square(x + y + z) && is_valid_square(x - y)
}