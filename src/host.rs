use std::io::{self, Read, prelude::*, stdout, stdin};
use std::net::{TcpListener, TcpStream};
use std::mem::transmute_copy;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::time::Duration;
use std::thread;
use std::io::ErrorKind::WouldBlock;

const MAX_X: u64 = 1000;
const TRY_RECV_DATA_TIMEOUT: u64 = 5;

macro_rules! spawn_manager_io {
    ($inst_sender:ident) => {
        thread::spawn(move || {
            'management_io: loop {
                let mut input_buffer = String::new();
                stdout().flush().unwrap();
                stdin().read_line(&mut input_buffer).unwrap();
                input_buffer = input_buffer.trim().to_owned();
                if input_buffer == "q" {
                    $inst_sender.send(3).unwrap();
                    println!("Sent terminate signal to main thread.");
                    break 'management_io;
                } else if input_buffer == "qp" {
                    $inst_sender.send(0).unwrap();
                    println!("Sent progress query signal to main thread.");
                } else if input_buffer == "pause" {
                    $inst_sender.send(1).unwrap();
                    println!("Sent pause signal to main thread.");
                } else if input_buffer == "play" {
                    $inst_sender.send(2).unwrap();
                    println!("Sent play signal to main thread.");
                } else if input_buffer == "nc" {
                    $inst_sender.send(4).unwrap();
                    println!("Sent query about number of connections to main thread.");
                } else if input_buffer == "h" {
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
                    Number of connections(nc): print out how many clients are currently connected.\n\
                        \tnum connections, number of connections\n\
                    Help(h): prints this dialog\n\
                        \th, help");
                }
            }
        })
    };
}

/*macro_rules! spawn_handler_thread {
    ($tcp_stream:ident, $term_signal_channel:ident) => {

    };
}*/

fn main() {
    let listener = TcpListener::bind("127.0.0.1:1337").unwrap();
    listener.set_nonblocking(true).expect("Could not set listener to be non-blocking.");
    let mut tcpstreams: Vec<TcpStream> = Vec::new();
    let timer = std::time::Instant::now();
    let mut iterator = (2..MAX_X).map(|x| x * x).filter(|&x| x % 24 == 1);
    let (inst_sender, inst_receiver): (Sender<u8>, Receiver<u8>)= channel();
    let terminal_manager = spawn_manager_io!(inst_sender);
    let mut send_term_signal = false;
    'main_loop: loop {
        if send_term_signal {
            for tcpstream in tcpstreams.iter_mut() {
                drop(tcpstream.write_all(&mut ['t' as u8; 1]));
            }
            println!("Wrote 't' to all connected TCP streams.");
            break 'main_loop;
        };
        if let Ok((tcpstream, _)) = listener.accept() {
            tcpstream.set_read_timeout(Some(Duration::from_millis(5000))).expect("Unable to set \
            write timeout on new connection.");
            println!("    Accepted new connection! {:?}", tcpstream.peer_addr());
            tcpstreams.push(tcpstream);
        }
        // Check for instruction on the IO channel
        if let Ok(instr) = inst_receiver.try_recv() {
            match instr {
                0 => { // Progress query
                    // Try to write a progress query to all connected streams
                    for tcpstream in tcpstreams.iter_mut() {
                        // Write 'c' for command
                        drop(tcpstream.write_all(&mut ['c' as u8; 1]));
                        // Write '0' for progress query command
                        drop(tcpstream.write_all(&mut [0; 1]));
                    }
                    println!("Wrote 'c 0' to all TCP streams.");
                },
                1 => { // Pause
                    for tcpstream in tcpstreams.iter_mut() {
                        // Write 'c' for command
                        drop(tcpstream.write_all(&mut ['c' as u8; 1]));
                        // Write '1' for pause command
                        drop(tcpstream.write_all(&mut [1; 1]));
                    }
                    println!("Wrote 'c 1' to all TCP streams.");
                },
                2 => { // Play
                    for tcpstream in tcpstreams.iter_mut() {
                        // Write 'c' for command
                        drop(tcpstream.write_all(&mut ['c' as u8; 1]));
                        // Write '2' for resume command
                        drop(tcpstream.write_all(&mut [2; 1]));
                    }
                    println!("Wrote 'c 2' to all TCP streams.");
                },
                3 => { // Terminate
                    send_term_signal = true;
                    println!("Set send_term_signal to true.");
                    continue 'main_loop;
                },
                4 => { // Query number of connections
                    println!("Current number of connections: {}", tcpstreams.len());
                },
                _ => {}
            }
        }
        if tcpstreams.len() == 0 {
            continue 'main_loop;
        }
        let mut still_connected = vec![true; tcpstreams.len()];
        // Try to receive instruction type from TcpStreams. If a read times out, assume the client
        // is no longer connected.
        'handle_insts: for (i, tcpstream) in tcpstreams.iter_mut().enumerate() {
            let mut recv_instr_byte: [u8; 1] = [0];
            if let Err(e) = tcpstream.read_exact(&mut recv_instr_byte) {
                if e.kind() != WouldBlock {
                    still_connected[i] = false;
                    println!("TCP stream {} disconnected.", i);
                }
                continue 'handle_insts;
            }
            let instr = recv_instr_byte[0] as char;
            match instr {
                'a' => {
                    let mut recv_at_bytes: [u8; 24] = [0; 24];
                    let tmr = std::time::Instant::now();
                    while let Err(e) = tcpstream.read_exact(&mut recv_at_bytes) {
                        if e.kind() != WouldBlock {
                            still_connected[i] = false;
                            println!("TCP stream {} disconnected.", i);
                            continue;
                        }
                        if tmr.elapsed().as_secs() > TRY_RECV_DATA_TIMEOUT {
                            println!("Was unable to receive XYZ from client {}.", i);
                            continue 'handle_insts;
                        }
                    }
                    let recv_at_xyz = unsafe {
                        transmute_copy::<[u8; 24], [u64; 3]>(&recv_at_bytes)
                    };
                    println!("Client {} progress at {:?} | x: {}, y: {}, z: {}", i, timer.elapsed(),
                             recv_at_xyz[0], recv_at_xyz[1], recv_at_xyz[2]);
                },
                'r' => {
                    if let Some(x) = iterator.next() {
                        // Write 'x' for new x
                        drop(tcpstream.write_all(&mut [b'x'; 1]));
                        // Get bytes for 'x'
                        let mut x_bytes = unsafe {
                            transmute_copy::<u64, [u8; 8]>(&x)
                        };
                        // Write bytes for 'x'
                        drop(tcpstream.write_all(&mut x_bytes));
                    } else {
                        send_term_signal = true;
                        break 'handle_insts;
                    }
                },
                's' => {
                    let mut recv_solution_bytes: [u8; 24] = [0; 24];
                    if let Err(e) = tcpstream.read_exact(&mut recv_solution_bytes) {
                        if e.kind() != WouldBlock {
                            still_connected[i] = false;
                            println!("TCP stream {} disconnected.", i);
                        }
                        continue 'handle_insts;
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
                    still_connected[i] = false;
                    continue;
                },
                _ => {}
            }
        }
        for (i, connected_tf) in still_connected.into_iter().enumerate() {
            if !connected_tf {
                tcpstreams.remove(i);
            }
        }
        thread::sleep_ms(1000);
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