use std::io::{Read, prelude::*, stdout, stdin};
use std::net::{TcpListener, TcpStream};
use std::mem::transmute_copy;
use std::sync::{mpsc::{channel, Sender, Receiver}, Arc, Mutex};
use std::thread;
use std::io::ErrorKind::{WouldBlock};
use std::time::Instant;

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
                },
                "qp" => {
                    inst_sender.send(PROGRESS_QUERY).unwrap();
                },
                "pause" => {
                    inst_sender.send(PAUSE).unwrap();
                },
                "play" => {
                    inst_sender.send(PLAY).unwrap();
                },
                "nc" => {
                    inst_sender.send(NUM_CONNECTIONS).unwrap();
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
    let mut response: [u8; 1] = [0];
    tcp_stream.set_read_timeout(Some(std::time::Duration::from_secs(HEARTBEAT_TIMEOUT)))
        .expect(format!("Was unable to set client ({:?}) read timeout for heartbeat",
            tcp_stream.peer_addr()).as_str());
    match tcp_stream.read_exact(&mut response) {
        Ok(_) => {
            tcp_stream.set_read_timeout(None).expect(format!("Was unable to reset client ({:?}) \
            read timeout after heartbeat", tcp_stream.peer_addr()).as_str());
            tcp_stream.set_nonblocking(true).expect("Unable to reset TCP stream to \
            nonblocking.");
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
                        manager_heartbeat_recv: Receiver<()>) {
    println!("({}) Started handler!", c_no);
    thread::spawn(move || {
        let total_timer = Instant::now();
        let mut timer = Instant::now();
        'main_loop: loop {
            drop(manager_heartbeat_recv.try_recv());
            tcp_stream.set_nonblocking(false).expect("Unable to set TCP stream to blocking \
            for heartbeat.");
            if timer.elapsed().as_secs() > HEARTBEAT_TIMER {
                if !heartbeat(&mut tcp_stream, c_no) {
                    break 'main_loop;
                }
                timer = Instant::now();
            }
            tcp_stream.set_nonblocking(true).expect("Unable to reset TCP stream to \
            nonblocking.");
            if let Ok(inst) = manager_inst_recv.try_recv() {
                match inst {
                    PROGRESS_QUERY => {
                        drop(tcp_stream.write_all(&[COMMAND as u8; 1]));
                        drop(tcp_stream.write_all(&[PROGRESS_QUERY; 1]));
                    },
                    PAUSE => {
                        drop(tcp_stream.write_all(&[COMMAND as u8; 1]));
                        drop(tcp_stream.write_all(&[PAUSE; 1]));
                    },
                    PLAY => {
                        drop(tcp_stream.write_all(&[COMMAND as u8; 1]));
                        drop(tcp_stream.write_all(&[PLAY; 1]));
                    },
                    SOFT_TERMINATE => {
                        drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
                        drop(tcp_stream.write_all(&[SOFT as u8; 1]));
                    },
                    HARD_TERMINATE => {
                        drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
                        drop(tcp_stream.write_all(&[HARD as u8; 1]));
                    },
                    _ => {}
                }
            }
            // Handle receiving instructions from the TCP stream
            let mut recv_instr_byte: [u8; 1] = [0];
            if tcp_stream.read_exact(&mut recv_instr_byte).is_err() {
                continue 'main_loop;
            }
            let instr = recv_instr_byte[0] as char;
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
                                 total_timer.elapsed(), recv_at_xyz[0], recv_at_xyz[1],
                                 recv_at_xyz[2]);
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
                            drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
                            drop(tcp_stream.write_all(&[FINISHED as u8; 1]));
                            break 'main_loop;
                        }
                    };
                    //println!("Writing 'x' to stream.");
                    drop(tcp_stream.write_all(&[NEW_X as u8; 1]));
                    // Get bytes for 'x'
                    let mut x_bytes = unsafe {
                        transmute_copy::<u64, [u8; 8]>(&x)
                    };
                    // Write bytes for 'x'
                    //println!("Writing x bytes to stream.");
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
                    break 'main_loop;
                },
                _ => continue 'main_loop
            }
        }
        println!("Client closing. Total runtime: {:?}", total_timer.elapsed());
    });
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
    let mut handler_heartbeat_senders = Vec::new();
    let mut still_connected = Vec::new();
    let mut send_term_signal = false;
    let mut soft_term = true;
    let mut await_completion = false;
    'main_loop: loop {
        if send_term_signal {
            if soft_term {
                for handler_thread_inst_sender in handler_thread_inst_senders.iter() {
                    handler_thread_inst_sender.send(SOFT_TERMINATE).unwrap();
                }
                await_completion = true;
                println!("Sent soft terminate signal to all TCP handler threads.");
                break 'main_loop;
            } else {
                for handler_thread_inst_sender in handler_thread_inst_senders.iter() {
                    handler_thread_inst_sender.send(HARD_TERMINATE).unwrap();
                }
                break 'main_loop;
            }
        };
        if let Ok((tcpstream, _)) = listener.accept() {
            println!("Accepted new connection! {:?}", tcpstream.peer_addr());
            let (handler_inst_send, handler_inst_recv): (Sender<u8>, Receiver<u8>) = channel();
            handler_thread_inst_senders.push(handler_inst_send);
            let (handler_heartbeat_s, handler_heartbeat_r): (Sender<()>, Receiver<()>) = channel();
            handler_heartbeat_senders.push(handler_heartbeat_s);
            still_connected.push(true);
            let arc_pointer = Arc::clone(&iterator);
            spawn_handler_thread(handler_heartbeat_senders.len(), tcpstream, handler_inst_recv,
                                 arc_pointer, handler_heartbeat_r);
        }
        // Check for instruction on the IO channel
        if let Ok(instr) = inst_receiver.try_recv() {
            match instr {
                PROGRESS_QUERY => {
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(PROGRESS_QUERY) {
                            still_connected[i] = false;
                            println!("Client {} disconnected.", i);
                        }
                    }
                },
                PAUSE => {
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(PAUSE) {
                            still_connected[i] = false;
                            println!("Client {} disconnected.", i);
                        }
                    }
                },
                PLAY => {
                    for (i, handler_thread_inst_sender) in handler_thread_inst_senders.iter()
                        .enumerate() {
                        if let Err(_) = handler_thread_inst_sender.send(PLAY) {
                            still_connected[i] = false;
                            println!("Client {} disconnected.", i);
                        }
                    }
                },
                SOFT_TERMINATE => {
                    send_term_signal = true;
                    continue 'main_loop;
                },
                NUM_CONNECTIONS => {
                    println!("Current number of connections: {}", handler_heartbeat_senders.len());
                },
                HARD_TERMINATE => {
                    send_term_signal = true;
                    soft_term = false;
                    continue 'main_loop;
                }
                _ => {}
            }
        }
        for (i, hhs) in handler_heartbeat_senders.iter().enumerate() {
            if hhs.send(()).is_err() {
                println!("TCP handler {} dropped its heartbeat receiver; assuming thread closed.", i);
                still_connected[i] = false;
            }
        }
        for (i, _) in still_connected.iter().rev().enumerate().filter(|&(_, &b)| !b) {
            handler_heartbeat_senders.remove(i);
            handler_thread_inst_senders.remove(i);
        }
        still_connected = still_connected.iter().map(|&b| b).filter(|&b| b).collect::<Vec<bool>>();
    }
    if await_completion {
        'await_completion: loop {
            let mut disconnected = 0;
            for heartbeat_sender in & handler_heartbeat_senders {
                if heartbeat_sender.send(()).is_err() {
                    disconnected += 1;
                }
            }
            if disconnected == handler_heartbeat_senders.len() {
                break 'await_completion;
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