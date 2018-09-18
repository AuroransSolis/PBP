use std::sync::{mpsc::{self, Sender, Receiver, TryRecvError::*}};
use std::thread::{self, JoinHandle};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::mem::transmute_copy;
use std::time::Duration;

const MAX_X: u64 = 1000;
const NUM_THREADS: usize = 4;

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

// Send an [x, y, z] over TCP
fn send_xyz(tcp_stream: &mut TcpStream, xyz: [u64; 3]) {
    let xyz_bytes = unsafe {
        transmute_copy::<[u64; 3], [u8; 24]>(&xyz)
    };
    drop(tcp_stream.write_all(&['a' as u8; 1]));
    drop(tcp_stream.write_all(&xyz_bytes));
}

// Deep breaths. Deep breaths. I know this is deeply wrong, but please bear with me. I need limited
// recursion, which is something I can't achieve with normal macros and just, say, a "recursion
// counter" variable that gets passed in as an identifier. So, I forced "identifiers" by having a
// linear progression through a list of macros with a given "label" at the start, like 'a', 'b', or
// 'c', where macro!(a, other_args) calls macro!(b, other_args) in the case where macro!() would
// recurse, and macro!(b, other_args) calls macro!(c, other_args) in the same case. However, in
// this case, 'c' is the last "label" in the series, so any case where macro!() would recurse
// results in a panic. This macro provides a way of generating that "limited recursion." In the
// case of how I've used it, sorta_recursive_get_x!(a, other_args) is the entry point for the
// limited recursion I need to get an x value after a data request in the case that the host sends
// a command after the data request is sent and before the data is received by the client.
macro_rules! gen_sorta_recursive_get_x_macro {
    ($($start_label:tt => $points_to:tt),*; $last_label:tt) => {
        macro_rules! sorta_recursive_get_x {
            $(
            ($start_label $tcp_strm:ident, $wus:ident, $cno:ident, $ml:tt) => {
                {
                    let mut recv_response: [u8; 1] = [0];
                    drop($tcp_strm.read_exact(&mut recv_response));
                    let recv_response = recv_response[0] as char;
                    match recv_response {
                        'h' => { // Heartbeat
                            drop($tcp_strm.write_all(&['h' as u8; 1]));
                            sorta_recursive_get_x!($points_to $tcp_strm, $wus, $cno, $ml)
                        },
                        'x' => { // New x value
                            let mut tcp_recv_bytes: [u8; 8] = [0; 8];
                            $tcp_strm.read_exact(&mut tcp_recv_bytes)
                                .expect("Was unable to read from TCP stream.");
                            unsafe {
                                transmute_copy::<[u8; 8], u64>(&tcp_recv_bytes)
                            }
                        },
                        't' => { // Terminate
                            let mut next_byte: [u8; 1] = [0];
                            drop($tcp_strm.read_exact(&mut next_byte));
                            if next_byte[0] as char == 'f' { // Iterator finished!
                                println!("({}) Host iterator finished! Exiting.", $cno);
                            } else {
                                println!("({}) Got unexpected terminate signal. Exiting.",
                                    $cno);
                            }
                            drop($tcp_strm.write_all(&['t' as u8; 1]));
                            $wus.send(()).unwrap();
                            break $ml;
                        },
                        'c' => {
                            let mut command_type: [u8; 1] = [0];
                            drop($tcp_strm.read_exact(&mut command_type));
                            match command_type[0] {
                                0 => {
                                    send_xyz(&mut $tcp_strm, ['e' as u64, 'r' as u64,
                                        'r' as u64]);
                                    sorta_recursive_get_x!($points_to $tcp_strm, $wus, $cno, $ml)
                                },
                                1 => {
                                    pause_thread(&mut $tcp_strm, 'e' as u64, 'r' as u64,
                                        'r' as u64);
                                    sorta_recursive_get_x!($points_to $tcp_strm, $wus, $cno, $ml)
                                },
                                2 => sorta_recursive_get_x!($points_to $tcp_strm, $wus, $cno, $ml),
                                n @ _ => panic!(format!("({}) Got 'c {}'. Wotfok?", $cno, n))
                            }
                        },
                        n @ _ => panic!(format!("({}) Got incoming data indicator: {}.\
                        Networking's totally fucked :^)",
                                                $cno, n))
                    }
                }
            };
            )*
            ($last_label $tcp_strm:ident, $wus:ident, $cno:ident, $ml:tt) => {
                {
                    let mut recv_response: [u8; 1] = [0];
                    drop($tcp_strm.read_exact(&mut recv_response));
                    let recv_response = recv_response[0] as char;
                    match recv_response {
                        'h' => { // Heartbeat
                            drop($tcp_strm.write_all(&['t' as u8; 1]));
                            panic!(format!("({}) 'get_x!' recursion limit reached!", $cno));
                        },
                        'x' => { // New x value
                            let mut tcp_recv_bytes: [u8; 8] = [0; 8];
                            $tcp_strm.read_exact(&mut tcp_recv_bytes)
                                .expect("Was unable to read from TCP stream.");
                            unsafe {
                                transmute_copy::<[u8; 8], u64>(&tcp_recv_bytes)
                            }
                        },
                        't' => { // Terminate
                            let mut next_byte: [u8; 1] = [0];
                            drop($tcp_strm.read_exact(&mut next_byte));
                            if next_byte[0] as char == 'f' { // Iterator finished!
                                println!("({}) Host iterator finished! Exiting.", $cno);
                            } else {
                                println!("({}) Got unexpected terminate signal. Exiting.",
                                    $cno);
                            }
                            drop($tcp_strm.write_all(&['t' as u8; 1]));
                            $wus.send(()).unwrap();
                            break $ml;
                        },
                        'c' => {
                            let mut command_type: [u8; 1] = [0];
                            drop($tcp_strm.read_exact(&mut command_type));
                            match command_type[0] {
                                0 => {
                                    drop($tcp_strm.write_all(&['t' as u8; 1]));
                                    $wus.send(()).unwrap();
                                    panic!(format!("({}) 'get_x!' recursion limit reached!", $cno));
                                },
                                1 => {
                                    drop($tcp_strm.write_all(&['t' as u8; 1]));
                                    $wus.send(()).unwrap();
                                    panic!(format!("({}) 'get_x!' recursion limit reached!", $cno));
                                },
                                2 => {
                                    drop($tcp_strm.write_all(&['t' as u8; 1]));
                                    $wus.send(()).unwrap();
                                    panic!(format!("({}) 'get_x!' recursion limit reached!", $cno));
                                },
                                n @ _ => {
                                    $wus.send(()).unwrap();
                                    panic!(format!("({}) Got 'c {}'. Wotfok?", $cno, n));
                                }
                            }
                        },
                        n @ _ => panic!(format!("({}) Got incoming data indicator: {}.\
                        Networking's totally fucked :^)",
                                                $cno, n))
                    }
                }
            }
        }
    };
}

