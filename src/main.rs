use std::sync::{mpsc::{Sender, Receiver}, Arc, Mutex};
use std::thread;

const MAX_X: u64 = 1_000_000;
const BUFFER_SIZE: usize = 1_000_000;

fn main() {
    let iter = (2..MAX_X).map(|x| x * x)
        .flat_map(move |x| (2..x).flat_map(move |y| (2..x - y).filter(move |&z| z != y)
            .map(move |z| (x, y, z))))
        .filter(|&(x, y, z)| all_valid(x, y, z));
    let arc_mut_iter = Arc::new(Mutex::new(iter));
    let mut handles = Vec::new();
    let main_start = std::time::SystemTime::now();
    for i in 0..2 {
        let am_iter = Arc::clone(&arc_mut_iter);
        let builder = thread::Builder::new().stack_size(24 * BUFFER_SIZE);
        let test_handle = builder.spawn(move || {
            let mut internal_buffer: [(u64, u64, u64); BUFFER_SIZE] =
                [(0, 0, 0); BUFFER_SIZE];
            for _ in 0..4 {
                {
                    let mut t_iter = am_iter.lock().unwrap();
                    println!("({}) Starting to pull numbers from the iterator.", i);
                    let start = std::time::SystemTime::now();
                    for pos in 0..BUFFER_SIZE {
                        internal_buffer[pos] = t_iter.next().expect("Iterator has no more values.");
                    }
                    println!("({}) Refilled the buffer with data. Time taken: {:?}", i,
                             start.elapsed().unwrap());
                }
                println!("({}) Starting to test data.", i);
                let test_start = std::time::SystemTime::now();
                for trip in internal_buffer.iter() {
                    if test_squares(trip.0, trip.1, trip.2) {
                        println!("haha what the fuck? {:?}", trip);
                    }
                }
                println!("({}) Completed testing. Time taken: {:?}", i, test_start.elapsed().unwrap());
            }
        }).unwrap();
        handles.push(test_handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
    println!("Total time: {:?}", main_start.elapsed().unwrap());
}

fn all_valid(x: u64, y: u64, z: u64) -> bool {
    (x + y) % 24 == 1 && (x - y - z) % 24 == 1 && (x + z) % 24 == 1
        && (x - y + z) % 24 == 1 && x % 24 == 1 && (x + y - z) % 24 == 1
        && (x - z) % 24 == 1 && (x + y + z) % 24 == 1 && (x - y) % 24 == 1
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