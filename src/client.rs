use std::sync::{mpsc, mpsc::{Sender, Receiver}};
use std::thread;
use std::io::{prelude::*, stdin, stdout};
use std::net::TcpStream;
use std::mem::transmute_copy;
use std::time::Duration;

const MAX_X: u64 = 1000;
const NUM_THREADS: usize = 4;

macro_rules! spawn_manager_io_thread {
    ($io_inst_sender:ident) => {
        thread::spawn(move || {
            'io_loop: loop {
                let mut input_buffer = String::new();
                stdout().flush().unwrap();
                stdin().read_line(&mut input_buffer).unwrap();
                input_buffer = input_buffer.trim().to_owned();
                if input_buffer == "quit -s" || input_buffer == "quit" {
                    println!("    Sending soft terminate signal to all threads.");
                    $io_inst_sender.send(1).unwrap();
                    break 'io_loop;
                } else if input_buffer == "quit -f" {
                    println!("    Sending hard terminate signal to all threads.");
                    $io_inst_sender.send(2).unwrap();
                    break 'io_loop;
                } else if input_buffer == "at" {
                    println!("    Querying progress on all threads.");
                    $io_inst_sender.send(0).unwrap();
                } else if input_buffer == "h" {
                    println!("Available commands:\n\
                        Format\n\
                    Name(command): description\n\
                        Options: [options]
                    Quit(q): end execution of this client and its threads.\n\
                        Options: -s (soft terminate; each thread finishes testing its range of \n\
                        values before returning and joining), -f (hard/force terminate; each\n\
                        thread stops testing immediately upon getting this signal). Default: -s\n\
                    Query progress(at): query progress of child threads.\n\
                    Help(h): prints this dialog.");
                }
            }
        })
    };
}

macro_rules! recv_incoming_type {
    ($tcp_stream:ident, $loop_name:tt) => {
        {
            let mut recv_type_byte: [u8; 1] = [0];
            if $tcp_stream.read_exact(&mut recv_type_byte).is_err() {
                continue $loop_name;
            }
            recv_type_byte[0] as char
        }
    };
}

fn pause_thread(tcp_stream: &mut TcpStream, x: u64, y: u64, z: u64) {
    'pause: loop {
        let mut recv_type: [u8; 1] = [0];
        let mut recv_command_bytes: [u8; 1] = [0];
        if tcp_stream.read_exact(&mut recv_type).is_ok()
            && tcp_stream.read(&mut recv_command_bytes)
            .is_ok() {
            if recv_type[0] as char == 'c'
                && recv_command_bytes[0] == 0 {
                let mut recv_command_byte: [u8; 1] = [0];
                if tcp_stream
                    .read_exact(&mut recv_command_byte)
                    .is_ok() {
                    if recv_command_byte[0] == 0 {
                        let xyz = [x, y, z];
                        let mut xyz_bytes = unsafe {
                            transmute_copy::<[u64; 3], [u8; 24]>(&xyz)
                        };
                        drop(tcp_stream.write_all(&mut ['a' as u8; 1]));
                        drop(tcp_stream.write_all(&mut xyz_bytes));
                    } else if recv_command_byte[0] == 2 {
                        break 'pause;
                    }
                }
            }
        }
    }
}

fn send_xyz(tcp_stream: &mut TcpStream, xyz: [u64; 3]) {
    let xyz_bytes = unsafe {
        transmute_copy::<[u64; 3], [u8; 24]>(&xyz)
    };
    drop(tcp_stream.write_all(&['a' as u8; 1]));
    drop(tcp_stream.write_all(&xyz_bytes));
}

