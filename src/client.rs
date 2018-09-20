use std::sync::{mpsc::{self, Sender, Receiver, TryRecvError::*}};
use std::thread::{self, JoinHandle};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::mem::transmute_copy;

const MAX_X: u64 = 1000;
const NUM_THREADS: usize = 2;

const COMMAND: char = 'c';
const PROGRESS_QUERY: u8 = 0;
const PAUSE: u8 = 1;
const PLAY: u8 = 2;

const HEARTBEAT: char = 'h';

const TERMINATE: char = 't';
const SOFT: char = 's';
const HARD: char = 'h';
const FINISHED: char = 'f';
const SOFT_VAL: u8 = 0;
const HARD_VAL: u8 = 1;
const FINISHED_VAL: u8 = 2;

const INCOMING_X: char = 'x';
const PROGRESS_REPORT: char = 'a';
const REQUEST: char = 'r';
const SOLUTION: char = 's';

fn pause_thread(tcp_stream: &mut TcpStream, x: u64, y: u64, z: u64) -> Result<Option<u64>, u8> {
    let mut got_x = None;
    tcp_stream.set_nonblocking(false).expect("Was unable to set TCP stream as blocking for PAUSE.");
    'pause: loop {
        let mut recv_type: [u8; 1] = [0];
        drop(tcp_stream.read_exact(&mut recv_type));
        match recv_type[0] as char {
            COMMAND => {
                let mut recv_command_byte: [u8; 1] = [0];
                if tcp_stream.read_exact(&mut recv_command_byte).is_ok() {
                    if recv_command_byte[0] == PROGRESS_QUERY {
                        let xyz = [x, y, z];
                        let mut xyz_bytes = unsafe {
                            transmute_copy::<[u64; 3], [u8; 24]>(&xyz)
                        };
                        drop(tcp_stream.write_all(&mut ['a' as u8; 1]));
                        drop(tcp_stream.write_all(&mut xyz_bytes));
                    } else if recv_command_byte[0] == PLAY {
                        break 'pause;
                    }
                }
            },
            HEARTBEAT => {
                drop(tcp_stream.write_all(&[HEARTBEAT as u8; 1]));
            },
            TERMINATE => {
                let mut term_type: [u8; 1] = [0];
                drop(tcp_stream.read_exact(&mut term_type));
                match term_type[0] as char {
                    SOFT => return Err(SOFT_VAL),
                    HARD => return Err(HARD_VAL),
                    FINISHED => return Err(FINISHED_VAL),
                    _ => {}
                }
            },
            INCOMING_X => {
                let mut bytes: [u8; 8] = [0; 8];
                drop(tcp_stream.read_exact(&mut bytes));
                let new_x = unsafe {
                    transmute_copy::<[u8; 8], u64>(&bytes)
                };
                got_x = Some(new_x);
            },
            _ => {}
        }
    }
    Ok(got_x)
}

// Send an [x, y, z] over TCP
fn send_xyz(tcp_stream: &mut TcpStream, xyz: [u64; 3]) {
    let xyz_bytes = unsafe {
        transmute_copy::<[u64; 3], [u8; 24]>(&xyz)
    };
    drop(tcp_stream.write_all(&[PROGRESS_REPORT as u8; 1]));
    drop(tcp_stream.write_all(&xyz_bytes));
}

const RECURSION_LIMIT: u8 = 5;

fn get_x_limited_recursion(tcp_stream: &mut TcpStream, wrap_up_sender: &Sender<()>, c_no: usize,
    recursion_count: u8) -> Result<u64, ()> {
    if recursion_count < RECURSION_LIMIT {
        let mut recv_response: [u8; 1] = [0];
        drop(tcp_stream.read_exact(&mut recv_response));
        let recv_response = recv_response[0] as char;
        match recv_response {
            HEARTBEAT => {
                // In the case that the client receives a heartbeat request after a data request,
                // the host ends up using the data request as the heartbeat response. Send another
                // data request to rectify the situation.
                drop(tcp_stream.write_all(&[REQUEST as u8; 1]));
                get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no, recursion_count + 1)
            },
            INCOMING_X => {
                let mut tcp_recv_bytes: [u8; 8] = [0; 8];
                tcp_stream.read_exact(&mut tcp_recv_bytes)
                    .expect("Was unable to read from TCP stream.");
                unsafe {
                    Ok(transmute_copy::<[u8; 8], u64>(&tcp_recv_bytes))
                }
            },
            TERMINATE => {
                let mut next_byte: [u8; 1] = [0];
                drop(tcp_stream.read_exact(&mut next_byte));
                if next_byte[0] as char == FINISHED { // Iterator finished!
                    println!("({}: gxlr) Host iterator finished! Closing thread.", c_no);
                } else {
                    println!("({}: gxlr) Got unexpected terminate signal. Closing thread.", c_no);
                }
                wrap_up_sender.send(()).unwrap();
                Err(())
            },
            COMMAND => {
                let mut command_type: [u8; 1] = [0];
                drop(tcp_stream.read_exact(&mut command_type));
                match command_type[0] {
                    PROGRESS_QUERY => {
                        send_xyz(tcp_stream, ['e' as u64, 'r' as u64, 'r' as u64]);
                        get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no,
                            recursion_count + 1)
                    },
                    PAUSE => {
                        let pause_result = pause_thread(tcp_stream, 'e' as u64, 'r' as u64,
                            'r' as u64);
                        if let Err(_) = pause_result {
                            Err(())
                        } else if let Ok(Some(new_x)) = pause_result {
                            Ok(new_x)
                        } else {
                            get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no,
                            recursion_count + 1)
                        }
                    },
                    PLAY => get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no,
                            recursion_count + 1),
                    n @ _ => panic!(format!("({}) Got 'c {}'. Wotfok?", c_no, n))
                }
            },
            _ => get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no, recursion_count + 1)
        }
    } else {
        panic!("({} get x) Recursion limit for getting new x value reached!", c_no);
    }
}

