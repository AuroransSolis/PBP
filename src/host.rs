use std::io::{Read, prelude::*, stdout, stdin};
use std::net::{TcpListener, TcpStream};
use std::mem::transmute_copy;
use std::sync::{mpsc::{channel, Sender, Receiver}, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::io::ErrorKind::{WouldBlock};
use std::time::{SystemTime, Instant};

const MAX_X: u64 = 1000;
const TRY_RECV_DATA_TIMEOUT: u64 = 5;

const PROGRESS: char = 'a';
const REQUEST: char = 'r';
const SOLUTION: char = 's';
const TERMINATING: char = 't';
const TERMINATE: char = 't';
const SOFT: char = 's';
const HARD: char = 'h';
const FINISHED: char = 'f';
const NEW_X: char = 'x';
const COMMAND: char = 'c';

// Management command values
const PROGRESS_QUERY: u8 = 0;
const PAUSE: u8 = 1;
const PLAY: u8 = 2;
const SOFT_TERMINATE: u8 = 3;
const HARD_TERMINATE: u8 = 4;
const NUM_CONNECTIONS: u8 = 5;

fn spawn_io_manager(inst_sender: Sender<u8>) {
    thread::spawn(move || {
        'io_management: loop {
            let mut input_buffer = String::new();
            stdout().flush().unwrap();
            stdin().read_line(&mut input_buffer).unwrap();
            input_buffer = input_buffer.trim().to_owned();
            match input_buffer.as_str() {
                "qs" => {
                    inst_sender.send(SOFT_TERMINATE).unwrap();
                    println!("Sent terminate signal to main thread.");
                    break 'io_management;
                },
                "qp" => {
                    inst_sender.send(PROGRESS_QUERY).unwrap();
                    println!("Sent progress query signal to main thread.");
                },
                "pause" => {
                    inst_sender.send(PAUSE).unwrap();
                    println!("Sent pause signal to main thread.");
                },
                "play" => {
                    inst_sender.send(PLAY).unwrap();
                    println!("Sent play signal to main thread.");
                },
                "nc" => {
                    inst_sender.send(NUM_CONNECTIONS).unwrap();
                    println!("Sent query about number of connections to main thread.");
                },
                "qf" => {
                    inst_sender.send(HARD_TERMINATE).unwrap();
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

const HEARTBEAT_TIMEOUT: u64 = 5;
const HEARTBEAT_TIMER: u64 = 30;
const HEARTBEAT_CHAR: char = 'h';

fn heartbeat(tcp_stream: &mut TcpStream, c_no: usize) -> bool {
    drop(tcp_stream.write_all(&[HEARTBEAT_CHAR as u8; 1]));
    println!("({}) Sent heartbeat to client.", c_no);
    let mut response: [u8; 1] = [0];
    tcp_stream.set_read_timeout(Some(std::time::Duration::from_secs(HEARTBEAT_TIMEOUT)))
        .expect(format!("Was unable to set client ({:?}) read timeout for heartbeat",
            tcp_stream.peer_addr()).as_str());
    match tcp_stream.read_exact(&mut response) {
        Ok(_) => {
            tcp_stream.set_read_timeout(None).expect(format!("Was unable to reset client ({:?}) \
            read timeout after heartbeat", tcp_stream.peer_addr()).as_str());
            println!("({}) Heartbeat successful! Response: '{}'", c_no, response[0] as char);
            true
        },
        Err(_) => {
            tcp_stream.set_read_timeout(None).expect(format!("Was unable to reset client ({:?}) \
            read timeout after heartbeat", tcp_stream.peer_addr()).as_str());
            println!("({}) Heartbeat unsuccessful!", c_no);
            false
        }, 
    }
}

fn spawn_handler_thread(c_no: usize, mut tcp_stream: TcpStream, manager_inst_recv: Receiver<u8>,
                        arc_mut_iter: Arc<Mutex<Iterator<Item = u64> + Send + 'static>>,
                        _manager_heartbeat_recv: Receiver<()>)
    -> JoinHandle<()> {
    println!("({}) Started handler!", c_no);
    thread::spawn(move || {
        let total_timer = Instant::now();
        let mut timer = Instant::now();
        'main_loop: loop {
            if timer.elapsed().as_secs() > HEARTBEAT_TIMER {
                println!("({}) Sending heartbeat to client.", c_no);
                if !heartbeat(&mut tcp_stream, c_no) {
                    break 'main_loop;
                }
                timer = Instant::now();
            }
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
            println!("({}) Got type indicator from client: '{}'", c_no, instr);
            match instr {
                PROGRESS => {
                    let mut recv_at_bytes: [u8; 24] = [0; 24];
                    let tmr = Instant::now();
                    while let Err(e) = tcp_stream.read_exact(&mut recv_at_bytes) {
                        if e.kind() != WouldBlock {
                            println!("({}) TCP stream {0} disconnected.", c_no);
                            break 'main_loop;
                        }
                        if tmr.elapsed().as_secs() > TRY_RECV_DATA_TIMEOUT {
                            println!("({}) Was unable to receive XYZ from client {0}.", c_no);
                            continue 'main_loop;
                        }
                    }
                    let recv_at_xyz = unsafe {
                        transmute_copy::<[u8; 24], [u64; 3]>(&recv_at_bytes)
                    };
                    if recv_at_xyz == ['e' as u64, 'r' as u64, 'r' as u64] {
                        println!("({}) Error receiving progress - no current x value.", c_no);
                    } else {
                        println!("({}) progress at {:?} | x: {}, y: {}, z: {}", c_no,
                                 timer.elapsed(), recv_at_xyz[0], recv_at_xyz[1], recv_at_xyz[2]);
                    }
                },
                REQUEST => {
                    // I put getting a lock in its own scope to try to facilitate dropping the lock
                    // as soon as possible.
                    let x = {
                        let mut iterator = arc_mut_iter.lock().unwrap();
                        if let Some(x) = iterator.next() {
                            x
                        } else {
                            // Write 't' to the TCP stream for "Terminate"
                            drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
                            // Write 'f' to the TCP stream for "iterator Finished"
                            drop(tcp_stream.write_all(&[FINISHED as u8; 1]));
                            break 'main_loop;
                        }
                    };
                    // Write 'x' for new x
                    drop(tcp_stream.write_all(&[NEW_X as u8; 1]));
                    // Get bytes for 'x'
                    let mut x_bytes = unsafe {
                        transmute_copy::<u64, [u8; 8]>(&x)
                    };
                    let mut x_bytes_string = x_bytes.iter().map(|b| format!("{:08b}", b)).fold("".to_string(), |r, b| format!("{}{}, ", r, b));
                    drop(x_bytes_string.pop());
                    drop(x_bytes_string.pop());
                    println!("({}) Sending bytes in response to data request: [{}] ({})", c_no, x_bytes_string, x);
                    // Write bytes for 'x'
                    drop(tcp_stream.write_all(&x_bytes));
                },
                SOLUTION => {
                    let mut recv_solution_bytes: [u8; 24] = [0; 24];
                    if let Err(e) = tcp_stream.read_exact(&mut recv_solution_bytes) {
                        if e.kind() != WouldBlock {
                            println!("({}) TCP stream {0} disconnected.", c_no);
                            break 'main_loop;
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
                TERMINATING => {
                    // Shutdown TCP stream.
                    break 'main_loop;
                },
                _ => {}
            }
            println!("({}) Checking for management instruction.", c_no);
            if let Ok(inst) = manager_inst_recv.try_recv() {
                println!("({}) Got management instruction: {}", c_no, inst);
                match inst {
                    PROGRESS_QUERY => { // Progress query
                        drop(tcp_stream.write_all(&[COMMAND as u8; 1]));
                        drop(tcp_stream.write_all(&[PROGRESS_QUERY; 1]));
                    },
                    PAUSE => { // Pause
                        drop(tcp_stream.write_all(&[COMMAND as u8; 1]));
                        drop(tcp_stream.write_all(&[PAUSE; 1]));
                    },
                    PLAY => { // Play
                        drop(tcp_stream.write_all(&[COMMAND as u8; 1]));
                        drop(tcp_stream.write_all(&[PLAY; 1]));
                    },
                    SOFT_TERMINATE => { // Soft terminate
                        drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
                        drop(tcp_stream.write_all(&[SOFT as u8; 1]));
                    },
                    HARD_TERMINATE => { // Hard terminate
                        drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
                        drop(tcp_stream.write_all(&[HARD as u8; 1]));
                    },
                    _ => {}
                }
            } else {
                println!("({}) Found none.", c_no);
            }
        }
        println!("Client closing. Total runtime: {:?}", total_timer.elapsed());
    })
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:1337").unwrap();
    listener.set_nonblocking(true).expect("Could not set listener to be non-blocking.");
    let timer = Instant::now();
    let iterator = Arc::new(Mutex::new((2..MAX_X).map(|x| x * x).filter(|&x| x % 24 == 1)));
    let (inst_sender, inst_receiver): (Sender<u8>, Receiver<u8>) = channel();
    // I won't actually join() this at the end, since working in a proper receiving loop would be
    // a pain. So I'll just ignore joining it.
    spawn_io_manager(inst_sender);
    let mut handler_thread_inst_senders: Vec<Sender<u8>> = Vec::new();
    let mut handler_thread_handles = Vec::new();
    let mut handler_heartbeat_senders = Vec::new();
    let mut send_term_signal = false;
    let mut soft_term = true;
    let mut client_number = 0;
    'main_loop: loop {
        if send_term_signal {
            if soft_term {
                for handler_thread_inst_sender in handler_thread_inst_senders.iter() {
                    handler_thread_inst_sender.send(SOFT_TERMINATE).unwrap();
                }
                println!("Sent soft terminate signal to all TCP handler threads.");
            } else {
                for handler_thread_inst_sender in handler_thread_inst_senders.iter() {
                    handler_thread_inst_sender.send(HARD_TERMINATE).unwrap();
                }
                break 'main_loop;
            }
        };
        // Some major "WHAT THE FUCK" going on here. The host only ever gets 1 JoinHandle<()>.
        if let Ok((tcpstream, _)) = listener.accept() {
            println!("Accepted new connection! {:?}", tcpstream.peer_addr());
            let (handler_inst_send, handler_inst_recv): (Sender<u8>, Receiver<u8>) = channel();
            handler_thread_inst_senders.push(handler_inst_send);
            let (handler_heartbeat_s, handler_heartbeat_r): (Sender<()>, Receiver<()>) = channel();
            handler_heartbeat_senders.push(handler_heartbeat_s);
            let arc_pointer = Arc::clone(&iterator);
            let tcp_handler = spawn_handler_thread(client_number, tcpstream, handler_inst_recv,
                                                   arc_pointer, handler_heartbeat_r);
            handler_thread_handles.push(tcp_handler);
            client_number += 1;
            println!("Number of connections: {}", handler_thread_handles.len());
        }
        // Check for instruction on the IO channel
        if let Ok(instr) = inst_receiver.try_recv() {
            println!("(MAIN) got management instruction: {}", instr);
            match instr {
                PROGRESS_QUERY => {
                    // Try to write a progress query to all connected streams
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        // Pass on the '0' to the TCP stream handler threads.
                        if let Err(_) = handler_thread_inst_sender.send(PROGRESS_QUERY) {
                            println!("Client {} disconnected.", i);
                        }
                    }
                    println!("Passed on progress query to all handler threads.");
                },
                PAUSE => {
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(PAUSE) {
                            println!("Client {} disconnected.", i);
                        }
                    }
                    println!("Passed on pause command to all handler threads.");
                },
                PLAY => {
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(PLAY) {
                            println!("Client {} disconnected.", i);
                        }
                    }
                    println!("Passed on play command to all handler threads.");
                },
                SOFT_TERMINATE => {
                    send_term_signal = true;
                    println!("Set send_term_signal to true, set soft_term to true.");
                    continue 'main_loop;
                },
                NUM_CONNECTIONS => {
                    println!("Current number of connections: {} ({:?})", handler_thread_handles.len(), handler_thread_handles);
                },
                HARD_TERMINATE => {
                    send_term_signal = true;
                    soft_term = false;
                    println!("Set send_term_signal to true, set soft_term to false.");
                    continue 'main_loop;
                }
                _ => {}
            }
        }
        for i in 0..handler_heartbeat_senders.len() {
            if handler_heartbeat_senders[i].send(()).is_err() {
                println!("TCP handler {} dropped its heartbeat receiver; assuming thread closed.", i);
                handler_thread_handles.remove(i);
                handler_thread_inst_senders.remove(i);
                handler_heartbeat_senders.remove(i);
            }
        }
    }
    println!("Closing main. Total runtime: {:?}", timer.elapsed());
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