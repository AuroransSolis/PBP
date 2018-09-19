use std::sync::{mpsc::{self, Sender, Receiver, TryRecvError::*}};
use std::thread::{self, JoinHandle};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::mem::transmute_copy;
use std::time::Duration;

const MAX_X: u64 = 1000;
const NUM_THREADS: usize = 2;

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
            } else if recv_type[0] as char == 'h' {
                drop(tcp_stream.write_all(&['h' as u8; 1]));
            }
        }
    }
}

// Send an [x, y, z] over TCP
fn send_xyz(tcp_stream: &mut TcpStream, xyz: [u64; 3]) {
    let xyz_bytes = unsafe {
        transmute_copy::<[u64; 3], [u8; 24]>(&xyz)
    };
    drop(tcp_stream.write_all(&['a' as u8; 1]));
    drop(tcp_stream.write_all(&xyz_bytes));
}

const RECURSION_LIMIT: u8 = 5;

fn get_x_limited_recursion(tcp_stream: &mut TcpStream, wrap_up_sender: &Sender<()>, c_no: usize,
    recursion_count: u8) -> Result<u64, bool> {
    if recursion_count < RECURSION_LIMIT {
        // Write 't' to the TCP stream to request work
        drop(tcp_stream.write_all(&['r' as u8; 1]));
        // Get response
        let mut recv_response: [u8; 1] = [0];
        drop(tcp_stream.read_exact(&mut recv_response));
        let recv_response = recv_response[0] as char;
        match recv_response {
            'h' => {
                drop(tcp_stream.write_all(&['h' as u8; 1]));
                get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no, recursion_count + 1)
            }
            'x' => { // New x value
                let mut tcp_recv_bytes: [u8; 8] = [0; 8];
                tcp_stream.read_exact(&mut tcp_recv_bytes)
                    .expect("Was unable to read from TCP stream.");
                unsafe {
                    Ok(transmute_copy::<[u8; 8], u64>(&tcp_recv_bytes))
                }
            },
            't' => { // Terminate
                let mut next_byte: [u8; 1] = [0];
                drop(tcp_stream.read_exact(&mut next_byte));
                if next_byte[0] as char == 'f' { // Iterator finished!
                    println!("({}) Host iterator finished! Closing thread.", c_no);
                } else {
                    println!("({}) Got unexpected terminate signal. Closing thread.", c_no);
                }
                wrap_up_sender.send(()).unwrap();
                Err(true)
            },
            'c' => {
                let mut command_type: [u8; 1] = [0];
                drop(tcp_stream.read_exact(&mut command_type));
                // Explanation for recursive macro calls in the following match block:
                // The host may send any number of commands, followed by the response to the
                // data request. So, I need to make sure that the data is caught WITHOUT sending
                // more data requests. Hence the existence of the "nosend" branch of this macro.
                match command_type[0] {
                    0 => {
                        send_xyz(tcp_stream, ['e' as u64, 'r' as u64, 'r' as u64]);
                        get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no,
                            recursion_count + 1)
                    },
                    1 => {
                        pause_thread(tcp_stream, 'e' as u64, 'r' as u64, 'r' as u64);
                        get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no,
                            recursion_count + 1)
                    },
                    2 => get_x_limited_recursion(tcp_stream, wrap_up_sender, c_no,
                            recursion_count + 1),
                    n @ _ => panic!(format!("({}) Got 'c {}'. Wotfok?", c_no, n))
                }
            },
            n @ _ => panic!(format!("({}) Got incoming data indicator: {}. RIP networking.",
                                    c_no, n))
        }
    } else {
        panic!("({}) Recursion limit for getting new x value reached!", c_no);
    }
}

fn spawn_tester_thread(tcp_addr: &'static str, wrap_up_sender: Sender<()>, c_no: usize) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut tcp_stream = TcpStream::connect(tcp_addr).unwrap();
        tcp_stream.set_read_timeout(Some(Duration::from_millis(5000))).expect("Was unable to \
            set read timeout.");
        //tcp_stream.set_nonblocking(true).expect("Couldn't set TCP stream as nonblocking.");
        println!("({}) Connected to host!", c_no);
        let mut soft_term = false;
        'main_loop: loop {
            if soft_term {
                break 'main_loop;
            }
            // Get x value for testing
            let x = match get_x_limited_recursion(&mut tcp_stream, &wrap_up_sender, c_no, 0) {
                Ok(x) => x,
                Err(false) => continue 'main_loop,
                Err(true) => break 'main_loop
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
                        let recv_type = {
                            let mut recv_type_byte: [u8; 1] = [0];
                            drop(tcp_stream.read_exact(&mut recv_type_byte));
                            recv_type_byte[0] as char
                        };
                        println!("({}) Found type indicator on TCP stream... Got: {}", c_no, recv_type as char);
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
                                let mut term_type: [u8; 1] = [0];
                                drop(tcp_stream.read_exact(&mut term_type));
                                match term_type[0] as char {
                                    's' => {
                                        println!("({}) Got soft terminate signal from host.", c_no);
                                        soft_term = true;
                                    },
                                    'h' => {
                                        println!("({}) Got hard terminate signal from host.", c_no);
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
        }
        println!("({}) Finishing execution!", c_no);
    })
}

fn main() {
    println!("Using maximum x value: {}", MAX_X);
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