gen_sorta_recursive_get_x_macro!{
    a => b,
    b => c,
    c => d;
    d
}

// Feel free to take a deep breath and prepare for disappointment.
macro_rules! get_x {
    ($tcp_stream:ident, $wrap_up_sender:ident, $c_no:ident, $main_loop:tt) => {
        {
            // Write 't' to the TCP stream to request work
            drop($tcp_stream.write_all(&['r' as u8; 1]));
            // Get response
            let mut recv_response: [u8; 1] = [0];
            drop($tcp_stream.read_exact(&mut recv_response));
            let recv_response = recv_response[0] as char;
            match recv_response {
                'h' => {
                    drop($tcp_stream.write_all(['h' as u8; 1]));
                    sorta_recursive_get_x!(a $tcp_stream, $wrap_up_sender, $c_no, $main_loop)
                }
                'x' => { // New x value
                    let mut tcp_recv_bytes: [u8; 8] = [0; 8];
                    $tcp_stream.read_exact(&mut tcp_recv_bytes)
                        .expect("Was unable to read from TCP stream.");
                    unsafe {
                        transmute_copy::<[u8; 8], u64>(&tcp_recv_bytes)
                    }
                },
                't' => { // Terminate
                    let mut next_byte: [u8; 1] = [0];
                    drop($tcp_stream.read_exact(&mut next_byte));
                    if next_byte[0] as char == 'f' { // Iterator finished!
                        println!("({}) Host iterator finished! Closing thread.", $c_no);
                    } else {
                        println!("({}) Got unexpected terminate signal. Closing thread.", $c_no);
                    }
                    $wrap_up_sender.send(()).unwrap();
                    break $main_loop;
                },
                'c' => {
                    let mut command_type: [u8; 1] = [0];
                    drop($tcp_stream.read_exact(&mut command_type));
                    // Explanation for recursive macro calls in the following match block:
                    // The host may send any number of commands, followed by the response to the
                    // data request. So, I need to make sure that the data is caught WITHOUT sending
                    // more data requests. Hence the existence of the "nosend" branch of this macro.
                    match command_type[0] {
                        0 => {
                            send_xyz(&mut $tcp_stream, ['e' as u64, 'r' as u64, 'r' as u64]);
                            sorta_recursive_get_x!(a $tcp_stream, $wrap_up_sender, $c_no,
                                $main_loop)
                        },
                        1 => {
                            pause_thread(&mut $tcp_stream, 'e' as u64, 'r' as u64, 'r' as u64);
                            sorta_recursive_get_x!(a $tcp_stream, $wrap_up_sender, $c_no,
                                $main_loop)
                        },
                        2 => sorta_recursive_get_x!(a $tcp_stream, $wrap_up_sender, $c_no,
                                $main_loop),
                        n @ _ => panic!(format!("({}) Got 'c {}'. Wotfok?", $c_no, n))
                    }
                },
                n @ _ => panic!(format!("({}) Got incoming data indicator: {}. RIP networking.",
                                        $c_no, n))
            }
        }
    };
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
            let x = get_x!(tcp_stream, wrap_up_sender, c_no, 'main_loop);
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
    let tcp_addr: &'static str = "169.254.36.152:1337";
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