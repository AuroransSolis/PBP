use std::sync::{Arc, Mutex, mpsc, mpsc::{Sender, Receiver}};
use std::thread;
use std::iter::Iterator;

const MAX_X: u64 = 750;
const NUM_THREADS: usize = 8;

struct TesterThreadBuilder {
    inst_channel: Receiver<u8>,
    res_channel: Sender<TTResult>
}

#[derive(Debug)]
enum TTResult {
    Solution((u64, u64, u64)),
    At((u64, u64, u64))
}

impl TesterThreadBuilder {
    fn new(ic: Receiver<u8>, rsc: Sender<TTResult>) -> TesterThreadBuilder {
        TesterThreadBuilder {
            inst_channel: ic,
            res_channel: rsc
        }
    }
}

macro_rules! spawn_tester_thread {
    ($ttb:ident, $arc_mutex_iterator:ident, $t_no:ident) => {
        thread::spawn(move || {
            'main_loop: loop {
                let x = {
                    let mut tmp = $arc_mutex_iterator.lock().unwrap();
                    if let Some(num) = tmp.next() {
                        num
                    } else {
                        break 'main_loop;
                    }
                };
                for y in (2..x / 24).map(|y| y * 24) {
                    for z in (2..(x - y) / 24).map(|z| z * 24).take_while(|&z| z != y) {
                        if let Ok(num) = $ttb.inst_channel.try_recv() {
                            match num {
                                0 => $ttb.res_channel.send(TTResult::At((x, y, z))).unwrap(),
                                1 => 'pause: loop {
                                    match $ttb.inst_channel.try_recv() {
                                        Ok(0) => $ttb.res_channel.send(TTResult::At((x, y, z)))
                                            .unwrap(),
                                        Ok(2) => break 'pause,
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                        if test_squares(x, y, z) {
                            $ttb.res_channel.send(TTResult::Solution((x, y, z))).unwrap();
                        }
                    }
                }
            }
            println!("({}) Completed execution!", $t_no);
        })
    };
}

fn main() {
    println!("Using maximum x value: {}", MAX_X);
    let mut handles = Vec::new();
    let mut inst_senders = Vec::new();
    let mut res_receivers = Vec::new();
    let am_iter = Arc::new(Mutex::new((2..MAX_X).map(|x| x * x).filter(|&x| x % 24 == 1)));
    for i in 0..NUM_THREADS {
        let (inst_sender, inst_receiver): (Sender<u8>, Receiver<u8>) = mpsc::channel();
        let (res_sender, res_receiver): (Sender<TTResult>, Receiver<TTResult>) = mpsc::channel();
        let tt = TesterThreadBuilder::new(inst_receiver, res_sender);
        let am_ref = Arc::clone(&am_iter);
        handles.push(spawn_tester_thread!(tt, am_ref, i));
        inst_senders.push(inst_sender);
        res_receivers.push(res_receiver);
    }
    println!("Constructed testing threads.");
    let total_time = std::time::Instant::now();
    let wait_time = 30; // Time to wait
    let mut wait_counter = 1;
    'progress_checker: loop {
        // Wait wait_time seconds, then query progress
        thread::sleep_ms(wait_time * 1000);
        // Query progress on all threads
        for inst_sender in &inst_senders {
            inst_sender.send(0);
        }
        let mut errs = 0;
        // Receive responses from threads
        for (i, res_receiver) in res_receivers.iter().enumerate() {
            match res_receiver.try_recv() {
                Ok(t) => println!("Thread {} at {} seconds: {:?}", i, wait_time * wait_counter, t),
                Err(_) => errs += 1
            }
        }
        if errs == NUM_THREADS {
            break 'progress_checker;
        }
        wait_counter += 1;
    }
    // Wrap it all up
    for handle in handles {
        handle.join().unwrap();
    }
    println!("Total time: {:?}", total_time.elapsed());
    println!("Exiting.");
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