fn spawn_tester_thread(tcp_addr: &'static str, wrap_up_sender: Sender<()>, c_no: usize) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut tcp_stream = TcpStream::connect(tcp_addr).unwrap();
        println!("({}) Connected to host!", c_no);
        let mut soft_term = false;
        'main_loop: loop {
            if soft_term {
                break 'main_loop;
            }
            // Get x value for testing
            drop(tcp_stream.write_all(&[REQUEST as u8; 1]));
            tcp_stream.set_nonblocking(false).expect("Couldn't set TCP stream as nonblocking \
            before getting X value.");
            let x = match get_x_limited_recursion(&mut tcp_stream, &wrap_up_sender, c_no, 0) {
                Ok(x) => x,
                Err(()) => break 'main_loop
            };
            //println!("got x");
            tcp_stream.set_nonblocking(true).expect("Couldn't set TCP stream as nonblocking \
            before beginning XYZ testing.");
            for y in (2..x / 24).map(|y| y * 24) {
                for z in (2..(x - y) / 24).map(|z| z * 24).take_while(|&z| z != y) {
                    if test_squares(x, y, z) {
                        drop(tcp_stream.write_all(&[SOLUTION as u8; 1]));
                        let xyz_bytes = unsafe {
                            transmute_copy::<[u64; 3], [u8; 24]>(&[x, y, z])
                        };
                        drop(tcp_stream.write_all(&xyz_bytes));
                    }
                    // If there's bytes to read on the TCP stream...
                    let recv_type = {
                        let mut recv_type_byte: [u8; 1] = [0];
                        drop(tcp_stream.read_exact(&mut recv_type_byte));
                        recv_type_byte[0] as char
                    };
                    match recv_type {
                        HEARTBEAT => drop(tcp_stream.write_all(&[HEARTBEAT as u8; 1])),
                        COMMAND => {
                            let mut command_type: [u8; 1] = [0];
                            drop(tcp_stream.read_exact(&mut command_type));
                            match command_type[0] {
                                PROGRESS_QUERY => send_xyz(&mut tcp_stream, [x, y, z]),
                                PAUSE => {
                                    let pause_result = pause_thread(&mut tcp_stream, x, y, z);
                                    if let Err(ty) = pause_result {
                                        match ty {
                                            SOFT_VAL => soft_term = true,
                                            HARD_VAL => break 'main_loop,
                                            FINISHED_VAL => soft_term = true,
                                            _ => {}
                                        }
                                    }
                                },
                                _ => {}
                            }
                        }
                        TERMINATE => {
                            println!("({}) Got terminate signal from host.", c_no);
                            let mut term_type: [u8; 1] = [0];
                            drop(tcp_stream.read_exact(&mut term_type));
                            match term_type[0] as char {
                                SOFT => {
                                    println!("({}) Terminate signal type: SOFT.", c_no);
                                    soft_term = true;
                                },
                                HARD => {
                                    println!("({}) Terminate signal type: HARD.", c_no);
                                    wrap_up_sender.send(()).unwrap();
                                    break 'main_loop;
                                },
                                _ => {}
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
        drop(tcp_stream.write_all(&[TERMINATE as u8; 1]));
        println!("({}) Finishing execution!", c_no);
    })
}

fn main() {
    println!("Using maximum x value: {}", MAX_X * MAX_X);
    let tcp_addr: &'static str = "127.0.0.1:1337";
    let mut wrap_up_receivers = Vec::new();
    let mut handles = Vec::new();
    for i in 0..NUM_THREADS {
        let (wrap_up_sender, wrap_up_receiver): (Sender<()>, Receiver<()>) = mpsc::channel();
        handles.push(spawn_tester_thread(tcp_addr, wrap_up_sender, i));
        wrap_up_receivers.push(wrap_up_receiver);
    }
    let mut active_threads = [true; NUM_THREADS];
    'manager_loop: loop {
        for (i, receiver) in wrap_up_receivers.iter().enumerate() {
            match receiver.try_recv() {
                Ok(_) => active_threads[i] = false,
                Err(Disconnected) => active_threads[i] = false,
                _ => {}
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