macro_rules! spawn_tester_thread {
    ($tcp_addr:ident, $wrap_up_sender:ident, $local_inst_receiver:ident, $t_no:ident) => {
        thread::spawn(move || {
            let mut tcp_stream = TcpStream::connect($tcp_addr).unwrap();
            tcp_stream.set_read_timeout(Some(Duration::from_millis(5000))).expect("Was unable to \
            set read timeout.");
            //tcp_stream.set_nonblocking(true).expect("Couldn't set TCP stream as nonblocking.");
            println!("({}) Connected to host!", $t_no);
            'main_loop: loop {
                let x = {
                    // Write 't' to the TCP stream to request work
                    drop(tcp_stream.write_all(&['r' as u8; 1]));
                    // Get response
                    let mut recv_response: [u8; 1] = [0];
                    drop(tcp_stream.read_exact(&mut recv_response));
                    // The client should only care if it's t
                    if recv_response[0] as char == 't' {
                        println!("({}) Got unexpected terminate signal. Closing thread.", $t_no);
                        $wrap_up_sender.send(()).unwrap();
                        break 'main_loop;
                    }
                    // If it's anything else, just continue the main loop to re-request work.
                    if recv_response[0] as char != 'x' {
                        continue 'main_loop;
                    }
                    let mut tcp_recv_bytes: [u8; 8] = [0; 8];
                    tcp_stream.read_exact(&mut tcp_recv_bytes)
                        .expect("Was unable to read from TCP stream.");
                    unsafe {
                        transmute_copy::<[u8; 8], u64>(&tcp_recv_bytes)
                    }
                };
                for y in (2..x / 24).map(|y| y * 24) {
                    for z in (2..(x - y) / 24).map(|z| z * 24).take_while(|&z| z != y) {
                        if test_squares(x, y, z) {
                            drop(tcp_stream.write_all(&['s' as u8; 1]));
                            let xyz_bytes = unsafe {
                                transmute_copy::<[u64; 3], [u8; 24]>(&[x, y, z])
                            };
                            drop(tcp_stream.write_all(&xyz_bytes));
                        }
                        // If there's bytes to read on the TCP stream...
                        if tcp_stream.peek(&mut [0u8]).is_ok() {
                            // Assign the first byte here
                            let recv_type = recv_incoming_type!(tcp_stream, 'main_loop);
                            match recv_type {
                                'c' => { // Command. Will be 0, 1, or 2
                                    let mut command_type: [u8; 1] = [0];
                                    drop(tcp_stream.read_exact(&mut command_type));
                                    match command_type[0] {
                                        0 => send_xyz(&mut tcp_stream, [x, y, z]),
                                        1 => pause_thread(&mut tcp_stream, x, y, z),
                                        _ => {}
                                    }
                                }
                                't' => {
                                    println!("({}) Got terminate signal from host.", $t_no);
                                    $wrap_up_sender.send(()).unwrap();
                                    break 'main_loop
                                },
                                _ => {}
                            }
                        }
                        if let Ok(inst) = $local_inst_receiver.try_recv() {
                            match inst {
                                2 => {
                                    drop(tcp_stream.write_all(&['t' as u8; 1]));
                                    break 'main_loop;
                                },
                                0 => {
                                    println!("({}) Current XYZ | x: {}, y: {}, z: {}",
                                        $t_no, x, y, z);
                                },
                                _ => {}
                            }
                        }
                    }
                }
                if let Ok(1) = $local_inst_receiver.try_recv() {
                    drop(tcp_stream.write_all(&['t' as u8; 1]));
                    break 'main_loop;
                }
                thread::sleep_ms(1000);
            }
            println!("({}) Finishing execution!", $t_no);
        })
    };
}

// inst channel instruction table:
// 0: local progress query
// 1: soft terminate
// 2: hard terminate

fn main() {
    println!("Using maximum x value: {}", MAX_X);
    let tcp_addr: &'static str = "127.0.0.1:1337";
    let mut inst_senders = Vec::new();
    let mut wrap_up_receivers = Vec::new();
    let mut handles = Vec::new();
    for i in 0..NUM_THREADS {
        let (inst_sender, inst_receiver): (Sender<u8>, Receiver<u8>) = mpsc::channel();
        let (wrap_up_sender, wrap_up_receiver): (Sender<()>, Receiver<()>) = mpsc::channel();
        handles.push(spawn_tester_thread!(tcp_addr, wrap_up_sender, inst_receiver, i));
        inst_senders.push(inst_sender);
        wrap_up_receivers.push(wrap_up_receiver);
    }
    let (io_inst_sender, io_inst_receiver): (Sender<u8>, Receiver<u8>) = mpsc::channel();
    handles.push(spawn_manager_io_thread!(io_inst_sender));
    let mut active_threads = [true; NUM_THREADS];
    'manager_loop: loop {
        if let Ok(inst) = io_inst_receiver.try_recv() {
            match inst {
                0 => { // Progress query
                    for inst_sender in &inst_senders {
                        inst_sender.send(0).unwrap();
                    }
                },
                1 => { // Soft terminate
                    for inst_sender in &inst_senders {
                        inst_sender.send(1).unwrap();
                    }
                    while active_threads != [false; NUM_THREADS] {
                        for (i, receiver) in wrap_up_receivers.iter().enumerate() {
                            if active_threads[i] {
                                if let Ok(_) = receiver.try_recv() {
                                   active_threads[i] = false;
                                }
                            }
                        }
                    }
                    break 'manager_loop;
                },
                2 => { // Hard terminate
                    for inst_sender in &inst_senders {
                        inst_sender.send(2).unwrap();
                    }
                    break 'manager_loop;
                },
                _ => {}
            }
        }
        for (i, receiver) in wrap_up_receivers.iter().enumerate() {
            if let Ok(_) = receiver.try_recv() {
                active_threads[i] = false;
            }
        }
        if active_threads == [false; NUM_THREADS] {
            break 'manager_loop;
        }
    }
    println!("Closing up shop.");
}

// Using method found on SE
const GOOD_MASK: u64 = 0xC840C04048404040;

fn is_valid_square(mut n: u64) -> bool {
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